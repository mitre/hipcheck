use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    db,
    npm::elaboration::storage::{persisted_elaboration_warnings, persisted_package_versions},
    rt,
};
use percent_encoding::percent_decode_str;
use serde::Serialize;

use super::{ResolvedWarning, print_warnings};

pub fn command() -> clap::Command {
    clap::Command::new("versions")
        .about("List package versions and derivations resolved from a source")
        .arg(source_id_argument())
        .arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let source_id = matches
        .get_one::<String>("source-id")
        .expect("required source ID");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let result = runtime.block_on(versions(config, source_id))?;
    if matches.get_flag("json") {
        println!("{}", json_output(&result));
    } else {
        for version in result.versions {
            let root_dependency_kinds = (!version.root_dependency_kinds.is_empty())
                .then(|| format!("; root kinds: {}", version.root_dependency_kinds.join(", ")));
            println!(
                "{} ({} derivations{})",
                display_purl(&version.purl),
                version.derivation_count,
                root_dependency_kinds.unwrap_or_default(),
            );
            for derivation in version.derivations {
                println!(
                    "  {}",
                    derivation
                        .iter()
                        .map(|purl| display_purl(purl))
                        .collect::<Vec<_>>()
                        .join(" -> ")
                );
            }
        }
        print_warnings(&result.warnings);
    }
    Ok(())
}

fn display_purl(purl: &str) -> std::borrow::Cow<'_, str> {
    percent_decode_str(purl).decode_utf8_lossy()
}

fn json_output(result: &ResolvedVersions) -> serde_json::Value {
    serde_json::json!({ "versions": result.versions, "warnings": result.warnings })
}

async fn versions(config: &Config, source_id: &str) -> Result<ResolvedVersions> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let source = super::source_by_id(&db, source_id).await?;
    let versions = persisted_package_versions(&db, source.id)
        .await
        .context("failed to read resolved package derivations")
        .map(|versions| {
            versions
                .into_iter()
                .map(|version| {
                    let derivations = version
                        .derivations
                        .into_iter()
                        .map(|derivation| {
                            std::iter::once("<root>".to_owned())
                                .chain(derivation)
                                .collect()
                        })
                        .collect::<Vec<_>>();
                    ResolvedVersion {
                        purl: version.package_url,
                        root_dependency_kinds: version.root_dependency_kinds,
                        derivation_count: derivations.len(),
                        derivations,
                    }
                })
                .collect()
        })?;
    let warnings = persisted_elaboration_warnings(&db, source.id)
        .await
        .context("failed to read persisted elaboration warnings")?
        .into_iter()
        .map(ResolvedWarning::from)
        .collect();
    Ok(ResolvedVersions { versions, warnings })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedVersions {
    versions: Vec<ResolvedVersion>,
    warnings: Vec<ResolvedWarning>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedVersion {
    purl: String,
    root_dependency_kinds: Vec<String>,
    derivation_count: usize,
    derivations: Vec<Vec<String>>,
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
        .help("Print resolved package versions and derivations as JSON")
}

#[cfg(test)]
mod tests {
    use super::{ResolvedVersion, ResolvedVersions, command, display_purl, json_output};

    #[test]
    fn versions_accepts_a_source_id() {
        command()
            .try_get_matches_from(["versions", "source-1", "--json"])
            .expect("package-source versions should parse");
    }

    #[test]
    fn versions_json_output_preserves_derivation_metadata() {
        let output = json_output(&ResolvedVersions {
            versions: vec![ResolvedVersion {
                purl: "pkg:npm/example@1.2.3".to_owned(),
                root_dependency_kinds: vec!["dependency".to_owned()],
                derivation_count: 1,
                derivations: vec![vec![
                    "<root>".to_owned(),
                    "pkg:npm/example@1.2.3".to_owned(),
                ]],
            }],
            warnings: Vec::new(),
        });

        assert_eq!(output["versions"][0]["purl"], "pkg:npm/example@1.2.3");
        assert_eq!(
            output["versions"][0]["rootDependencyKinds"],
            serde_json::json!(["dependency"])
        );
        assert_eq!(output["versions"][0]["derivationCount"], 1);
        assert_eq!(output["warnings"], serde_json::json!([]));
    }

    #[test]
    fn display_purl_decodes_scoped_npm_package_names() {
        assert_eq!(
            display_purl("pkg:npm/%40types/node@26.1.2"),
            "pkg:npm/@types/node@26.1.2"
        );
    }
}
