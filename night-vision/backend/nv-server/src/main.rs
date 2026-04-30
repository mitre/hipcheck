mod api;
mod config;
mod db;
mod rt;
mod version;

use crate::{
    api::{Api, ApiCtx},
    config::Config,
};
use anyhow::{Context, Result, anyhow};
use camino::Utf8PathBuf;
use dropshot::{ConfigLogging, ConfigLoggingLevel, ServerBuilder};
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("error: {}", err);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// Create the async runtime and launch the server.
fn run() -> Result<()> {
    // NOTE: In general, prefer to add items to the configuration file rather
    // than adding flags to the CLI. The configuration file can be tracked
    // and managed more easily, and is the standard way to configure the server.
    let matches = clap::Command::new(crate_name())
        .about("Night Vision backend server")
        .version(version::get_version())
        .long_version(version::get_long_version())
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .value_parser(clap::value_parser!(Utf8PathBuf))
                .default_value("config.nv")
                .help("Path to the configuration file"),
        )
        .get_matches();

    // SAFETY: `get_one` always returns `Some` when a default value is set, so this won't panic.
    let config_path = matches
        .get_one::<Utf8PathBuf>("config")
        .expect("config path is required");

    let config = Config::parse(config_path)?;

    println!("{}", config.report());

    rt::build(&config)?.block_on(launch_server(&config))?;

    Ok(())
}

/// Launch the server.
async fn launch_server(config: &Config) -> Result<()> {
    let api = Api::new();

    let ctx = ApiCtx {
        db: db::connection(config).await?,
    };

    let log = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    }
    .to_logger("nv-server")?;

    let server = ServerBuilder::new(api, ctx, log)
        .config(config.dropshot_config()?)
        .start()
        .context("failed to start server")?;

    server.await.map_err(|e| anyhow!("server error: {}", e))?;

    Ok(())
}

/// Get the crate name, replacing underscores with hyphens.
fn crate_name() -> String {
    env!("CARGO_CRATE_NAME").replace("_", "-").to_string()
}
