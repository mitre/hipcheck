use anyhow::{Context as _, Result};
use camino::Utf8PathBuf;
use nv_common::{
    config::Config,
    db::{self, entities::package_sources},
    npm::package_json::NpmPackageJson,
    rt,
};
use sea_orm::{ActiveModelTrait as _, ActiveValue::Set};
use std::fs;
use uuid::Uuid;

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

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let path = matches
        .get_one::<Utf8PathBuf>("file")
        .expect("required file");
    let contents = fs::read_to_string(path).context("failed to read package source")?;
    NpmPackageJson::parse_package_json(contents.as_bytes()).context("invalid npm package.json")?;
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let id = runtime.block_on(store(config, path, contents))?;
    if matches.get_flag("json") {
        println!("{}", json_output(&id));
    } else {
        println!("{id}");
    }
    Ok(())
}

fn json_output(id: &str) -> serde_json::Value {
    serde_json::json!({ "id": id })
}

async fn store(config: &Config, path: &Utf8PathBuf, contents: String) -> Result<String> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let id = Uuid::now_v7().to_string();
    package_sources::ActiveModel {
        source_id: Set(id.clone()),
        display_name: Set("imported package source".to_owned()),
        file_name: Set(path.file_name().unwrap_or("package.json").to_owned()),
        file_contents: Set(contents),
        inferred_type: Set("npm-package-json".to_owned()),
        resolution_status: Set("pending".to_owned()),
        resolution_error: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .context("failed to store package source")?;
    Ok(id)
}

fn json_argument() -> clap::Arg {
    clap::Arg::new("json")
        .long("json")
        .action(clap::ArgAction::SetTrue)
        .help("Print the stored package source as JSON")
}

#[cfg(test)]
mod tests {
    use super::{command, json_output};
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

    #[test]
    fn import_json_output_contains_the_source_id() {
        assert_eq!(
            json_output("source-1"),
            serde_json::json!({"id": "source-1"})
        );
    }
}
