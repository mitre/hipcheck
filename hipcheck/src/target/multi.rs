// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Result,
	hc_error,
	target::{RemoteGitRepo, SingleTargetSeed, SingleTargetSeedKind},
};

use gomod_rs::{Context as GmContext, Directive};
use url::Url;

use std::{ops::Deref, path::Path};

pub(crate) async fn resolve_go_mod(path: &Path) -> Result<Vec<SingleTargetSeed>> {
	let raw_content = tokio::fs::read_to_string(path)
		.await
		.map_err(|e| hc_error!("Failed to load go.mod target seed: {}", e))?;
	let gomod = gomod_rs::parse_gomod(&raw_content)
		.map_err(|e| hc_error!("go.mod parsing failed: {}", e))?;
	// Extract the dependencies list from the fle
	let dependencies = gomod
		.iter()
		.filter_map(|x| match x {
			GmContext {
				value: Directive::Require { specs },
				..
			} => Some(specs),
			_ => None,
		})
		.flatten()
		// Map single dependency spec to a TargetSeed
		.map(|x| {
			let repo = x.value.0;
			let url = Url::parse(repo).map_err(|e| hc_error!("URL parse failed: {}", e))?;
			let version: String = x.value.1.deref().to_owned();
			let specifier = format!("{repo} {version}");
			Ok(SingleTargetSeed {
				kind: SingleTargetSeedKind::RemoteRepo(RemoteGitRepo {
					url,
					known_remote: None,
				}),
				refspec: Some(version),
				specifier,
			})
		})
		.collect::<Result<Vec<SingleTargetSeed>>>()?;
	Ok(dependencies)
}
