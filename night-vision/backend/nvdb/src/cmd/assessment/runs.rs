use anyhow::{Context as _, Result};
use nv_common::{
    config::Config, db, db::entities::package_versions, hipcheck::storage::list_hipcheck_runs, rt,
};
use sea_orm::{ColumnTrait as _, EntityTrait as _, QueryFilter as _};

const DEFAULT_LIMIT: u64 = 10;

pub fn command() -> clap::Command {
    clap::Command::new("runs")
        .about("List persisted assessments for a package")
        .arg(
            clap::Arg::new("package")
                .long("package")
                .required(true)
                .value_name("PURL")
                .help("Package URL without a version"),
        )
        .arg(
            clap::Arg::new("limit")
                .long("limit")
                .value_name("N")
                .default_value(DEFAULT_LIMIT.to_string())
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Maximum number of assessments to list"),
        )
        .arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let purl = matches.get_one::<String>("package").expect("required PURL");
    let limit = *matches.get_one::<u64>("limit").expect("default limit");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let runs = runtime.block_on(async {
        let db = db::connection(config).await?;
        let version = package_versions::Entity::find()
            .filter(package_versions::Column::PackageUrl.eq(purl))
            .one(&db)
            .await?
            .context("package version not found")?;
        Ok::<_, anyhow::Error>(list_hipcheck_runs(&db, version.id, limit).await?)
    })?;
    if matches.get_flag("json") {
        let output = runs
            .iter()
            .map(|run| {
                serde_json::json!({
                    "id": run.id,
                    "state": run.status,
                    "recommendation": run.policy_recommendation,
                    "createdAt": run.created_at,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::json!({"runs":output}));
    } else {
        println!("assessments: {}", runs.len());
        for run in runs {
            println!(
                "{} {} {} {}",
                run.id,
                run.status,
                run.policy_recommendation.as_deref().unwrap_or("<none>"),
                run.created_at,
            );
        }
    }
    Ok(())
}

fn json_argument() -> clap::Arg {
    clap::Arg::new("json")
        .long("json")
        .action(clap::ArgAction::SetTrue)
        .help("Print persisted assessments as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;
    use clap::error::ErrorKind;

    #[test]
    fn runs_requires_a_package() {
        let error = command()
            .try_get_matches_from(["runs"])
            .expect_err("missing package should fail");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn runs_accepts_package_limit_and_json_output() {
        command()
            .try_get_matches_from([
                "runs",
                "--package",
                "pkg:npm/example",
                "--limit",
                "20",
                "--json",
            ])
            .expect("assessment runs should parse");
    }
}
