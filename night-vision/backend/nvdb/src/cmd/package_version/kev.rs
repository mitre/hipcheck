use anyhow::{Context as _, Result, bail};
use nv_common::{
    config::Config,
    cve::kev::{
        KevAffectedNpmPackageVersion, KevNpmMatchConfidence, KevNpmMatchStatus,
        ReachableNpmPackageVersion, kev_affected_npm_package_version,
    },
    db,
    npm::types::NpmPackageName,
    rt,
};
use percent_encoding::percent_decode_str;
use serde::Serialize;
use std::str::FromStr as _;

pub fn command() -> clap::Command {
    clap::Command::new("kev")
        .about("List KEV-linked CVE matches for a package version")
        .arg(
            clap::Arg::new("purl")
                .required(true)
                .value_name("PURL")
                .help("Package URL for the resolved package version"),
        )
        .arg(
            clap::Arg::new("json")
                .long("json")
                .action(clap::ArgAction::SetTrue)
                .help("Print KEV-linked matches as JSON"),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let purl = matches.get_one::<String>("purl").expect("required PURL");
    let package = npm_package_version_from_purl(purl).context("invalid NPM package PURL")?;
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let results = runtime.block_on(kev(config, package))?;

    if matches.get_flag("json") {
        print_json(purl, &results)?;
    } else {
        print_matches(purl, &results);
    }

    Ok(())
}

async fn kev(
    config: &Config,
    package: ReachableNpmPackageVersion,
) -> Result<Vec<KevAffectedNpmPackageVersion>> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    kev_affected_npm_package_version(&db, package)
        .await
        .context("failed to match the package version against KEV-linked CVEs")
}

fn npm_package_version_from_purl(purl: &str) -> Result<ReachableNpmPackageVersion> {
    let Some(package_and_version) = purl.strip_prefix("pkg:npm/") else {
        bail!("PURL must use the pkg:npm type");
    };
    if package_and_version.contains(['?', '#']) {
        bail!("PURL must not include qualifiers or a subpath");
    }
    let Some((encoded_name, encoded_version)) = package_and_version.rsplit_once('@') else {
        bail!("PURL must include a package version");
    };
    let package_name = decode_purl_component(encoded_name, "package name")?;
    NpmPackageName::from_str(&package_name).context("PURL has an invalid npm package name")?;
    let version = decode_purl_component(encoded_version, "version")?;
    if version.is_empty() || version.contains(['/', '@']) {
        bail!("PURL must include one non-empty package version");
    }

    Ok(ReachableNpmPackageVersion {
        package_name,
        version,
        source_evidence: format!("queried package version PURL {purl}"),
    })
}

fn decode_purl_component(component: &str, name: &str) -> Result<String> {
    if !has_valid_percent_encoding(component) {
        bail!("PURL {name} has invalid percent encoding");
    }
    percent_decode_str(component)
        .decode_utf8()
        .with_context(|| format!("PURL {name} is not valid UTF-8"))
        .map(std::borrow::Cow::into_owned)
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KevMatchOutput<'a> {
    cve_id: &'a str,
    status: &'a str,
    confidence: &'a str,
    kev: KevContextOutput<'a>,
    package_name: Option<&'a str>,
    affected_version: Option<&'a str>,
    source_evidence: &'a [String],
    caveats: &'a [String],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KevContextOutput<'a> {
    vendor_project: Option<&'a str>,
    product: Option<&'a str>,
    vulnerability_name: Option<&'a str>,
    date_added: Option<&'a str>,
}

impl<'a> From<&'a KevAffectedNpmPackageVersion> for KevMatchOutput<'a> {
    fn from(matched: &'a KevAffectedNpmPackageVersion) -> Self {
        Self {
            cve_id: &matched.cve_id,
            status: match matched.status {
                KevNpmMatchStatus::Affected => "affected",
                KevNpmMatchStatus::Unknown => "unknown",
            },
            confidence: match matched.confidence {
                KevNpmMatchConfidence::High => "high",
                KevNpmMatchConfidence::Unknown => "unknown",
            },
            kev: KevContextOutput {
                vendor_project: matched.kev_context.vendor_project.as_deref(),
                product: matched.kev_context.product.as_deref(),
                vulnerability_name: matched.kev_context.vulnerability_name.as_deref(),
                date_added: matched.kev_context.date_added.as_deref(),
            },
            package_name: matched.package_name.as_deref(),
            affected_version: matched.affected_version.as_deref(),
            source_evidence: &matched.source_evidence,
            caveats: &matched.caveats,
        }
    }
}

