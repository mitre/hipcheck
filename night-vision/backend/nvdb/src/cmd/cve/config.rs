use anyhow::Result;
use nv_common::config::{Config, ConfigValueSource};

pub fn command() -> clap::Command {
    clap::Command::new("config").about("Print the effective CVE List ingest configuration")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    let worker_config = config.cve_list_worker_config();

    println!("repository_url: {}", worker_config.repository_url());
    println!("repository_ref: {}", worker_config.repository_ref());
    println!("checkout_path: {}", worker_config.checkout_path());
    println!(
        "sync_interval_ms: {}",
        worker_config.sync_interval().as_millis()
    );
    println!(
        "sync_timeout_ms: {}",
        worker_config.sync_timeout().as_millis()
    );
    println!(
        "first_sync_timeout_ms: {}",
        worker_config.first_sync_timeout().as_millis()
    );
    println!(
        "parse_concurrency: {}{}",
        worker_config.parse_concurrency(),
        source_suffix(worker_config.parse_concurrency_source())
    );
    println!(
        "write_batch_size: {}{}",
        worker_config.write_batch_size(),
        source_suffix(worker_config.write_batch_size_source())
    );
    println!(
        "write_channel_size: {}{}",
        worker_config.write_channel_size(),
        source_suffix(worker_config.write_channel_size_source())
    );

    Ok(())
}

fn source_suffix(source: ConfigValueSource) -> String {
    format!(" ({source})")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn cve_config_accepts_no_extra_arguments() {
        command()
            .try_get_matches_from(["config"])
            .expect("config subcommand should parse");
    }
}
