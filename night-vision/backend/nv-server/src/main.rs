mod api;
mod config;
mod context;
mod version;

use crate::api::Api;
use crate::config::Config;
use crate::context::ApiCtx;
use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use camino::Utf8PathBuf;
use clap::value_parser;
use dropshot::ConfigLogging;
use dropshot::ConfigLoggingLevel;
use dropshot::ServerBuilder;
use sea_orm::ConnectOptions;
use sea_orm::Database;
use sea_orm::DatabaseConnection;
use std::process::ExitCode;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::runtime::Runtime;

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
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
                .value_parser(value_parser!(Utf8PathBuf))
                .default_value("config.nv")
                .help("Path to the configuration file"),
        )
        .get_matches();

    // SAFETY: `get_one` always returns `Some` when a default value is set, so this won't panic.
    let config_path = matches
        .get_one::<Utf8PathBuf>("config")
        .expect("config path is required");

    let config = Config::parse(config_path)?;

    tokio_runtime(&config)?.block_on(launch_server(&config))?;

    Ok(())
}

/// Launch the server.
async fn launch_server(config: &Config) -> Result<()> {
    let api = Api::new();

    let ctx = ApiCtx {
        db: db_conn(config).await?,
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

// Since Tokio handles configuration through the builder pattern, rather than
// by taking a configuration object (as Dropshot does), we need to do this
// little dance to configure the runtime.
fn tokio_runtime(config: &Config) -> Result<Runtime> {
    let mut builder = Builder::new_multi_thread();

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

    let runtime = builder.build().context("failed to build Tokio runtime")?;

    Ok(runtime)
}

/// Get connection options for the database.
async fn db_conn(config: &Config) -> Result<DatabaseConnection> {
    let mut opt = ConnectOptions::new(&config.database_connection);

    if let Some(database_max_connections) = config.database_max_connections {
        opt.max_connections(database_max_connections);
    }

    if let Some(database_min_connections) = config.database_min_connections {
        opt.min_connections(database_min_connections);
    }

    if let Some(database_connect_timeout) = config.database_connect_timeout {
        opt.connect_timeout(Duration::from_millis(database_connect_timeout));
    }

    if let Some(database_idle_timeout) = config.database_idle_timeout {
        opt.idle_timeout(Duration::from_millis(database_idle_timeout));
    }

    if let Some(database_acquire_timeout) = config.database_acquire_timeout {
        opt.acquire_timeout(Duration::from_millis(database_acquire_timeout));
    }

    if let Some(database_max_lifetime) = config.database_max_lifetime {
        opt.max_lifetime(Duration::from_millis(database_max_lifetime));
    }

    let db = Database::connect(opt)
        .await
        .context("failed to connect to database")?;

    Ok(db)
}

fn crate_name() -> String {
    env!("CARGO_CRATE_NAME").replace("_", "-").to_string()
}
