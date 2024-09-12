// SPDX-License-Identifier: Apache-2.0

//! Tasks to list or create RFDs

use crate::{NewRfdArgs, RfdArgs};
use anyhow::{anyhow, Result};
use convert_case::{Case, Casing as _};
use glob::{glob, Paths};
use pathbuf::pathbuf;
use std::{fs::File, path::PathBuf};

/// Run the `rfd` command.
pub fn run(args: RfdArgs) -> Result<()> {
	match args.command {
		crate::RfdCommand::List => list(),
		crate::RfdCommand::New(args) => new(args),
	}
}

/// List all the RFDs checked into the repo.
fn list() -> Result<()> {
	for rfd in rfds()? {
		log::warn!("RFD #{}: {}", rfd.id_string, rfd.title);
	}

	Ok(())
}

/// Create a new RFD.
fn new(args: NewRfdArgs) -> Result<()> {
	let root = crate::workspace::root()?;

	let id = match args.number {
		Some(id) => id,
		None => guess_next_id()?,
	};

	let title = args.title.to_case(Case::Kebab);
	let file_name = format!("{:04}-{}.md", id, title);
	let path = pathbuf![&root, "site", "content", "rfds", &file_name];
	let _ = File::create_new(path)?;
	log::warn!(
		"created draft RFD #{}: \"{}\", at '{}'",
		id,
		args.title,
		pathbuf!["docs", "rfds", &file_name].display()
	);
	Ok(())
}

fn guess_next_id() -> Result<u16> {
	let highest_id = rfds()?.fold(0, |highest_id, rfd| {
		if rfd.id > highest_id {
			rfd.id
		} else {
			highest_id
		}
	});

	highest_id
		.checked_add(1)
		.ok_or_else(|| anyhow!("RFD IDs overflow a u16"))
}

/// Get an iterator over all RFDs.
fn rfds() -> Result<RfdIter> {
	RfdIter::new()
}

/// A single RFD.
#[derive(Debug)]
struct Rfd {
	#[allow(unused)]
	/// The path to the RFD file.
	path: PathBuf,

	#[allow(unused)]
	/// The name of the RFD file.
	file_name: String,

	/// The numeric ID of the RFD.
	id: u16,

	/// The left-0 padded form of the RFD ID.
	id_string: String,

	/// The title of the RFD.
	title: String,
}

/// An iterator over RFDs.
#[derive(Debug)]
struct RfdIter {
	/// The internal glob-based iterator.
	paths: Paths,
}

impl RfdIter {
	/// Get a new RFD iterator.
	fn new() -> Result<Self> {
		let root = crate::workspace::root()?;
		let pattern = format!("{}/docs/rfds/*.md", root.display());
		let paths = glob(&pattern)?;
		Ok(RfdIter { paths })
	}
}

impl Iterator for RfdIter {
	type Item = Rfd;

	// This is a bit of a weird iterator. Basically, we log errors but don't return
	// `None`, for them, preferring instead to call `self.next()` again, effectively
	// behaving like a `continue` in a `for`-loop.
	fn next(&mut self) -> Option<Self::Item> {
		match self.paths.next() {
			Some(Ok(path)) => {
				// Get the possibly-not-UTF-8 file name.
				let raw_file_name = match path.file_name() {
					Some(file_name) => file_name,
					None => {
						log::warn!("path has no file name: {}", path.display());
						return self.next();
					}
				};

				// Get the file name as UTF-8.
				let file_name = match raw_file_name.to_str() {
					Some(file_name) => file_name,
					None => {
						log::warn!(
							"file name is not UTF-8: {}",
							raw_file_name.to_string_lossy()
						);
						return self.next();
					}
				};

				// Skip the README
				if file_name == "README.md" {
					return self.next();
				}

				// Split the ID from the title portion of the file name.
				let (id_string, name) = match file_name.split_once('-') {
					Some(pair) => pair,
					None => {
						log::warn!("invalid RFD file name found: {}", file_name);
						return self.next();
					}
				};

				// Put the title into the correct format.
				let title = name
					.strip_suffix(".md")
					.unwrap_or(name)
					.to_case(Case::Title);

				// Parse the numeric ID.
				let id = match id_string.trim_start_matches('0').parse().ok() {
					Some(id) => id,
					None => {
						log::warn!("invalid ID found: {}", id_string);
						return self.next();
					}
				};

				// And we're done!
				let rfd = Rfd {
					path: path.clone(),
					file_name: file_name.to_owned(),
					id,
					id_string: id_string.to_owned(),
					title,
				};

				Some(rfd)
			}
			Some(Err(e)) => {
				log::warn!("{}", e);
				self.next()
			}
			None => None,
		}
	}
}
