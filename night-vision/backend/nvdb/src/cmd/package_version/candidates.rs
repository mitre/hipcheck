//! Discover operator-selectable upgrades for a KEV-affected npm release.

use anyhow::{Context as _, Result, bail};
use nv_common::{
    config::Config,
    cve::kev::{
        KevNpmMatchStatus, ReachableNpmPackageVersion, kev_affected_npm_package_version,
        kev_affected_npm_package_versions,
    },
    db,
    npm::{
        candidates::{
            ApiCompatibility, CandidateExclusionReason, CandidateStatus, UpgradeDistance,
            discover_upgrade_candidates,
        },
        elaboration::{NpmRegistryClient, PackumentProvider as _},
        purl::NpmPackagePurl,
    },
    rt,
};
use serde::Serialize;

use super::kev::reachable_npm_package_version_from_purl;

pub fn command() -> clap::Command {
    clap::Command::new("candidates")
        .about("List eligible upgrades for a KEV-affected package version")
        .arg(
            clap::Arg::new("purl")
                .required(true)
                .value_name("AFFECTED-PURL"),
        )
        .arg(
            clap::Arg::new("json")
                .long("json")
                .action(clap::ArgAction::SetTrue),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let purl = matches.get_one::<String>("purl").expect("required PURL");
    let baseline = NpmPackagePurl::parse(purl).context("invalid NPM package PURL")?;
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let candidates = runtime.block_on(discover(config, purl, &baseline))?;
    if matches.get_flag("json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&CandidatesOutput {
                affected_purl: purl,
                candidates
            })?
        );
    } else {
        println!("affected_purl: {purl}");
        for candidate in candidates {
            println!(
                "{} distance={} compatibility={} published={} eligibility={} kev_status={}",
                candidate.purl,
                candidate.upgrade_distance,
                candidate.compatibility,
                candidate.published_at.as_deref().unwrap_or("unknown"),
                candidate.eligibility,
                candidate.kev_status
            );
        }
    }
    Ok(())
}

async fn discover(
    config: &Config,
    purl: &str,
    baseline: &NpmPackagePurl,
) -> Result<Vec<CandidateOutput>> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let baseline_matches =
        kev_affected_npm_package_version(&db, reachable_npm_package_version_from_purl(purl)?)
            .await
            .context("failed to match baseline against KEV-linked CVEs")?;
    if !baseline_matches
        .iter()
        .any(|item| item.status == KevNpmMatchStatus::Affected)
    {
        bail!("affected PURL has no locally known active KEV match");
    }
    let client = NpmRegistryClient::new(
        config.npm_registry_url.clone(),
        config.package_elaboration_max_packument_bytes,
        config.package_elaboration_limits().request_timeout,
    )
    .context("invalid NPM registry configuration")?;
    let packument = client
        .fetch(&baseline.name)
        .await
        .context("failed to fetch NPM package metadata")?;
    let discovered = discover_upgrade_candidates(&packument, &baseline.version)
        .context("affected PURL has an invalid semantic version")?;
    let reachable = discovered
        .iter()
        .map(|candidate| ReachableNpmPackageVersion {
            package_name: candidate.name.as_str().to_owned(),
            version: candidate.version.to_string(),
            source_evidence: format!("candidate PURL {}", candidate.purl),
        })
        .collect::<Vec<_>>();
    let candidate_matches = kev_affected_npm_package_versions(&db, &reachable)
        .await
        .context("failed to match candidates against KEV-linked CVEs")?;
    let mut output = Vec::with_capacity(discovered.len());
    for candidate in discovered {
        let version = candidate.version.to_string();
        let matches = candidate_matches.iter().filter(|matched| {
            matched.package_name.as_deref() == Some(candidate.name.as_str())
                && matched.affected_version.as_deref() == Some(&version)
        });
        let statuses = matches
            .map(|matched| matched.status.clone())
            .collect::<Vec<_>>();
        let kev_status = if statuses.contains(&KevNpmMatchStatus::Affected) {
            "affected"
        } else if statuses.contains(&KevNpmMatchStatus::Unknown) {
            "unknown"
        } else {
            "no-known-active-kev-match"
        };
        let eligibility = match &candidate.status {
            CandidateStatus::Included if kev_status != "affected" => "eligible".to_owned(),
            CandidateStatus::Included => "ineligible: known active KEV match".to_owned(),
            CandidateStatus::Excluded(reasons) => format!(
                "ineligible: {}",
                reasons.iter().map(exclusion).collect::<Vec<_>>().join(", ")
            ),
        };
        output.push(CandidateOutput {
            purl: candidate.purl,
            upgrade_distance: upgrade_distance(candidate.upgrade_distance),
            compatibility: compatibility(candidate.api_compatibility),
            published_at: candidate
                .published_at
                .map(|timestamp| timestamp.to_string()),
            eligibility,
            kev_status: kev_status.to_owned(),
        });
    }
    Ok(output)
}

fn compatibility(value: ApiCompatibility) -> &'static str {
    match value {
        ApiCompatibility::Compatible => "compatible",
        ApiCompatibility::Incompatible => "incompatible",
        ApiCompatibility::NoGuarantee => "no-guarantee",
    }
}
fn upgrade_distance(value: UpgradeDistance) -> &'static str {
    match value {
        UpgradeDistance::Patch => "patch",
        UpgradeDistance::Minor => "minor",
        UpgradeDistance::Major => "major",
    }
}
fn exclusion(value: &CandidateExclusionReason) -> String {
    match value {
        CandidateExclusionReason::Prerelease => "prerelease".to_owned(),
        CandidateExclusionReason::Deprecated(message) => format!("deprecated: {message}"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidatesOutput<'a> {
    affected_purl: &'a str,
    candidates: Vec<CandidateOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateOutput {
    purl: String,
    upgrade_distance: &'static str,
    compatibility: &'static str,
    published_at: Option<String>,
    eligibility: String,
    kev_status: String,
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn candidates_accepts_an_affected_purl_and_json() {
        command()
            .try_get_matches_from(["candidates", "pkg:npm/example@1.2.3", "--json"])
            .expect("candidate command should parse");
    }

    #[test]
    fn candidates_requires_an_affected_purl() {
        command().try_get_matches_from(["candidates"]).unwrap_err();
    }
}
