use anyhow::{Context as _, Result};
use nv_common::{
	config::Config, db, db::entities::packages, hipcheck::storage::list_hipcheck_runs_for_package,
	npm::types::NpmPackageName, rt,
};
use percent_encoding::percent_decode_str;
use sea_orm::{ColumnTrait as _, EntityTrait as _, QueryFilter as _};
use std::str::FromStr as _;

const DEFAULT_LIMIT: u64 = 10;

pub fn command() -> clap::Command {
	clap::Command::new("runs")
		.about("List persisted assessments for a package")
		.arg(
			clap::Arg::new("package")
				.long("package")
				.required(true)
				.value_name("PURL")
				.help("Package URL without a version"),
		)
		.arg(
			clap::Arg::new("limit")
				.long("limit")
				.value_name("N")
				.default_value(DEFAULT_LIMIT.to_string())
				.value_parser(clap::value_parser!(u64).range(1..))
				.help("Maximum number of assessments to list"),
		)
		.arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let purl = matches.get_one::<String>("package").expect("required PURL");
	let limit = *matches.get_one::<u64>("limit").expect("default limit");
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
	let runs = runtime.block_on(async {
		let db = db::connection(config).await?;
		let package_name = npm_package_name_from_purl(purl).context("invalid NPM package PURL")?;
		let package = packages::Entity::find()
			.filter(packages::Column::PackageHost.eq("npm"))
			.filter(packages::Column::Name.eq(package_name.as_str()))
			.one(&db)
			.await?
			.context("package not found")?;
		Ok::<_, anyhow::Error>(list_hipcheck_runs_for_package(&db, package.id, limit).await?)
	})?;
	if matches.get_flag("json") {
		let output = runs
			.iter()
			.map(|run| {
				json_run_output(
					&run.assessment_id,
					run.target_purl.as_deref(),
					&run.status,
					run.policy_recommendation.as_deref(),
					run.created_at,
				)
			})
			.collect::<Vec<_>>();
		println!("{}", serde_json::json!({"runs":output}));
	} else {
		println!("assessments: {}", runs.len());
		for run in runs {
			println!(
				"{} {} {} {} {}",
				run.assessment_id,
				run.target_purl.as_deref().unwrap_or("<none>"),
				run.status,
				run.policy_recommendation.as_deref().unwrap_or("<none>"),
				run.created_at,
			);
		}
	}
	Ok(())
}

fn json_run_output(
	id: &str,
	target: Option<&str>,
	state: &str,
	recommendation: Option<&str>,
	created_at: impl serde::Serialize,
) -> serde_json::Value {
	serde_json::json!({
		"id": id,
		"target": target,
		"state": state,
		"recommendation": recommendation,
		"createdAt": created_at,
	})
}

fn npm_package_name_from_purl(purl: &str) -> Result<NpmPackageName> {
	let Some(encoded_name) = purl.strip_prefix("pkg:npm/") else {
		anyhow::bail!("PURL must use the pkg:npm type");
	};
	if encoded_name.is_empty() || encoded_name.contains(['@', '?', '#']) {
		anyhow::bail!("PURL must identify an unversioned npm package");
	}
	if !has_valid_percent_encoding(encoded_name) {
		anyhow::bail!("PURL package name has invalid percent encoding");
	}
	let name = percent_decode_str(encoded_name)
		.decode_utf8()
		.context("PURL package name is not valid UTF-8")?;
	NpmPackageName::from_str(&name).context("PURL has an invalid npm package name")
}

fn has_valid_percent_encoding(value: &str) -> bool {
	let mut bytes = value.bytes();
	while let Some(byte) = bytes.next() {
		if byte != b'%' {
			continue;
		}
		let (Some(first), Some(second)) = (bytes.next(), bytes.next()) else {
			return false;
		};
		if !first.is_ascii_hexdigit() || !second.is_ascii_hexdigit() {
			return false;
		}
	}
	true
}

fn json_argument() -> clap::Arg {
	clap::Arg::new("json")
		.long("json")
		.action(clap::ArgAction::SetTrue)
		.help("Print persisted assessments as JSON")
}

#[cfg(test)]
mod tests {
	use super::{command, json_run_output, npm_package_name_from_purl};
	use clap::error::ErrorKind;

	const ASSESSMENT_ID: &str = "0198f30e-2bfa-7000-8000-000000000007";

	#[test]
	fn runs_requires_a_package() {
		let error = command()
			.try_get_matches_from(["runs"])
			.expect_err("missing package should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn runs_accepts_package_limit_and_json_output() {
		command()
			.try_get_matches_from([
				"runs",
				"--package",
				"pkg:npm/example",
				"--limit",
				"20",
				"--json",
			])
			.expect("assessment runs should parse");
	}

	#[test]
	fn parses_unversioned_npm_package_purl() {
		let package =
			npm_package_name_from_purl("pkg:npm/%40scope/example").expect("PURL should parse");

		assert_eq!(package.as_str(), "@scope/example");
	}

	#[test]
	fn rejects_versioned_or_non_npm_package_purls() {
		for purl in [
			"pkg:npm/example@1.2.3",
			"pkg:cargo/example",
			"pkg:npm/example?repository_url=https://example.test",
			"pkg:npm/example%ZZ",
		] {
			assert!(npm_package_name_from_purl(purl).is_err(), "{purl}");
		}
	}

	#[test]
	fn runs_json_output_includes_the_assessed_version() {
		assert_eq!(
			json_run_output(
				ASSESSMENT_ID,
				Some("pkg:npm/example@1.2.3"),
				"completed",
				Some("pass"),
				"2026-09-03T00:00:00Z",
			),
			serde_json::json!({
				"id": ASSESSMENT_ID,
				"target": "pkg:npm/example@1.2.3",
				"state": "completed",
				"recommendation": "pass",
				"createdAt": "2026-09-03T00:00:00Z",
			})
		);
	}
}
