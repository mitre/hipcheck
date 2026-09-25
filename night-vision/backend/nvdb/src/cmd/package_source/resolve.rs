use anyhow::{Context as _, Result};
use indicatif::{ProgressBar, ProgressStyle};
use nv_common::{
	config::Config,
	db,
	npm::elaboration::{
		ClaimedElaboration, ElaborationProgress, ElaborationProgressReporter,
		ElaborationProgressSnapshot, Finalization, elaborate, elaborate_with_progress,
		lifecycle::{AttemptKind, FailureKind, attempt_is_active, begin_attempt, lease_duration},
		storage::persisted_elaboration_warnings,
	},
	rt,
};
use std::{
	io::IsTerminal as _,
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

use super::{ResolvedWarning, print_warnings};

pub fn command() -> clap::Command {
	clap::Command::new("resolve")
		.about("Resolve and persist reachable package versions for a source")
		.arg(source_id_argument())
		.arg(json_argument())
		.arg(no_progress_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let source_id = matches
		.get_one::<String>("source-id")
		.expect("required source ID");
	let progress = (!matches.get_flag("no-progress")).then(PackageSourceResolveProgress::new);
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
	let resolution = runtime.block_on(resolve(config, source_id, progress.clone()));
	if let Some(progress) = &progress {
		progress.finish();
	}
	let result = resolution?;
	if matches.get_flag("json") {
		println!("{}", json_output(source_id, &result));
	} else {
		println!("source_id: {source_id}");
		println!("resolved_packages: {}", result.package_count);
		println!("resolution_snapshot: replaces any previous snapshot");
		print_warnings(&result.warnings);
	}
	Ok(())
}

fn json_output(source_id: &str, result: &ResolutionSummary) -> serde_json::Value {
	serde_json::json!({
		"sourceId": source_id,
		"packages": result.package_count,
		"snapshotBehavior": "replaces-previous",
		"warnings": result.warnings,
	})
}

async fn resolve(
	config: &Config,
	source_id: &str,
	progress: Option<Arc<PackageSourceResolveProgress>>,
) -> Result<ResolutionSummary> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let source = super::source_by_id(&db, source_id).await?;
	let limits = config.package_elaboration_limits();
	let attempt_generation = begin_attempt(
		&db,
		source.id,
		AttemptKind::Manual,
		lease_duration(limits.total_run_timeout),
	)
	.await
	.context("failed to claim package-source attempt")?
	.context("package source is cancelled, deleting, or already being processed")?;
	let attempt = ClaimedElaboration::new(&db, source.id, attempt_generation);
	let setup = ClaimedElaboration::prepare(
		source.file_contents.as_bytes(),
		config.npm_registry_url.clone(),
		&limits,
		config.package_elaboration_max_packument_bytes,
	);
	let result = match setup {
		Ok((source_document, client)) => match &progress {
			Some(progress) => {
				let provider = Arc::new(client);
				let elaboration =
					elaborate_with_progress(&source_document, provider, limits, progress.clone());
				tokio::pin!(elaboration);
				tokio::select! {
					result = &mut elaboration => result,
					() = wait_for_inactive_attempt(&db, source.id, attempt_generation) => {
						anyhow::bail!("package source was cancelled or deleted while resolution was running");
					}
				}
			}
			None => {
				let provider = Arc::new(client);
				let elaboration = elaborate(&source_document, provider, limits);
				tokio::pin!(elaboration);
				tokio::select! {
					result = &mut elaboration => result,
					() = wait_for_inactive_attempt(&db, source.id, attempt_generation) => {
						anyhow::bail!("package source was cancelled or deleted while resolution was running");
					}
				}
			}
		},
		Err(error) => {
			// The claim is already durable; setup failures must release its
			// lease and record their classification before returning to CLI.
			let failure_kind = error.failure_kind();
			return match attempt.finalize(Err(failure_kind)).await {
				Ok(_) => Err(anyhow::Error::new(error)),
				Err(finalize_error) => Err(anyhow::Error::new(finalize_error)
					.context("failed to record elaboration failure")),
			};
		}
	};
	match result {
		Ok(result) => {
			let package_count = result.packages.len();
			if let Some(progress) = &progress {
				progress.publishing_snapshot();
			}
			match attempt
				.finalize(Ok(result))
				.await
				.context("failed to persist elaboration result")?
			{
				Finalization::Completed => {}
				Finalization::Failed(_) => anyhow::bail!("failed to persist elaboration result"),
				Finalization::Inactive => {
					anyhow::bail!(
						"package source was cancelled or deleted while resolution was running"
					);
				}
			}
			let warnings = persisted_elaboration_warnings(&db, source.id)
				.await
				.context("failed to read persisted elaboration warnings")?
				.into_iter()
				.map(ResolvedWarning::from)
				.collect();
			Ok(ResolutionSummary {
				package_count,
				warnings,
			})
		}
		Err(error) => {
			attempt
				.finalize(Err(FailureKind::from(&error)))
				.await
				.context("failed to record elaboration failure")?;
			Err(error.into())
		}
	}
}

async fn wait_for_inactive_attempt(
	db: &sea_orm::DatabaseConnection,
	source_id: i32,
	attempt_generation: i32,
) {
	loop {
		tokio::time::sleep(Duration::from_millis(250)).await;
		match attempt_is_active(db, source_id, attempt_generation).await {
			Ok(true) | Err(_) => {}
			Ok(false) => return,
		}
	}
}

struct ResolutionSummary {
	package_count: usize,
	warnings: Vec<ResolvedWarning>,
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

fn no_progress_argument() -> clap::Arg {
	clap::Arg::new("no-progress")
		.long("no-progress")
		.action(clap::ArgAction::SetTrue)
		.help("Suppress transient resolution progress on stderr")
}

const NON_TERMINAL_PROGRESS_INTERVAL: Duration = Duration::from_secs(5);

trait ProgressLineWriter: Send + Sync {
	fn write_line(&self, line: &str);
}

impl<F> ProgressLineWriter for F
where
	F: Fn(&str) + Send + Sync,
{
	fn write_line(&self, line: &str) {
		self(line);
	}
}

enum ProgressDisplay {
	Terminal(ProgressBar),
	NonTerminal {
		writer: Arc<dyn ProgressLineWriter>,
		last_snapshot: Mutex<Option<Instant>>,
	},
}

struct PackageSourceResolveProgress {
	display: ProgressDisplay,
}

impl PackageSourceResolveProgress {
	fn new() -> Arc<Self> {
		Self::with_terminal(
			std::io::stderr().is_terminal(),
			Arc::new(|line: &str| eprintln!("{line}")),
		)
	}

	fn with_terminal(is_terminal: bool, writer: Arc<dyn ProgressLineWriter>) -> Arc<Self> {
		let display = if is_terminal {
			let bar = ProgressBar::new_spinner();
			bar.set_style(
				ProgressStyle::with_template("{spinner:.cyan} {msg} {elapsed_precise}")
					.expect("spinner progress template should be valid"),
			);
			bar.enable_steady_tick(Duration::from_millis(100));
			ProgressDisplay::Terminal(bar)
		} else {
			ProgressDisplay::NonTerminal {
				writer,
				last_snapshot: Mutex::new(None),
			}
		};
		Arc::new(Self { display })
	}

	fn publishing_snapshot(&self) {
		self.set_phase("publishing resolution snapshot");
	}

	fn finish(&self) {
		if let ProgressDisplay::Terminal(bar) = &self.display {
			bar.finish_and_clear();
		}
	}

	fn set_phase(&self, phase: &str) {
		match &self.display {
			ProgressDisplay::Terminal(bar) => bar.set_message(phase.to_owned()),
			ProgressDisplay::NonTerminal { writer, .. } => writer.write_line(phase),
		}
	}

	fn set_snapshot(&self, snapshot: ElaborationProgressSnapshot) {
		let message = progress_snapshot_message(snapshot);
		match &self.display {
			ProgressDisplay::Terminal(bar) => bar.set_message(message),
			ProgressDisplay::NonTerminal {
				writer,
				last_snapshot,
			} => {
				let mut last_snapshot = last_snapshot
					.lock()
					.expect("progress update lock is not poisoned");
				if last_snapshot.is_none_or(|last| last.elapsed() >= NON_TERMINAL_PROGRESS_INTERVAL)
				{
					writer.write_line(&message);
					*last_snapshot = Some(Instant::now());
				}
			}
		}
	}
}

impl ElaborationProgressReporter for PackageSourceResolveProgress {
	fn report(&self, progress: ElaborationProgress) {
		match progress {
			ElaborationProgress::Started => self.set_phase("resolving reachable package versions"),
			ElaborationProgress::PackumentFetchStarted { package } => {
				if let ProgressDisplay::Terminal(bar) = &self.display {
					bar.println(format!("fetching packument for {package}"));
				}
			}
			ElaborationProgress::PackumentFetchCompleted { .. } => {}
			ElaborationProgress::SchedulerUpdated { snapshot }
			| ElaborationProgress::Finished { snapshot } => self.set_snapshot(snapshot),
		}
	}
}

fn progress_snapshot_message(snapshot: ElaborationProgressSnapshot) -> String {
	format!(
		"{} packages · {} edges · {} paths · {} active · {} queued · {} completed",
		snapshot.packages,
		snapshot.edges,
		snapshot.derivations,
		snapshot.in_flight_work,
		snapshot.queued_work,
		snapshot.completed_work_items,
	)
}

#[cfg(test)]
mod tests {
	use super::{
		PackageSourceResolveProgress, ResolutionSummary, command, json_output,
		progress_snapshot_message,
	};
	use nv_common::npm::elaboration::{
		ElaborationProgress, ElaborationProgressReporter as _, ElaborationProgressSnapshot,
	};
	use std::sync::{Arc, Mutex};

	#[test]
	fn resolve_accepts_a_source_id() {
		command()
			.try_get_matches_from(["resolve", "source-1", "--json"])
			.expect("package-source resolve should parse");
	}

	#[test]
	fn resolve_accepts_no_progress_with_json_output() {
		command()
			.try_get_matches_from(["resolve", "source-1", "--json", "--no-progress"])
			.expect("package-source resolve should accept --no-progress");
	}

	#[test]
	fn non_terminal_progress_throttles_snapshots() {
		let lines = Arc::new(Mutex::new(Vec::new()));
		let writer_lines = lines.clone();
		let progress = PackageSourceResolveProgress::with_terminal(
			false,
			Arc::new(move |line: &str| {
				writer_lines
					.lock()
					.expect("progress line lock is not poisoned")
					.push(line.to_owned());
			}),
		);
		let snapshot = ElaborationProgressSnapshot {
			packages: 4,
			edges: 7,
			derivations: 9,
			queued_work: 2,
			in_flight_work: 3,
			completed_work_items: 1,
		};

		progress.report(ElaborationProgress::Started);
		progress.report(ElaborationProgress::SchedulerUpdated { snapshot });
		progress.report(ElaborationProgress::SchedulerUpdated { snapshot });
		progress.publishing_snapshot();

		assert_eq!(
			lines
				.lock()
				.expect("progress line lock is not poisoned")
				.as_slice(),
			[
				"resolving reachable package versions",
				progress_snapshot_message(snapshot).as_str(),
				"publishing resolution snapshot",
			]
		);
	}

	#[test]
	fn resolve_json_output_describes_the_published_snapshot() {
		let output = json_output(
			"source-1",
			&ResolutionSummary {
				package_count: 2,
				warnings: Vec::new(),
			},
		);

		assert_eq!(
			output,
			serde_json::json!({
				"sourceId": "source-1",
				"packages": 2,
				"snapshotBehavior": "replaces-previous",
				"warnings": [],
			})
		);
	}
}
