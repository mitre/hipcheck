use nv_common::config::Config;
use slog::{Logger, info};

pub fn command() -> clap::Command {
    clap::Command::new("pull").about("Pull KEV Catalog").arg(
        clap::Arg::new("force")
            .short('f')
            .long("force")
            .action(clap::ArgAction::SetTrue)
            .help("Send unconditional HTTP request"),
    )
}

pub fn run(force: bool, config: &Config, log: Logger) -> anyhow::Result<()> {
    info!(log, "Pulling KEV Catalog");
    let rt = nv_common::rt::AsyncRuntime::new(config)?;

    let request_mode = if force {
        nv_common::kev::RequestMode::NoConditionalRequest
    } else {
        nv_common::kev::RequestMode::UseConditionalRequest
    };
    rt.block_on(nv_common::kev::run_fetch_kev(
        request_mode,
        config,
        log.clone(),
    ))?;
    Ok(())
}
