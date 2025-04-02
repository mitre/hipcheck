// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Result,
	hc_error,
	target::{Package, PackageHost, RemoteGitRepo, SingleTargetSeed, SingleTargetSeedKind},
};

use gomod_rs::{Context as GmContext, Directive};
use url::Url;

use std::{fs::File, ops::Deref, path::Path};

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

pub(crate) async fn resolve_package_lock_json(path: &Path) -> Result<Vec<SingleTargetSeed>> {
	// Parse package-lock.json
	let file =
		File::open(path).map_err(|e| hc_error!("Failed to load package-lock.json: {}", e))?;
	let package_lock: crate::target::PackageLockJson = serde_json::from_reader(file)
		.map_err(|e| hc_error!("Failed to parse package-lock.json target seed: {}", e))?;

	// Extract dependencies from file
	let dependencies: Vec<SingleTargetSeed> = package_lock
		.dependencies
		.iter()
		.flat_map(|dependencies| {
			dependencies.iter().map(|(name, dependency)| {
				// Map dependency to SingleTargetSeedKind::Package

				// println!("{}", name.clone());
				// let url = Url::parse(&dependency.resolved.clone().unwrap())
				// 	.map_err(|e| hc_error!("URL parse failed: {}", e))?;
				let name = name.to_string();
				let version = dependency.version.clone();
				let specifier = format!("{name} {version}");
				// Original
				// Ok(SingleTargetSeed {
				// 	kind: SingleTargetSeedKind::Package(Package {
				// 		purl: url,
				// 		name,
				// 		version: version.clone(),
				// 		host: PackageHost::Npm,
				// 	}),
				// 	refspec: Some(version),
				// 	specifier,
				// })
				
				// NPM Test duplicate
				// If the package is scoped, replace the leading '@' in the scope with %40 for proper pURL formatting
				let purl = Url::parse(&match version.as_str() {
					"no version" => format!("pkg:npm/{}", str::replace(&name, '@', "%40")),
					_ => format!(
						"pkg:npm/{}@{}",
						str::replace(&name, '@', "%40"),
						version.clone()
					),
				})
				.unwrap();

				Ok(SingleTargetSeed {
					kind: SingleTargetSeedKind::Package(Package {
						purl,
						name,
						version: version.clone(),
						host: PackageHost::Npm,
					}),
					refspec: None,
					specifier, 
				})
			})
		})
		.collect::<Result<Vec<SingleTargetSeed>>>()?;
	Ok(dependencies)
}
