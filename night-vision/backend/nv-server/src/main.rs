#![deny(unsafe_code)]

mod api;
mod cli;
mod config;
mod db;
mod env;
mod error;
mod log;
mod rt;
mod secret;
#[cfg(test)]
mod test_util;

/// Run the Night Vision server.
fn main() -> Result<(), error::FatalError> {
    // Setup steps (env loading, arg parsing, config parsing, async runtime setup).
    // If any of these fails, we bail out and report the error without starting the REST API server.
    let env = env::Env::load();
    let args = cli::Cli::args(&env);
    let config = config::Config::parse(args.config_path())?;

    if args.run_mode() == cli::RunMode::WriteOpenApi && config.openapi_dest_path.is_none() {
        return Err(error::FatalError::NoOpenApiDestPathInOpenApiMode());
    }

    let runtime = rt::AsyncRuntime::new(&config)?;
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
            runtime.block_on(api.serve(&env, &config))?;
        }
    }

    Ok(())
}
