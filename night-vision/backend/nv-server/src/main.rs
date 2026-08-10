#![deny(unsafe_code)]

use crate::debug_logging::{debug_mode_flag, spawn_sigusr1_listener};
use nv_common::log::logger_with_dynamic_debug;

mod api;
mod cli;
mod debug_logging;
mod env;
mod error;

/// Run the Night Vision server.
fn main() -> Result<(), error::FatalError> {
    // Setup steps (env loading, arg parsing, config parsing, async runtime setup).
    // If any of these fails, we bail out and report the error without starting the REST API server.
    let env = env::Env::load();
    let args = cli::Cli::args(&env);
    let debug_mode = debug_mode_flag();
    let log = logger_with_dynamic_debug(args.verbosity_filter(), Some(debug_mode));
    let config = nv_common::config::Config::parse(args.config_path())?;

    if args.run_mode() == cli::RunMode::WriteOpenApi && config.openapi_dest_path.is_none() {
        return Err(error::FatalError::NoOpenApiDestPathInOpenApiMode());
    }

    let runtime = nv_common::rt::AsyncRuntime::new(&config)?;
    let api = api::RestApi::new()?;

    if let Some(openapi_dest_path) = &config.openapi_dest_path {
        api.write_openapi(openapi_dest_path.as_ref())?;
    }

    match args.run_mode() {
        cli::RunMode::WriteOpenApi => {}
        cli::RunMode::Normal => {
            // Print the configuration (anything different from
            // default), and then launch the server. From this point
            // on, most errors will just be reported back to users but
            // not actually kill the server; should run continuously
            // unless some catastrophic error is encountered which
            // causes the server to die.
            println!("{config}");
            runtime.block_on(async {
                let _sigusr1_listener = spawn_sigusr1_listener(log.clone());
                api.serve(&config, log).await
            })?;
        }
    }

    Ok(())
}
