use anyhow::{Context as _, Result};
use nv_common::{
	config::Config,
	cve::kev::{
		KevAffectedNpmPackageVersion, KevNpmMatchStatus, ReachableNpmPackageVersion,
		kev_affected_npm_package_versions,
	},
	db,
	npm::elaboration::storage::persisted_package_versions,
	rt,
};
use serde::Serialize;

use crate::cmd::package_version::kev::{KevMatchOutput, reachable_npm_package_version_from_purl};

pub fn command() -> clap::Command {
	clap::Command::new("kevs")
		.about("List KEV-linked matches for versions resolved from a source")
		.arg(source_id_argument())
		.arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let source_id = matches
		.get_one::<String>("source-id")
		.expect("required source ID");
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
	let result = runtime.block_on(kevs(config, source_id))?;

	if matches.get_flag("json") {
		println!("{}", json_output(source_id, &result));
	} else {
		print_matches(source_id, &result);
	}

	Ok(())
}

async fn kevs(config: &Config, source_id: &str) -> Result<Vec<SourceKevMatch>> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let source = super::source_by_id(&db, source_id).await?;
	let versions = persisted_package_versions(&db, source.id)
		.await
		.context("failed to read resolved package versions")?;
	let resolved = versions
		.into_iter()
		.map(|version| {
			let purl = version.package_url;
			let package = reachable_npm_package_version_from_purl(&purl)
				.with_context(|| format!("stored package PURL is invalid: {purl}"))?;
			Ok(ResolvedPackage {
				purl: purl.clone(),
				package: ReachableNpmPackageVersion {
					source_evidence: format!("package source {source_id} resolved {purl}"),
					..package
				},
			})
		})
		.collect::<Result<Vec<_>>>()?;
	let packages = resolved
		.iter()
		.map(|resolved| resolved.package.clone())
		.collect::<Vec<_>>();
	let matched = kev_affected_npm_package_versions(&db, &packages)
		.await
		.context("failed to match resolved versions against KEV-linked CVEs")?;

	let mut results = Vec::new();
	for matched in matched {
		match matched.status {
			KevNpmMatchStatus::Affected => {
				let Some(package_name) = matched.package_name.as_deref() else {
					continue;
				};
				let Some(version) = matched.affected_version.as_deref() else {
					continue;
				};
				if let Some(resolved) = resolved.iter().find(|resolved| {
					resolved.package.package_name == package_name
						&& resolved.package.version == version
				}) {
					results.push(SourceKevMatch {
						purl: resolved.purl.clone(),
						matched,
					});
				}
			}
			KevNpmMatchStatus::Unknown => {
				let Some(package_name) = matched.package_name.as_deref() else {
					continue;
				};
				for resolved in resolved
					.iter()
					.filter(|resolved| resolved.package.package_name == package_name)
				{
					results.push(SourceKevMatch {
						purl: resolved.purl.clone(),
						matched: matched.clone(),
					});
				}
			}
		}
	}
	results.sort_by(|left, right| {
		left.purl
			.cmp(&right.purl)
			.then(left.matched.cve_id.cmp(&right.matched.cve_id))
			.then(
				left.matched
					.affected_version
					.cmp(&right.matched.affected_version),
			)
	});
	Ok(results)
}

struct ResolvedPackage {
	purl: String,
	package: ReachableNpmPackageVersion,
}

#[derive(Clone)]
struct SourceKevMatch {
	purl: String,
	matched: KevAffectedNpmPackageVersion,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceKevMatchOutput<'a> {
	purl: &'a str,
	#[serde(flatten)]
	matched: KevMatchOutput<'a>,
}

fn json_output(source_id: &str, matches: &[SourceKevMatch]) -> serde_json::Value {
	let matches = matches
		.iter()
		.map(|source_match| SourceKevMatchOutput {
			purl: &source_match.purl,
			matched: KevMatchOutput::from(&source_match.matched),
		})
		.collect::<Vec<_>>();
	serde_json::json!({ "sourceId": source_id, "matches": matches })
}

