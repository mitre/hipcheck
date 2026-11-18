use anyhow::Result;

use super::packument;

pub fn command() -> clap::Command {
	clap::Command::new("npm")
		.about("Inspect npm registry data")
		.arg_required_else_help(true)
		.subcommand(packument::command())
}

pub fn run(matches: &clap::ArgMatches) -> Result<()> {
	if let Some(packument_matches) = matches.subcommand_matches("packument") {
		return packument::run(packument_matches);
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::command;

	#[test]
	fn npm_command_accepts_packument_corpus_refresh() {
		command()
			.try_get_matches_from(["npm", "packument", "corpus-refresh", "--destructive"])
			.expect("packument corpus refresh should parse");
	}

	#[test]
	fn npm_command_accepts_packument_corpus_add() {
		command()
			.try_get_matches_from(["npm", "packument", "corpus-add", "example", "--destructive"])
			.expect("packument corpus add should parse");
	}

	#[test]
	fn npm_command_accepts_packument_resolve() {
		command()
			.try_get_matches_from(["npm", "packument", "resolve", "packument.json", "^1.2.3"])
			.expect("packument resolve should parse");
	}
}
