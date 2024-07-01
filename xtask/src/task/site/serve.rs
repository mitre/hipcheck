// SPDX-License-Identifier: Apache-2.0

use crate::workspace;
use crate::SiteEnvironment;
use crate::SiteServeArgs;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use pathbuf::pathbuf;
use std::thread::spawn;
use xshell::Shell;

macro_rules! cmd {
	($cmd:literal) => {
		move || {
			let sh = Shell::new()?;
			sh.change_dir(pathbuf![&workspace::root()?, "site"]);
			xshell::cmd!(sh, $cmd)
				.quiet()
				// Really, truly be quiet.
				.ignore_stdout()
				.ignore_stderr()
				.run()?;
			Ok::<(), anyhow::Error>(())
		}
	};
}

/// Execute the `site serve` task.
pub fn run(args: SiteServeArgs) -> Result<()> {
	// Check our dependencies.
	which::which("tailwindcss").context("`site serve` requires `tailwindcss` to be installed")?;
	which::which("zola").context("`site serve` requires `zola` to be installed")?;

	// When building for a dev environment, specify the base_url explicitly.
	// Otherwise use the default one from the Zola configuration.
	let base_url = match args.env() {
		SiteEnvironment::Dev => Some("--base-url localhost"),
		SiteEnvironment::Prod => None,
	};

	// Start the subcommands.
	let handles = vec![
		spawn(cmd!("zola serve {base_url...}")),
		spawn(cmd!(
			"tailwindcss -i styles/main.css -o public/main.css --watch=always"
		)),
	];

	// Let the user know the site is up.
	log::info!("Serving at 'localhost:1111'. Press Ctrl-C to stop.");

	// Wait for the subcommands and report errors.
	let mut error = false;
	for handle in handles {
		match handle.join() {
			Ok(Ok(_)) => {}
			Ok(Err(err)) => {
				log::error!("{}", err);
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
