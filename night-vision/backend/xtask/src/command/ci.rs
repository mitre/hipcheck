use crate::workspace::get_workspace_path;
use anyhow::Result;
use clap::ArgMatches;
use xshell::{Shell, cmd};

pub fn ci(_args: &ArgMatches) -> Result<()> {
    let s = Shell::new()?;
    let workspace_path = get_workspace_path()?;
    let _dir = s.push_dir(workspace_path);

    cmd!(s, "cargo xtask lint").run()?;
    cmd!(s, "cargo hakari generate --diff").run()?;
    cmd!(s, "cargo fmt --all --check").run()?;
    cmd!(s, "cargo clippy --locked --workspace -- -D warnings").run()?;
    cmd!(s, "cargo doc --locked --workspace --no-deps")
        .env("RUSTDOCFLAGS", "-Dwarnings")
        .run()?;
    cmd!(s, "cargo nextest r --locked --workspace --profile ci").run()?;

    Ok(())
}
