use nv_common::config::Config;
use slog::{Logger, info};

pub fn command() -> clap::Command {
    clap::Command::new("status").about("Check contents of KEV cache in database")
}

pub fn run(config: &Config, log: Logger) -> anyhow::Result<()> {
    info!(log, "Reading KEV cache");
    let rt = nv_common::rt::AsyncRuntime::new(config)?;

    rt.block_on(nv_common::kev::read_status(config, log.clone()))?;
    Ok(())
}
