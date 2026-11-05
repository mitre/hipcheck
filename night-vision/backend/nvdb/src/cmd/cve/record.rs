use anyhow::{Context as _, Result, bail};
use nv_common::{
	config::Config,
	cve::record::CveId,
	db::{
		self,
		entities::{cve_list_records, cve_list_records::Model as CveListRecord},
	},
	rt,
};
use sea_orm::EntityTrait as _;
use serde_json::Value;

use crate::time_display::TimeDisplay;

pub fn command() -> clap::Command {
	clap::Command::new("record")
		.about("Look up one stored CVE List record")
		.arg(
			clap::Arg::new("cve-id")
				.value_name("CVE-ID")
				.required(true)
				.help("CVE identifier to look up"),
		)
		.arg(
			clap::Arg::new("json")
				.long("json")
				.action(clap::ArgAction::SetTrue)
				.help("Print the stored upstream JSON"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches, time_display: TimeDisplay) -> Result<()> {
	let cve_id = matches
		.get_one::<String>("cve-id")
		.expect("required CVE ID argument");
	let cve_id = CveId::parse(cve_id).context("invalid CVE ID")?;
	let output_json = matches.get_flag("json");

	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
	runtime.block_on(record(config, &cve_id, output_json, time_display))
}

async fn record(
	config: &Config,
	cve_id: &CveId,
	output_json: bool,
	time_display: TimeDisplay,
) -> Result<()> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let record = cve_list_records::Entity::find_by_id(cve_id.as_str().to_owned())
		.one(&db)
		.await
		.context("failed to read CVE List record")?;
	let Some(record) = record else {
		bail!("CVE record not found: {}", cve_id.as_str());
	};

	if output_json {
		print_json(&record)?;
	} else {
		print_metadata(&record, time_display);
	}

	Ok(())
}

fn print_metadata(record: &CveListRecord, time_display: TimeDisplay) {
	println!("cve_id: {}", record.cve_id);
	println!("record_format_version: {}", record.record_format_version);
	println!(
		"state: {}",
		cve_state(&record.record).unwrap_or("<unknown>")
	);
	println!("deleted: {}", record.deleted);
	println!(
		"first_seen_at: {}",
		time_display.format(&record.first_seen_at)
	);
	println!(
		"last_seen_at: {}",
		time_display.format(&record.last_seen_at)
	);
	println!("updated_at: {}", time_display.format(&record.updated_at));
}

fn print_json(record: &CveListRecord) -> Result<()> {
	let output = serde_json::to_string_pretty(&record.record)
		.context("failed to serialize stored CVE List record JSON")?;
	println!("{output}");
	Ok(())
}

fn cve_state(record: &Value) -> Option<&str> {
	record
		.get("cveMetadata")
		.and_then(Value::as_object)
		.and_then(|metadata| metadata.get("state"))
		.and_then(Value::as_str)
}
