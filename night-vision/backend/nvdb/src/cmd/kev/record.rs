use anyhow::{Context as _, Result, bail};
use nv_common::{
	config::Config,
	cve::record::CveId,
	db::{
		self,
		entities::{cisa_kev_entries, cisa_kev_entries::Model as KevEntry},
	},
	rt,
};
use sea_orm::EntityTrait as _;

use crate::time_display::TimeDisplay;

pub fn command() -> clap::Command {
	clap::Command::new("record")
		.about("Print one cached KEV entry")
		.arg(
			clap::Arg::new("cve-id")
				.value_name("CVE-ID")
				.required(true)
				.help("CVE identifier to look up"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches, time_display: TimeDisplay) -> Result<()> {
	let cve_id = matches
		.get_one::<String>("cve-id")
		.expect("required CVE ID argument");
	let cve_id = CveId::parse(cve_id).context("invalid CVE ID")?;
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

	runtime.block_on(record(config, &cve_id, time_display))
}

async fn record(config: &Config, cve_id: &CveId, time_display: TimeDisplay) -> Result<()> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let entry = cisa_kev_entries::Entity::find_by_id(cve_id.as_str().to_owned())
		.one(&db)
		.await
		.context("failed to read cached KEV entry")?;
	let Some(entry) = entry else {
		bail!("KEV entry not found: {}", cve_id.as_str());
	};

	print_entry(&entry, time_display)
}

fn print_entry(entry: &KevEntry, time_display: TimeDisplay) -> Result<()> {
	println!("cve_id: {}", entry.cve_id);
	println!(
		"first_seen_at: {}",
		time_display.format(&entry.first_seen_at)
	);
	println!("last_seen_at: {}", time_display.format(&entry.last_seen_at));
	println!("updated_at: {}", time_display.format(&entry.updated_at));
	println!(
		"removed_at: {}",
		time_display.format_optional(entry.removed_at.as_ref())
	);
	println!("entry:");
	let output = serde_json::to_string_pretty(&entry.entry)
		.context("failed to serialize cached KEV entry JSON")?;
	println!("{output}");

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::command;
	use clap::error::ErrorKind;

	#[test]
	fn kev_record_requires_a_cve_id() {
		let error = command()
			.try_get_matches_from(["record"])
			.expect_err("missing CVE ID should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn kev_record_accepts_a_cve_id() {
		command()
			.try_get_matches_from(["record", "CVE-2026-1000"])
			.expect("CVE ID should parse");
	}
}