fn print_json(purl: &str, matches: &[KevAffectedNpmPackageVersion]) -> Result<()> {
    let output = serde_json::to_string_pretty(&json_output(purl, matches))
        .context("failed to serialize KEV package-version matches")?;
    println!("{output}");
    Ok(())
}

fn json_output(purl: &str, matches: &[KevAffectedNpmPackageVersion]) -> serde_json::Value {
    let matches = matches.iter().map(KevMatchOutput::from).collect::<Vec<_>>();
    serde_json::json!({
        "purl": purl,
        "matches": matches,
    })
}

fn print_matches(purl: &str, matches: &[KevAffectedNpmPackageVersion]) {
    println!("purl: {purl}");
    if matches.is_empty() {
        println!("matches: <none>");
        println!("No KEV-linked CVE match was found in locally available data.");
        println!("This does not mean the package version is safe.");
        return;
    }

    println!("matches: {}", matches.len());
    for matched in matches {
        let output = KevMatchOutput::from(matched);
        println!(
            "{} status={} confidence={}",
            output.cve_id, output.status, output.confidence
        );
        println!(
            "  kev: vendor_project={} product={} vulnerability_name={} date_added={}",
            output.kev.vendor_project.unwrap_or("<none>"),
            output.kev.product.unwrap_or("<none>"),
            output.kev.vulnerability_name.unwrap_or("<none>"),
            output.kev.date_added.unwrap_or("<none>"),
        );
        println!(
            "  package: {}@{}",
            output.package_name.unwrap_or("<unknown>"),
            output.affected_version.unwrap_or("<unknown>"),
        );
        print_values("evidence", output.source_evidence);
        print_values("caveats", output.caveats);
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

#[cfg(test)]
mod tests {
    use super::{command, json_output, npm_package_version_from_purl};
    use clap::error::ErrorKind;
    use nv_common::cve::kev::{
        KevAffectedNpmPackageVersion, KevContext, KevNpmMatchConfidence, KevNpmMatchStatus,
    };

    #[test]
    fn kev_requires_a_purl() {
        let error = command()
            .try_get_matches_from(["kev"])
            .expect_err("missing PURL should fail");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn kev_accepts_a_purl() {
        command()
            .try_get_matches_from(["kev", "pkg:npm/example@1.2.3", "--json"])
            .expect("package-version KEV lookup should parse");
    }

    #[test]
    fn parses_unscoped_npm_package_version_purl() {
        let package =
            npm_package_version_from_purl("pkg:npm/example@1.2.3").expect("PURL should parse");

        assert_eq!(package.package_name, "example");
        assert_eq!(package.version, "1.2.3");
    }

    #[test]
    fn parses_percent_encoded_scoped_npm_package_version_purl() {
        let package = npm_package_version_from_purl("pkg:npm/%40scope/example@1.2.3")
            .expect("PURL should parse");

        assert_eq!(package.package_name, "@scope/example");
        assert_eq!(package.version, "1.2.3");
    }

    #[test]
    fn rejects_non_versioned_or_non_npm_purls() {
        for purl in [
            "pkg:npm/example",
            "pkg:cargo/example@1.2.3",
            "pkg:npm/example@1.2.3?repository_url=https://example.test",
            "pkg:npm/example@1.2%ZZ",
        ] {
            assert!(npm_package_version_from_purl(purl).is_err(), "{purl}");
        }
    }

    #[test]
    fn kev_json_output_preserves_match_evidence_and_caveats() {
        let output = json_output(
            "pkg:npm/example@1.2.3",
            &[KevAffectedNpmPackageVersion {
                cve_id: "CVE-2026-0001".to_owned(),
                kev_context: KevContext {
                    cve_id: "CVE-2026-0001".to_owned(),
                    vendor_project: Some("example".to_owned()),
                    product: Some("example".to_owned()),
                    vulnerability_name: None,
                    date_added: Some("2026-01-01".to_owned()),
                },
                package_name: Some("example".to_owned()),
                affected_version: Some("1.2.3".to_owned()),
                source_evidence: vec!["CVE record".to_owned()],
                confidence: KevNpmMatchConfidence::High,
                caveats: vec!["range matched".to_owned()],
                status: KevNpmMatchStatus::Affected,
            }],
        );

        assert_eq!(output["purl"], "pkg:npm/example@1.2.3");
        assert_eq!(output["matches"][0]["cveId"], "CVE-2026-0001");
        assert_eq!(output["matches"][0]["confidence"], "high");
        assert_eq!(
            output["matches"][0]["caveats"],
            serde_json::json!(["range matched"])
        );
    }
}
