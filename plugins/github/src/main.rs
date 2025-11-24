// SPDX-License-Identifier: Apache-2.0

mod api;
mod config;
mod graphql;
mod plugin;
mod redacted;
mod rest;
mod tls;
mod types;

use crate::plugin::GithubAPIPlugin;
use clap::Parser;
use hipcheck_sdk::{LogLevel, prelude::*};

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,

	#[arg(long, default_value_t=LogLevel::Error)]
	log_level: LogLevel,

	#[arg(trailing_var_arg(true), allow_hyphen_values(true), hide = true)]
	unknown_args: Vec<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();

	PluginServer::register(GithubAPIPlugin::new(), args.log_level)
		.listen_local(args.port)
		.await
}
