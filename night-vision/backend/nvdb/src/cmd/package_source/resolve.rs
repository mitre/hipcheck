use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    db,
    npm::{
        elaboration::{
            NpmRegistryClient, elaborate,
            storage::{
                persist_completed_elaboration, persisted_elaboration_warnings,
                record_elaboration_failure,
            },
        },
        package_json::NpmPackageJson,
    },
    rt,
};
use std::sync::Arc;

use super::{ResolvedWarning, print_warnings};

pub fn command() -> clap::Command {
    clap::Command::new("resolve")
        .about("Resolve and persist reachable package versions for a source")
        .arg(source_id_argument())
        .arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let source_id = matches
        .get_one::<String>("source-id")
        .expect("required source ID");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let result = runtime.block_on(resolve(config, source_id))?;
    if matches.get_flag("json") {
        println!(
            "{}",
            serde_json::json!({ "packages": result.package_count, "warnings": result.warnings })
        );
    } else {
        println!("resolved_packages: {}", result.package_count);
        print_warnings(&result.warnings);
    }
    Ok(())
}

async fn resolve(config: &Config, source_id: &str) -> Result<ResolutionSummary> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let source = super::source_by_id(&db, source_id).await?;
    let source_document = NpmPackageJson::parse_package_json(source.file_contents.as_bytes())
        .context("stored source is not a valid npm package.json")?;
    let client = NpmRegistryClient::new(
        config.npm_registry_url.clone(),
        config.package_elaboration_max_packument_bytes,
        config.package_elaboration_limits().request_timeout,
    )
    .context("invalid NPM registry configuration")?;
    let result = elaborate(
        &source_document,
        Arc::new(client),
        config.package_elaboration_limits(),
    )
    .await;
    match result {
        Ok(result) => {
            let package_count = result.packages.len();
            persist_completed_elaboration(&db, source.id, &result)
                .await
                .context("failed to persist elaboration result")?;
            let warnings = persisted_elaboration_warnings(&db, source.id)
                .await
                .context("failed to read persisted elaboration warnings")?
                .into_iter()
                .map(ResolvedWarning::from)
                .collect();
            Ok(ResolutionSummary {
                package_count,
                warnings,
            })
        }
        Err(error) => {
            record_elaboration_failure(&db, source.id, &error.to_string())
                .await
                .context("failed to record elaboration failure")?;
            Err(error.into())
        }
    }
}

struct ResolutionSummary {
    package_count: usize,
    warnings: Vec<ResolvedWarning>,
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
        .help("Print the resolution summary as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn resolve_accepts_a_source_id() {
        command()
            .try_get_matches_from(["resolve", "source-1", "--json"])
            .expect("package-source resolve should parse");
    }
}
