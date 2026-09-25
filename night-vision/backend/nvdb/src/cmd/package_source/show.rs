use anyhow::{Context as _, Result};
use nv_common::{
    config::Config, db, npm::elaboration::storage::persisted_elaboration_warnings, rt,
};

use super::{ResolvedWarning, print_warnings};

pub fn command() -> clap::Command {
    clap::Command::new("show")
        .about("Show package-source metadata and processing state")
        .arg(source_id_argument())
        .arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let source_id = matches
        .get_one::<String>("source-id")
        .expect("required source ID");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let result = runtime.block_on(show(config, source_id))?;
    if matches.get_flag("json") {
        println!("{}", json_output(&result));
    } else {
        println!("id: {}", result.source.source_id);
        println!("file_name: {}", result.source.file_name);
        println!("status: {}", result.source.resolution_status);
        println!(
            "error: {}",
            result
                .source
                .resolution_error
                .as_deref()
                .unwrap_or("<none>")
        );
        print_warnings(&result.warnings);
    }
    Ok(())
}

fn json_output(result: &PackageSourceDetails) -> serde_json::Value {
    serde_json::json!({
        "id": result.source.source_id,
        "fileName": result.source.file_name,
        "status": result.source.resolution_status,
        "error": result.source.resolution_error,
        "warnings": result.warnings,
    })
}

async fn show(config: &Config, source_id: &str) -> Result<PackageSourceDetails> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let source = super::source_by_id(&db, source_id).await?;
    let warnings = persisted_elaboration_warnings(&db, source.id)
        .await
        .context("failed to read persisted elaboration warnings")?
        .into_iter()
        .map(ResolvedWarning::from)
        .collect();
    Ok(PackageSourceDetails { source, warnings })
}

struct PackageSourceDetails {
    source: nv_common::db::entities::package_sources::Model,
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
        .help("Print package-source metadata as JSON")
}

#[cfg(test)]
mod tests {
    use super::{PackageSourceDetails, command, json_output};
    use nv_common::db::entities::package_sources;

    #[test]
    fn show_accepts_a_source_id() {
        command()
            .try_get_matches_from(["show", "source-1", "--json"])
            .expect("package-source show should parse");
    }

    #[test]
    fn show_json_output_preserves_source_state() {
        let output = json_output(&PackageSourceDetails {
            source: package_sources::Model {
                id: 1,
                source_id: "source-1".to_owned(),
                display_name: "test package source".to_owned(),
                file_name: "package.json".to_owned(),
                file_contents: "{}".to_owned(),
                inferred_type: "npm-package-json".to_owned(),
                resolution_status: "failed".to_owned(),
                resolution_error: Some("registry unavailable".to_owned()),
                created_at: "2026-09-03T00:00:00Z".parse().expect("valid timestamp"),
                attempt_generation: 1,
                cancellation_requested: false,
                terminal_at: Some(
                    "2026-09-03T01:00:00Z"
                        .parse()
                        .expect("valid terminal timestamp"),
                ),
                deletion_reason: None,
                next_attempt_at: "2026-09-03T00:00:00Z".parse().expect("valid timestamp"),
                lease_expires_at: None,
                failure_kind: Some("internal".to_owned()),
                retryable: true,
                automatic_attempt_count: 3,
            },
            warnings: Vec::new(),
        });

        assert_eq!(output["id"], "source-1");
        assert_eq!(output["fileName"], "package.json");
        assert_eq!(output["status"], "failed");
        assert_eq!(output["error"], "registry unavailable");
        assert_eq!(output["warnings"], serde_json::json!([]));
    }
}
