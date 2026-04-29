mod config;
mod version;

use anyhow::Context;
use anyhow::Result;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::value_parser;
use dropshot::ApiDescription;
use dropshot::ConfigLogging;
use dropshot::ConfigLoggingLevel;
use dropshot::ServerBuilder;
use std::process::ExitCode;
use std::time::Duration;

use crate::config::Config;

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// Create the async runtime and launch the server.
fn run() -> Result<()> {
    let matches = clap::Command::new(env!("CARGO_CRATE_NAME"))
        .about("Night Vision backend server")
        .version(version::get_version())
        .long_version(version::get_long_version())
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .value_parser(value_parser!(Utf8PathBuf))
                .help("Path to the configuration file"),
        )
        .get_matches();

    let config_path = matches
        .get_one::<Utf8PathBuf>("config")
        .map(Utf8PathBuf::as_path)
        .unwrap_or_else(|| Utf8Path::new("config.nv"));

    let config = Config::parse(&config_path)?;

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    configure_tokio_runtime(&mut builder, &config);
    let rt = builder.build()?;

    rt.block_on(launch_server(&config))?;

    Ok(())
}

/// Launch the server.
async fn launch_server(config: &Config) -> Result<()> {
    let log = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    }
    .to_logger("nv-server")?;

    let api = ApiDescription::new();
    // TODO: Register API functions

    let server = ServerBuilder::new(api, (), log)
        .config(config.dropshot_config()?)
        .start()
        .context("failed to start server")?;

    server
        .await
        .map_err(|e| anyhow::anyhow!("server error: {}", e))?;

    Ok(())
}

fn configure_tokio_runtime(builder: &mut tokio::runtime::Builder, config: &Config) {
    // Make sure to turn on IO and timers, otherwise it won't run at all.
    builder.enable_all();

    if let Some(worker_threads) = config.async_worker_threads {
        builder.worker_threads(worker_threads);
    }

    if let Some(thread_stack_size) = config.async_worker_thread_stack_size {
        builder.thread_stack_size(thread_stack_size);
    }

    if let Some(max_blocking_threads) = config.async_max_blocking_threads {
        builder.max_blocking_threads(max_blocking_threads);
    }

    if let Some(keep_alive) = config.async_blocking_thread_keep_alive {
        builder.thread_keep_alive(Duration::from_millis(keep_alive));
    }

    if let Some(global_queue_interval) = config.async_global_queue_interval {
        builder.global_queue_interval(global_queue_interval);
    }

    if let Some(event_interval) = config.async_event_interval {
        builder.event_interval(event_interval);
    }
}
