use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    db::{self, entities::cisa_kev_entries},
    rt,
};
use sea_orm::{EntityTrait as _, QueryOrder as _, QuerySelect as _};
use serde_json::json;

const DEFAULT_LIMIT: u64 = 50;

pub fn command() -> clap::Command {
    clap::Command::new("list")
        .about("List cached KEV entries")
        .arg(
            clap::Arg::new("limit")
                .long("limit")
                .conflicts_with("no-limit")
                .value_name("N")
                .default_value(DEFAULT_LIMIT.to_string())
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Maximum number of KEV entries to list"),
        )
        .arg(
            clap::Arg::new("no-limit")
                .long("no-limit")
                .action(clap::ArgAction::SetTrue)
                .help("List all cached KEV entries without a limit"),
        )
        .arg(
            clap::Arg::new("desc")
                .long("desc")
                .action(clap::ArgAction::SetTrue)
                .help("List CVE IDs in descending order"),
        )
        .arg(
            clap::Arg::new("json")
                .long("json")
                .action(clap::ArgAction::SetTrue)
                .help("Print cached KEV entries as JSON"),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let filter = KevListFilter::from_matches(matches);
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

    runtime.block_on(list(config, &filter))
}

async fn list(config: &Config, filter: &KevListFilter) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let mut query = cisa_kev_entries::Entity::find();
    query = if filter.descending {
        query.order_by_desc(cisa_kev_entries::Column::CveId)
    } else {
        query.order_by_asc(cisa_kev_entries::Column::CveId)
    };

    if let Some(limit) = filter.limit {
        query = query.limit(
            limit
                .checked_add(1)
                .expect("KEV list limit should fit in u64"),
        );
    }

    let mut entries = query
        .all(&db)
        .await
        .context("failed to read cached KEV entries")?;
    let entry_count = u64::try_from(entries.len()).expect("entry count should fit in u64");
    let truncated_at = filter.limit.filter(|limit| entry_count > *limit);
    if let Some(limit) = truncated_at {
        entries.truncate(
            usize::try_from(limit).expect("truncated KEV list limit should fit in usize"),
        );
    }

    if filter.output_json {
        print_json(&entries)?;
    } else {
        print_entries(&entries);
    }
    if let Some(limit) = truncated_at {
        print_limit_warning(limit);
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct KevListFilter {
    limit: Option<u64>,
    descending: bool,
    output_json: bool,
}

impl KevListFilter {
    fn from_matches(matches: &clap::ArgMatches) -> Self {
        Self {
            limit: (!matches.get_flag("no-limit")).then(|| {
                *matches
                    .get_one::<u64>("limit")
                    .expect("limit has a default value")
            }),
            descending: matches.get_flag("desc"),
            output_json: matches.get_flag("json"),
        }
    }
}

fn print_entries(entries: &[cisa_kev_entries::Model]) {
    if entries.is_empty() {
        println!("kev_entries: <none>");
        return;
    }

    for entry in entries {
        println!("{}", entry.cve_id);
    }
}

fn print_json(entries: &[cisa_kev_entries::Model]) -> Result<()> {
    let entries: Vec<_> = entries
        .iter()
        .map(|entry| {
            json!({
                "cve_id": entry.cve_id,
                "entry": entry.entry,
                "first_seen_at": entry.first_seen_at.to_string(),
                "last_seen_at": entry.last_seen_at.to_string(),
                "updated_at": entry.updated_at.to_string(),
            })
        })
        .collect();
    let output = serde_json::to_string_pretty(&entries)
        .context("failed to serialize cached KEV entries as JSON")?;

    println!("{output}");
    Ok(())
}

fn print_limit_warning(limit: u64) {
    eprintln!(
        "warning: more than {limit} KEV entries matched; output was limited to {limit}. \\
         To see the full list, run `nvdb kev list --no-limit` or \\
         `nvdb kev list --no-limit --json`."
    );
}

#[cfg(test)]
mod tests {
    use super::{KevListFilter, command};

    #[test]
    fn kev_list_accepts_no_filters() {
        let matches = command()
            .try_get_matches_from(["list"])
            .expect("list should parse");

        assert_eq!(
            KevListFilter::from_matches(&matches),
            KevListFilter {
                limit: Some(50),
                descending: false,
                output_json: false,
            }
        );
    }

    #[test]
    fn kev_list_accepts_output_options() {
        let matches = command()
            .try_get_matches_from(["list", "--limit", "25", "--desc", "--json"])
            .expect("list should parse");

        assert_eq!(
            KevListFilter::from_matches(&matches),
            KevListFilter {
                limit: Some(25),
                descending: true,
                output_json: true,
            }
        );
    }

    #[test]
    fn kev_list_accepts_no_limit() {
        let matches = command()
            .try_get_matches_from(["list", "--no-limit"])
            .expect("list should parse");

        assert_eq!(KevListFilter::from_matches(&matches).limit, None);
    }

    #[test]
    fn kev_list_rejects_limit_and_no_limit_together() {
        command()
            .try_get_matches_from(["list", "--limit", "10", "--no-limit"])
            .expect_err("limit and no-limit should conflict");
    }
}
