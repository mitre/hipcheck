use anyhow::{Context as _, Result, bail};
use nv_common::{
    config::Config,
    db::{
        self,
        entities::{cve_list_sync_runs, cve_list_sync_runs::Model as CveListSyncRun},
    },
    rt,
};
use sea_orm::EntityTrait as _;

use crate::time_display::TimeDisplay;

pub fn command() -> clap::Command {
    clap::Command::new("run")
        .about("Show one CVE List sync run")
        .arg(
            clap::Arg::new("generation")
                .value_name("GENERATION")
                .required(true)
                .value_parser(clap::value_parser!(i64).range(1..))
                .help("Sync-run generation to show"),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches, time_display: TimeDisplay) -> Result<()> {
    let generation = *matches
        .get_one::<i64>("generation")
        .expect("required generation argument");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

    runtime.block_on(show_run(config, generation, time_display))
}

async fn show_run(config: &Config, generation: i64, time_display: TimeDisplay) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let run = cve_list_sync_runs::Entity::find_by_id(generation)
        .one(&db)
        .await
        .context("failed to read CVE List sync run")?;
    let Some(run) = run else {
        bail!("CVE List sync run not found: {generation}");
    };

    print_run(&run, time_display);

    Ok(())
}

fn print_run(run: &CveListSyncRun, time_display: TimeDisplay) {
    println!("generation: {}", run.generation);
    println!("status: {}", run.status);
    println!(
        "repository_url: {}",
        run.repository_url.as_deref().unwrap_or("<none>")
    );
    println!(
        "repository_ref: {}",
        run.repository_ref.as_deref().unwrap_or("<none>")
    );
    println!(
        "commit_sha: {}",
        run.commit_sha.as_deref().unwrap_or("<none>")
    );
    println!("records_seen: {}", run.records_seen);
    println!("records_inserted: {}", run.records_inserted);
    println!("records_updated: {}", run.records_updated);
    println!("checked_at: {}", time_display.format(&run.checked_at));
    println!(
        "completed_at: {}",
        time_display.format_optional(run.completed_at.as_ref())
    );
    println!("error: {}", run.error.as_deref().unwrap_or("<none>"));
}
