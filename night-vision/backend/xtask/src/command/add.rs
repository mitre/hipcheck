use anyhow::Result;
use clap::ArgMatches;
use xshell::Shell;

pub fn add(args: &ArgMatches) -> Result<()> {
	let all: Vec<_> = args.get_many::<String>("all").unwrap().collect();

	let s = Shell::new()?;
	xshell::cmd!(s, "cargo add {all...}").run()?;
	xshell::cmd!(s, "cargo hakari generate").run()?;
	xshell::cmd!(s, "cargo autoinherit").run()?;

	Ok(())
}
