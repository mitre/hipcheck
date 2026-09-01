use anyhow::Result;
use nv_common::config::Config;

pub fn command() -> clap::Command {
    clap::Command::new("show")
        .about("Show package-source metadata and processing state")
        .arg(source_id_argument())
        .arg(json_argument())
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("package-source show is not implemented")
}

fn source_id_argument() -> clap::Arg {
    clap::Arg::new("source-id")
        .required(true)
        .value_name("SOURCE-ID")
        .help("Stored package-source identifier")
}

fn json_argument() -> clap::Arg {
    clap::Arg::new("json")
        .long("json")
        .action(clap::ArgAction::SetTrue)
        .help("Print package-source metadata as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn show_accepts_a_source_id() {
        command()
            .try_get_matches_from(["show", "source-1", "--json"])
            .expect("package-source show should parse");
    }
}
