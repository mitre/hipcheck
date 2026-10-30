use anyhow::Result;
use nv_common::{
	config::Config, db::entities::package_sources,
	npm::elaboration::storage::PersistedElaborationWarning,
};
use sea_orm::{ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _};
use serde::Serialize;

pub mod cleanup;
pub mod import;
pub mod kevs;
pub mod resolve;
pub mod show;
pub mod versions;

pub fn command() -> clap::Command {
	clap::Command::new("package-source")
		.about("Manage package sources and their resolved versions")
		.arg_required_else_help(true)
		.subcommand(cleanup::command())
		.subcommand(import::command())
		.subcommand(kevs::command())
		.subcommand(resolve::command())
		.subcommand(show::command())
		.subcommand(versions::command())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	if let Some(import_matches) = matches.subcommand_matches("import") {
		return import::run(config, import_matches);
	}
	if let Some(cleanup_matches) = matches.subcommand_matches("cleanup") {
		return cleanup::run(config, cleanup_matches);
	}

	if let Some(kevs_matches) = matches.subcommand_matches("kevs") {
		return kevs::run(config, kevs_matches);
	}

	if let Some(resolve_matches) = matches.subcommand_matches("resolve") {
		return resolve::run(config, resolve_matches);
	}

	if let Some(show_matches) = matches.subcommand_matches("show") {
		return show::run(config, show_matches);
	}

	if let Some(versions_matches) = matches.subcommand_matches("versions") {
		return versions::run(config, versions_matches);
	}

	Ok(())
}

pub async fn source_by_id(
	db: &DatabaseConnection,
	source_id: &str,
) -> Result<package_sources::Model> {
	package_sources::Entity::find()
		.filter(package_sources::Column::SourceId.eq(source_id))
		.filter(package_sources::Column::ResolutionStatus.ne("deleting"))
		.one(db)
		.await?
		.ok_or_else(|| anyhow::anyhow!("unknown package source {source_id}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedWarning {
	declared_by_purl: Option<String>,
	dependency_name: String,
	specification_kind: String,
	message: String,
}

impl From<PersistedElaborationWarning> for ResolvedWarning {
	fn from(warning: PersistedElaborationWarning) -> Self {
		Self {
			declared_by_purl: warning.declared_by_purl,
			dependency_name: warning.dependency_name,
			specification_kind: warning.specification_kind,
			message: warning.message,
		}
	}
}

pub fn print_warnings(warnings: &[ResolvedWarning]) {
	println!("warnings: {}", warnings.len());
	for warning in warnings {
		println!(
			"  {}: {} ({}) — {}",
			warning.declared_by_purl.as_deref().unwrap_or("<root>"),
			warning.dependency_name,
			warning.specification_kind,
			warning.message,
		);
	}
}
