mod version;

use anyhow::Context;
use anyhow::Result;
use dropshot::ApiDescription;
use dropshot::ConfigLogging;
use dropshot::ConfigLoggingLevel;
use dropshot::ServerBuilder;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// Create the async runtime and launch the server.
fn run() -> Result<()> {
    let _matches = clap::Command::new(env!("CARGO_CRATE_NAME"))
        .about("Night Vision backend server")
        .version(version::get_version())
        .long_version(version::get_long_version())
        .get_matches();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(launch_server())?;
    Ok(())
}

/// Launch the server.
async fn launch_server() -> Result<()> {
    let log = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    }
    .to_logger("nv-server")?;

    let api = ApiDescription::new();
    // TODO: Register API functions

    let server = ServerBuilder::new(api, (), log)
        .start()
        .context("failed to start server")?;

    server
        .await
        .map_err(|e| anyhow::anyhow!("server error: {}", e))?;

    Ok(())
}
