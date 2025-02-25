// SPDX-License-Identifier: Apache-2.0

//! Git related types and implementations for pulling/cloning source repos.

use crate::{
	error::{Error as HcError, Result as HcResult},
	hc_error,
	shell::{progress_phase::ProgressPhase, verbosity::Verbosity, Shell},
};
use console::Term;
use gix::{
	bstr::ByteSlice,
	progress::{prodash::progress::Step, MessageLevel, StepShared, Unit},
	refs::FullName,
	remote, NestedProgress, ObjectId,
};
use std::{
	io::Write,
	path::Path,
	sync::{atomic::Ordering, Arc},
	usize,
};
use url::Url;

// const UPDATE_STDOUT_EVERY_MS: u64 = 1;
const SEPARATOR: &str = "::";

struct GitProgress {
	name: String,
	id: gix::progress::Id,
	step: StepShared,
	max: Option<usize>,
	unit: Option<Unit>,
	verbosity: Verbosity,
	progress_phase: Option<ProgressPhase>,
}

impl GitProgress {
	fn new() -> Self {
		Self::default()
	}

	// fn write_to_shell_stdout(&self, level: MessageLevel, message: String) {
	// 	if matches!(self.verbosity, Verbosity::Quiet | Verbosity::Silent) {
	// 		return;
	// 	}
	// 	// only need to capture DONE messages
	// 	if matches!(level, MessageLevel::Failure | MessageLevel::Info) {
	// 		return;
	// 	}
	//
	// 	// let step = self.step.load(Ordering::Relaxed);
	// 	// if there is a message, use that, otherwise generate one based on available data
	// 	// let message = message.unwrap_or_else(|| match (self.max, &self.unit) {
	// 	// 	(max, Some(unit)) => {
	// 	// 		format!("{}:: -> {}", self.name, unit.display(step, max, None))
	// 	// 	}
	// 	// 	(Some(max), None) => {
	// 	// 		let perc = (step as f32 / max as f32) * 100.0;
	// 	// 		format!("{}: {:03.0}% {}/{}", self.name, perc, step, max)
	// 	// 	}
	// 	// 	(None, None) => format!("{}: {}", self.name, step),
	// 	// });
	// 	//
	// 	// WithSidebands::with_progress_handler(parent, handle_progress)
	//
	// 	Shell::in_suspend(|| {
	// 		let mut stdout = Term::stdout();
	// 		// stdout.clear_line().expect("could not clear line on stdout");
	// 		writeln!(&mut stdout, "remote: {}: {}", self.name, message)
	// 			.expect("could not write to stdout");
	// 		stdout.flush().expect("could not flush stdout");
	// 	})
	// }
}

impl Default for GitProgress {
	fn default() -> Self {
		Self {
			verbosity: Shell::get_verbosity(),
			step: Arc::default(),
			name: String::new(),
			max: None,
			unit: None,
			id: gix::progress::UNKNOWN,
			progress_phase: None,
		}
	}
}

impl gix::Count for GitProgress {
	fn set(&self, step: Step) {
		self.step.store(step, Ordering::SeqCst);
		if let Some(progress) = &self.progress_phase {
			progress.set_position(step as u64);
		}
	}

	fn step(&self) -> Step {
		self.step.load(Ordering::Relaxed)
	}

	fn inc_by(&self, step: Step) {
		self.step.fetch_add(step, Ordering::Relaxed);
		if let Some(progress) = &self.progress_phase {
			progress.inc(step as u64);
		}
	}

	fn counter(&self) -> gix::progress::StepShared {
		self.step.clone()
	}
}

impl gix::progress::Progress for GitProgress {
	fn init(
		&mut self,
		max: Option<gix::progress::prodash::progress::Step>,
		unit: Option<gix::progress::Unit>,
	) {
		self.max = max;
		self.unit = unit;
		// only initialize the progress bar if the Verbosity is normal
		if matches!(self.verbosity, Verbosity::Normal) {
			if let Some(max) = self.max {
				self.progress_phase = Some(ProgressPhase::start(max as u64, self.name.as_str()));
			}
		}
	}

