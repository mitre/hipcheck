use crate::NewRfdArgs;
use crate::RfdArgs;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use convert_case::Case;
use convert_case::Casing as _;
use glob::glob;

/// Run the `rfd` command.
pub fn run(args: RfdArgs) -> Result<()> {
	match args.command {
		crate::RfdCommand::List => list(),
		crate::RfdCommand::New(args) => new(args),
	}
}

fn list() -> Result<()> {
	let root = crate::workspace::root()?;
	let pattern = format!("{}/docs/rfds/*.md", root.display());

	for entry in glob(&pattern)? {
		match entry {
			Err(e) => log::warn!("{}", e),
			Ok(path) => {
				let file_name = path
					.file_name()
					.context("no file name on markdown file")?
					.to_str()
					.ok_or_else(|| anyhow!("invalid RFD file name"))?;

				// Skip the README
				if file_name == "README.md" {
					continue;
				}

				let (id, name) = file_name
					.split_once("-")
					.ok_or_else(|| anyhow!("no numeric delimiter found"))?;

				let name = name.strip_suffix(".md").unwrap_or(name);
				let name = name.to_case(Case::Title);

				log::warn!("RFD #{}: {}", id, name);
			}
		}
	}

	Ok(())
}

fn new(_args: NewRfdArgs) -> Result<()> {
	todo!()
}
