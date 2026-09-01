use anyhow::Result;
use camino::Utf8PathBuf;
use nv_common::config::Config;

pub fn command() -> clap::Command {
    clap::Command::new("import")
        .about("Store a validated npm package source")
        .arg(
            clap::Arg::new("file")
                .required(true)
                .value_name("FILE")
                .value_parser(clap::value_parser!(Utf8PathBuf))
                .help("Path to an npm package.json file"),
        )
        .arg(json_argument())
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("package-source import is not implemented")
}

fn json_argument() -> clap::Arg {
    clap::Arg::new("json")
        .long("json")
        .action(clap::ArgAction::SetTrue)
        .help("Print the stored package source as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;
    use clap::error::ErrorKind;

    #[test]
    fn import_requires_a_file() {
        let error = command()
            .try_get_matches_from(["import"])
            .expect_err("missing file should fail");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn import_accepts_a_file_and_json_output() {
        command()
            .try_get_matches_from(["import", "package.json", "--json"])
            .expect("package-source import should parse");
    }
}