	fn set_name(&mut self, name: String) {
		self.name = self
			.name
			.split(SEPARATOR)
			.next()
			.map(|parent| format!("{}{}{}", parent, SEPARATOR, name))
			.unwrap_or_else(|| name)
	}

	fn name(&self) -> Option<String> {
		self.name.split(SEPARATOR).nth(1).map(ToOwned::to_owned)
	}

	fn id(&self) -> gix::progress::Id {
		self.id
	}

	fn message(&self, level: gix::progress::MessageLevel, message: String) {
		// self.write_to_shell_stdout(level, message);
	}

	fn unit(&self) -> Option<Unit> {
		self.unit.clone()
	}

	fn max(&self) -> Option<gix::progress::prodash::progress::Step> {
		self.max
	}

	fn done(&self, message: String) {
		if let Some(progress) = &self.progress_phase {
			// progress.update_status(message);
			if let Some(max) = self.max {
				progress.set_position(max as u64);
			}
			progress.finish_successful(true);
		}
	}

	fn set_max(&mut self, max: Option<gix::progress::prodash::progress::Step>) -> Option<Step> {
		let prev_max = self.max.take();
		self.max = max;
		if let Some(new_max) = max {
			if let Some(progress) = &self.progress_phase {
				progress.set_length(new_max as u64);
			}
		}
		prev_max
	}
}

impl NestedProgress for GitProgress {
	type SubProgress = GitProgress;

	fn add_child(&mut self, name: impl Into<String>) -> Self::SubProgress {
		self.add_child_with_id(name, gix::progress::UNKNOWN)
	}

	fn add_child_with_id(
		&mut self,
		name: impl Into<String>,
		id: gix::progress::Id,
	) -> Self::SubProgress {
		let name = if self.name.is_empty() {
			name.into()
		} else {
			format!("{}{}{}", self.name, SEPARATOR, name.into())
		};
		GitProgress {
			name,
			id,
			step: Default::default(),
			max: None,
			unit: None,
			verbosity: Shell::get_verbosity(),
			progress_phase: None,
		}
	}
}

/// default options to use when fetching a repo with `gix`
fn fetch_options(url: &Url, dest: &Path) -> gix::clone::PrepareFetch {
	gix::clone::PrepareFetch::new(
		url.as_str(),
		dest,
		gix::create::Kind::WithWorktree,
		gix::create::Options::default(),
		gix::open::Options::default(),
	)
	.expect("fetch options must be valid to perform a clone")
}

/// fast-forward HEAD of repo to a new object ID (SHA1)
///
/// returns new ObjectId (SHA1) of updated HEAD upon success
fn fast_forward_to_hash(
	repo: &gix::Repository,
	current_head: gix::Head,
	new_object_id: gix::ObjectId,
) -> HcResult<ObjectId> {
	let current_id = current_head
		.id()
		.ok_or_else(|| hc_error!("Could not determine hash of current HEAD"))?;

	if current_id == new_object_id {
		log::debug!("skipping fast-forward, IDs match");
		return Ok(current_id.into());
	}
	let edit = gix::refs::transaction::RefEdit {
		change: gix::refs::transaction::Change::Update {
			log: gix::refs::transaction::LogChange {
				mode: gix::refs::transaction::RefLog::AndReference,
				force_create_reflog: false,
				message: format!("fast-forward HEAD from {} to {}", current_id, new_object_id)
					.into(),
			},
			expected: gix::refs::transaction::PreviousValue::Any,
			new: gix::refs::Target::Object(new_object_id),
		},
		name: FullName::try_from("HEAD").unwrap(),
		deref: true,
	};
	log::trace!(
		"attempting fast-forward from {} to {}",
		current_id,
		new_object_id
	);

	// commit change to the repo and the reflog
	repo.refs
		.transaction()
		.prepare(
			[edit],
			gix::lock::acquire::Fail::Immediately,
			gix::lock::acquire::Fail::Immediately,
		)?
		.commit(Some(Default::default()))?;
	log::trace!("fast-forward successful");
	Ok(new_object_id)
}

