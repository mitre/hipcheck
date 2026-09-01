use anyhow::Result;
use nv_common::config::Config;

pub fn command() -> clap::Command {
    clap::Command::new("resolve")
        .about("Resolve and persist reachable package versions for a source")
        .arg(source_id_argument())
        .arg(json_argument())
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("package-source resolve is not implemented")
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
        .help("Print the resolution summary as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn resolve_accepts_a_source_id() {
        command()
            .try_get_matches_from(["resolve", "source-1", "--json"])
            .expect("package-source resolve should parse");
    }
}
