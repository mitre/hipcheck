use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    db::{
        self,
        entities::{cisa_kev_sync_runs, cisa_kev_sync_runs::Model as KevSyncRun},
    },
    rt,
};
use sea_orm::{EntityTrait as _, QueryOrder as _, QuerySelect as _};

const DEFAULT_LIMIT: u64 = 10;
const ERROR_SUMMARY_MAX_CHARS: usize = 120;

pub fn command() -> clap::Command {
    clap::Command::new("runs")
        .about("List recent KEV catalog sync runs")
        .arg(
            clap::Arg::new("limit")
                .long("limit")
                .value_name("N")
                .default_value(DEFAULT_LIMIT.to_string())
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Maximum number of sync runs to list"),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let limit = *matches
        .get_one::<u64>("limit")
        .expect("limit has a default value");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

    runtime.block_on(runs(config, limit))
}

async fn runs(config: &Config, limit: u64) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let runs = cisa_kev_sync_runs::Entity::find()
        .order_by_desc(cisa_kev_sync_runs::Column::Generation)
        .limit(limit)
        .all(&db)
        .await
        .context("failed to read KEV catalog sync runs")?;

    print_runs(&runs);

    Ok(())
}

fn print_runs(runs: &[KevSyncRun]) {
    if runs.is_empty() {
        println!("runs: <none>");
        return;
    }

    for (index, run) in runs.iter().enumerate() {
        if index > 0 {
            println!();
        }

        println!("generation: {}", run.generation);
        println!("status: {}", run.status);
        println!(
            "catalog_version: {}",
            run.catalog_version.as_deref().unwrap_or("<none>")
        );
        println!(
            "catalog_date_released: {}",
            run.catalog_date_released
                .as_ref()
                .map_or("<none>".to_owned(), ToString::to_string)
        );
        println!(
            "catalog_count: {}",
            run.catalog_count
                .map_or("<none>".to_owned(), |count| count.to_string())
        );
        println!("records_seen: {}", run.records_seen);
        println!("records_inserted: {}", run.records_inserted);
        println!("records_updated: {}", run.records_updated);
        println!("checked_at: {}", run.checked_at);
        println!(
            "completed_at: {}",
            run.completed_at
                .as_ref()
                .map_or("<none>".to_owned(), ToString::to_string)
        );
        println!("error_summary: {}", error_summary(run.error.as_deref()));
    }
}

fn error_summary(error: Option<&str>) -> String {
    let Some(error) = error.map(str::trim).filter(|error| !error.is_empty()) else {
        return "<none>".to_owned();
    };
    let first_line = error.lines().next().unwrap_or(error).trim();

    truncate_summary(first_line, ERROR_SUMMARY_MAX_CHARS)
}

fn truncate_summary(summary: &str, max_chars: usize) -> String {
    let mut char_indices = summary.char_indices();
    let Some((byte_index, _)) = char_indices.nth(max_chars) else {
        return summary.to_owned();
    };

    format!("{}...", &summary[..byte_index])
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn kev_runs_uses_default_limit() {
        let matches = command()
            .try_get_matches_from(["runs"])
            .expect("runs should parse");

        assert_eq!(matches.get_one::<u64>("limit"), Some(&10));
    }

    #[test]
    fn kev_runs_accepts_limit() {
        let matches = command()
            .try_get_matches_from(["runs", "--limit", "25"])
            .expect("runs should parse");

        assert_eq!(matches.get_one::<u64>("limit"), Some(&25));
    }

    #[test]
    fn kev_runs_rejects_zero_limit() {
        command()
            .try_get_matches_from(["runs", "--limit", "0"])
            .expect_err("zero limit should fail");
    }
}
