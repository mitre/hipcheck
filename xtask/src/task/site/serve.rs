// SPDX-License-Identifier: Apache-2.0

use crate::{workspace, SiteEnvironment, SiteServeArgs};
use anyhow::{anyhow, Context, Result};
use pathbuf::pathbuf;
use std::thread::spawn;
use xshell::Shell;

macro_rules! cmd {
	($cmd:literal) => {
		move || {
			let sh = Shell::new()?;
			sh.change_dir(pathbuf![&workspace::root()?, "site"]);
			xshell::cmd!(sh, $cmd).quiet().run()?;
			Ok::<(), anyhow::Error>(())
		}
	};

	($cmd:literal, from: $path:literal) => {
		move || {
			let sh = Shell::new()?;
			sh.change_dir(pathbuf![&workspace::root()?, "site", $path]);
			xshell::cmd!(sh, $cmd).quiet().run()?;
			Ok::<(), anyhow::Error>(())
		}
	};
}

/// Execute the `site serve` task.
pub fn run(args: SiteServeArgs) -> Result<()> {
	// Check our dependencies.
	which::which("tailwindcss").context("`site serve` requires `tailwindcss` to be installed")?;
	which::which("zola").context("`site serve` requires `zola` to be installed")?;
	which::which("deno").context("`site serve` requires `deno` to be installed")?;

	// When building for a dev environment, specify the base_url explicitly.
	// Otherwise use the default one from the Zola configuration.
	let base_url = match args.env() {
		SiteEnvironment::Dev => &["--base-url", "localhost"][..],
		SiteEnvironment::Prod => &[],
	};

	// Start the subcommands.
	let handles = vec![
		("zola", spawn(cmd!("zola serve {base_url...}"))),
		(
			"tailwind",
			spawn(cmd!(
				"tailwindcss -i styles/main.css -o public/main.css --watch=always"
			)),
		),
		("deno", spawn(cmd!("deno task dev", from: "scripts"))),
	];

	// Let the user know the site is up.
	log::info!("Serving at 'localhost:1111'. Press Ctrl-C to stop.");

	// Wait for the subcommands and report errors.
	let mut error = false;
	for handle in handles {
		match handle.1.join() {
			Ok(Ok(_)) => {}
			Ok(Err(err)) => {
				for err in err.chain() {
					log::error!("{}: {}", handle.0, err);
				}

				error = true;
			}
			Err(_) => {
				log::error!("an unknown error occured");
				error = true;
			}
		}
	}

	if error {
		return Err(anyhow!("1 or more errors occured during site generation"));
	}

	Ok(())
}
