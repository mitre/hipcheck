use anyhow::{Context as _, Result};
use nv_common::{config::Config, db, rt};

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
    let source = runtime.block_on(show(config, source_id))?;
    if matches.get_flag("json") {
        println!(
            "{}",
            serde_json::json!({
                "id": source.source_id,
                "fileName": source.file_name,
                "status": source.resolution_status,
                "error": source.resolution_error,
            })
        );
    } else {
        println!("id: {}", source.source_id);
        println!("file_name: {}", source.file_name);
        println!("status: {}", source.resolution_status);
        println!(
            "error: {}",
            source.resolution_error.as_deref().unwrap_or("<none>")
        );
    }
    Ok(())
}

async fn show(
    config: &Config,
    source_id: &str,
) -> Result<nv_common::db::entities::package_sources::Model> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    super::source_by_id(&db, source_id).await
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
    use super::command;

    #[test]
    fn show_accepts_a_source_id() {
        command()
            .try_get_matches_from(["show", "source-1", "--json"])
            .expect("package-source show should parse");
    }
}
