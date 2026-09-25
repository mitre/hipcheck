use anyhow::Result;
use nv_common::{
	config::{Config, redact_url_authentication},
	kev::{
		DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS, DEFAULT_KEV_RESPONSE_BODY_MAX_BYTES,
		DEFAULT_KEV_URL,
	},
};

pub fn command() -> clap::Command {
	clap::Command::new("config").about("Print the effective KEV fetch configuration")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
	let kev_url = config.kev_url.as_ref().map_or_else(
		|| DEFAULT_KEV_URL.to_owned(),
		|url| redact_url_authentication(url).to_string(),
	);
	let refresh_interval_ms = config
		.kev_refresh_interval
		.unwrap_or(DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS);
	let response_body_max_bytes = config
		.kev_response_body_max_bytes
		.unwrap_or(DEFAULT_KEV_RESPONSE_BODY_MAX_BYTES);

	println!("kev_url: {kev_url}");
	println!("refresh_interval_ms: {refresh_interval_ms}");
	println!("response_body_max_bytes: {response_body_max_bytes}");

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::command;

	#[test]
	fn kev_config_accepts_no_extra_arguments() {
		command()
			.try_get_matches_from(["config"])
			.expect("config subcommand should parse");
	}
}