/// Clone a repo from the given url to a destination path in the filesystem.
pub fn clone(url: &Url, dest: &Path) -> HcResult<()> {
	log::debug!("attempting to clone {} to {:?}", url.as_str(), dest);
	std::fs::create_dir_all(dest)?;
	let mut fetch_options = fetch_options(url, dest);
	let (mut checkout, _) =
		fetch_options.fetch_then_checkout(GitProgress::new(), &gix::interrupt::IS_INTERRUPTED)?;
	let _ = checkout.main_worktree(GitProgress::new(), &gix::interrupt::IS_INTERRUPTED)?;
	log::info!("Successfully cloned {} to {:?}", url.as_str(), dest);
	Ok(())
}

/// For a given repo, checkout a particular ref in a detached HEAD state.
///
/// 1. If a refspec is passed, then attempt to fast-forward to the specified revision
/// 2. If no ref is provided, then attempt to fast-forward repo to HEAD of the default remote.
/// 3. If there is no default remote, then attempt to set HEAD to match upstream of local branch
///
/// If none of these are possible, then error due to inability to infer target
pub fn checkout(repo_path: &Path, refspec: Option<String>) -> HcResult<gix::ObjectId> {
	let repo = gix::open(repo_path)?;
	let head = repo.head()?;

	// if a refspec was given attempt to resolve it, error if unable to resolve
	if let Some(refspec) = refspec {
		log::trace!("attempting to find refspec [{}]", refspec);
		// try refspec as given
		let target = match repo.rev_parse(refspec.as_str()) {
			Ok(rev) => {
				let oid = rev
					.single()
					.ok_or_else(|| hc_error!("ref '{}' was not a unique identifier", refspec))?;
				repo.find_object(oid)?.id
			}
			Err(e) => {
				return Err(hc_error!(
					"Could not find repo with provided refspec: {}",
					e
				));
			}
		};
		log::trace!("found refspec: {:?}", target);
		return fast_forward_to_hash(&repo, head, target);
	}

	// Determine if there is a default remote, if there is determine what it thinks HEAD is and
	// fast-forward to the remote HEAD
	if let Some(Ok(default_remote)) = repo.find_default_remote(remote::Direction::Fetch) {
		if let Some(remote_name) = default_remote.name() {
			if let Ok(mut remote_head) =
				repo.find_reference(format!("refs/remotes/{}/HEAD", remote_name.as_bstr()).as_str())
			{
				let target = remote_head.peel_to_id_in_place()?;
				return fast_forward_to_hash(&repo, head, target.into());
			}
		}
	}

	let mut local_branches = repo.branch_names();
	if local_branches.len() == 1 {
		let mut local_branch = repo.find_reference(
			format!("refs/heads/{}", local_branches.pop_first().unwrap()).as_str(),
		)?;
		let tip_of_local_branch = local_branch.peel_to_id_in_place()?;
		return fast_forward_to_hash(&repo, head, tip_of_local_branch.into());
	}
	Err(HcError::msg("target is ambiguous"))
}

/// TODO: redo commit history to add support for fetch/clone/checkout separately
/// TODO: add support for visual progress indicators
/// Perform a `git fetch` for all remotes in the repo.
pub fn fetch(repo_path: &Path) -> HcResult<()> {
	log::debug!("Fetching: {:?}", repo_path);
	let repo = gix::open(repo_path)?;
	let remote_names = repo.remote_names();
	for remote_name in remote_names {
		log::trace!("Attempt to fetch remote: {}", remote_name.as_bstr());
		let remote = repo.find_remote(remote_name.as_bstr())?;
		remote
			.connect(gix::remote::Direction::Fetch)?
			.prepare_fetch(GitProgress::new(), Default::default())?
			.receive(GitProgress::new(), &gix::interrupt::IS_INTERRUPTED)?;
	}
	Ok(())
}