fn print_matches(source_id: &str, matches: &[SourceKevMatch]) {
	println!("source_id: {source_id}");
	if matches.is_empty() {
		println!("matches: <none>");
		println!("No KEV-linked CVE match was found in locally available data.");
		println!("This does not mean the resolved versions are safe.");
		return;
	}

	println!("matches: {}", matches.len());
	for source_match in matches {
		let matched = &source_match.matched;
		println!(
			"{} {} status={} confidence={}",
			source_match.purl,
			matched.cve_id,
			match matched.status {
				KevNpmMatchStatus::Affected => "affected",
				KevNpmMatchStatus::Unknown => "unknown",
			},
			match matched.confidence {
				nv_common::cve::kev::KevNpmMatchConfidence::High => "high",
				nv_common::cve::kev::KevNpmMatchConfidence::Unknown => "unknown",
			},
		);
		println!(
			"  kev: vendor_project={} product={} vulnerability_name={} date_added={}",
			matched
				.kev_context
				.vendor_project
				.as_deref()
				.unwrap_or("<none>"),
			matched.kev_context.product.as_deref().unwrap_or("<none>"),
			matched
				.kev_context
				.vulnerability_name
				.as_deref()
				.unwrap_or("<none>"),
			matched
				.kev_context
				.date_added
				.as_deref()
				.unwrap_or("<none>"),
		);
		print_values("evidence", &matched.source_evidence);
		print_values("caveats", &matched.caveats);
	}
}

fn print_values(label: &str, values: &[String]) {
	if values.is_empty() {
		println!("  {label}: <none>");
		return;
	}
	println!("  {label}:");
	for value in values {
		println!("    - {value}");
	}
}

fn source_id_argument() -> clap::Arg {
	clap::Arg::new("source-id")
		.required(true)
		.value_name("SOURCE-ID")
		.help("Stored package-source identifier")
}

fn json_argument() -> clap::Arg {
	clap::Arg::new("json")
		.long("json")
		.action(clap::ArgAction::SetTrue)
		.help("Print KEV-linked matches as JSON")
}

#[cfg(test)]
mod tests {
	use super::{SourceKevMatch, command, json_output};
	use nv_common::cve::kev::{
		KevAffectedNpmPackageVersion, KevContext, KevNpmMatchConfidence, KevNpmMatchStatus,
	};

	#[test]
	fn kevs_accepts_a_source_id() {
		command()
			.try_get_matches_from(["kevs", "source-1", "--json"])
			.expect("package-source kevs should parse");
	}

	#[test]
	fn kevs_json_output_includes_the_resolved_purl_and_match_context() {
		let output = json_output(
			"source-1",
			&[SourceKevMatch {
				purl: "pkg:npm/example@1.2.3".to_owned(),
				matched: KevAffectedNpmPackageVersion {
					cve_id: "CVE-2026-0001".to_owned(),
					kev_context: KevContext {
						cve_id: "CVE-2026-0001".to_owned(),
						vendor_project: Some("example".to_owned()),
						product: None,
						vulnerability_name: None,
						date_added: None,
					},
					package_name: Some("example".to_owned()),
					affected_version: Some("1.2.3".to_owned()),
					source_evidence: vec!["CVE record".to_owned()],
					confidence: KevNpmMatchConfidence::High,
					caveats: vec!["range matched".to_owned()],
					status: KevNpmMatchStatus::Affected,
				},
			}],
		);

		assert_eq!(output["sourceId"], "source-1");
		assert_eq!(output["matches"][0]["purl"], "pkg:npm/example@1.2.3");
		assert_eq!(output["matches"][0]["cveId"], "CVE-2026-0001");
		assert_eq!(output["matches"][0]["confidence"], "high");
	}
}
