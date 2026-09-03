use anyhow::{Context as _, Result};
use nv_common::{config::Config, db, npm::elaboration::storage::persisted_package_versions, rt};
use serde::Serialize;

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
    let versions = runtime.block_on(versions(config, source_id))?;
    if matches.get_flag("json") {
        println!("{}", serde_json::json!({ "versions": versions }));
    } else {
        for version in versions {
            println!(
                "{} ({} derivations)",
                version.purl, version.derivation_count
            );
            for derivation in version.derivations {
                println!("  {}", derivation.join(" -> "));
            }
        }
    }
    Ok(())
}

async fn versions(config: &Config, source_id: &str) -> Result<Vec<ResolvedVersion>> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let source = super::source_by_id(&db, source_id).await?;
    persisted_package_versions(&db, source.id)
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
                        derivation_count: derivations.len(),
                        derivations,
                    }
                })
                .collect()
        })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedVersion {
    purl: String,
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
    use super::command;

    #[test]
    fn versions_accepts_a_source_id() {
        command()
            .try_get_matches_from(["versions", "source-1", "--json"])
            .expect("package-source versions should parse");
    }
}
