use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    db::{self, entities::package_versions},
    rt,
};
use sea_orm::{ColumnTrait as _, EntityTrait as _, QueryFilter as _, QueryOrder as _};

pub fn command() -> clap::Command {
    clap::Command::new("versions")
        .about("List package versions resolved from a source")
        .arg(source_id_argument())
        .arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let source_id = matches
        .get_one::<String>("source-id")
        .expect("required source ID");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let versions = runtime.block_on(versions(config, source_id))?;
    if matches.get_flag("json") {
        println!("{}", serde_json::json!({ "versions": versions }));
    } else {
        for version in versions {
            println!("{version}");
        }
    }
    Ok(())
}

async fn versions(config: &Config, source_id: &str) -> Result<Vec<String>> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let source = super::source_by_id(&db, source_id).await?;
    package_versions::Entity::find()
        .filter(package_versions::Column::SourceId.eq(source.id))
        .order_by_asc(package_versions::Column::PackageUrl)
        .all(&db)
        .await
        .context("failed to read resolved package versions")
        .map(|versions| {
            versions
                .into_iter()
                .map(|version| version.package_url)
                .collect()
        })
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
        .help("Print resolved package versions as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn versions_accepts_a_source_id() {
        command()
            .try_get_matches_from(["versions", "source-1", "--json"])
            .expect("package-source versions should parse");
    }
}
