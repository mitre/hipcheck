use anyhow::Result;
use std::{ops::Not, process::ExitCode};

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{}", e);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let matches = clap::Command::new("xtask")
        .about("Task runner for the Night Vision backend")
        .arg_required_else_help(true)
        .subcommand(
            clap::Command::new("deps")
                .about("Manage dependencies")
                .arg_required_else_help(true)
                .subcommand(clap::Command::new("install").about("Install dependencies"))
                .subcommand(
                    clap::Command::new("list").about("List dependencies and their install state"),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("deps") {
        if let Some(matches) = matches.subcommand_matches("install") {
            install_deps(matches)?;
        }

        if let Some(matches) = matches.subcommand_matches("list") {
            list_deps(matches);
        }
    }

    Ok(())
}

struct Dep {
    binary_name: &'static str,
    install_method: DepInstallMethod,
}

impl Dep {
    fn is_installed(&self) -> bool {
        which::which(self.binary_name).is_ok()
    }

    fn installed_str(&self) -> &'static str {
        if self.is_installed() {
            "installed"
        } else {
            "not installed"
        }
    }
}

enum DepInstallMethod {
    LockedInstall,
    Binstall,
}

fn get_deps() -> &'static [Dep] {
    &[
        Dep {
            binary_name: "cargo-binstall",
            install_method: DepInstallMethod::LockedInstall,
        },
        Dep {
            binary_name: "cargo-nextest",
            install_method: DepInstallMethod::Binstall,
        },
        Dep {
            binary_name: "cargo-autoinherit",
            install_method: DepInstallMethod::Binstall,
        },
        Dep {
            binary_name: "sea-orm-cli",
            install_method: DepInstallMethod::Binstall,
        },
    ]
}

fn install_deps(_matches: &clap::ArgMatches) -> Result<()> {
    let sh = xshell::Shell::new()?;

    for dep in get_deps() {
        if dep.is_installed().not() {
            let binary_name = dep.binary_name;

            match dep.install_method {
                DepInstallMethod::LockedInstall => {
                    xshell::cmd!(sh, "cargo install --locked {binary_name}").run()?
                }
                DepInstallMethod::Binstall => {
                    xshell::cmd!(sh, "cargo binstall {binary_name}").run()?;
                }
            }
        } else {
            println!("{} is already installed", dep.binary_name);
        }
    }

    Ok(())
}

fn list_deps(_matches: &clap::ArgMatches) {
    for dep in get_deps() {
        println!("{}: {}", dep.binary_name, dep.installed_str());
    }
}
