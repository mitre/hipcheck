// SPDX-License-Identifier: Apache-2.0

use crate::{error::Result, hc_error};
use dialoguer::Confirm;
use pathbuf::pathbuf;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
	borrow::Borrow,
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	result::Result as StdResult,
	time::SystemTime,
};
use tabled::{Table, Tabled};
use walkdir::{DirEntry, WalkDir};

static CACHE_FILE_NAME: &str = "index.json";

#[derive(Debug, Clone)]
pub enum RepoCacheSort {
	Oldest,
	Largest,
	Alpha,
}

#[derive(Debug, Clone)]
pub enum RepoCacheDeleteScope {
	All,
	Group {
		sort: RepoCacheSort,
		invert: bool,
		n: usize,
	},
}

#[derive(Debug, Clone)]
pub struct RepoCacheListScope {
	pub sort: RepoCacheSort,
	pub invert: bool,
	pub n: Option<usize>,
}

#[derive(Debug, Clone, Tabled)]
struct RepoCacheEntry {
	pub name: String,
	#[tabled(display("display_parent"), rename = "path")]
	pub parent: PathBuf,
	pub commit: String,
	#[tabled(display("display_size"))]
	pub size: usize,
	#[tabled(display("display_modified"))]
	pub modified: SystemTime,
}

// Helper funcs for displaying CacheEntry using `tabled` crate
fn display_parent(parent: &Path) -> String {
	parent
		.as_os_str()
		.to_str()
		.unwrap_or("<DISPLAY_ERROR>")
		.to_string()
}

fn display_modified(modified: &SystemTime) -> String {
	let Ok(dur) = modified.duration_since(SystemTime::UNIX_EPOCH) else {
		return "<DISPLAY_ERROR>".to_owned();
	};
	let Some(dt) = chrono::DateTime::<chrono::offset::Utc>::from_timestamp(
		dur.as_secs() as i64,
		dur.subsec_nanos(),
	) else {
		return "<DISPLAY_ERROR>".to_owned();
	};
	let chars = dt.to_rfc2822().chars().collect::<Vec<char>>();
	// Remove unnecessary " +0000" from end of rfc datetime str
	chars[..chars.len() - 6].iter().collect()
}

fn display_size(size: &usize) -> String {
	static ONE_KB: f64 = 1000.0;
	static ONE_MB: f64 = ONE_KB * 1000.0;
	static ONE_GB: f64 = ONE_MB * 1000.0;
	let e_size = *size as f64;
	if e_size > ONE_GB {
		format!("{:.2} GB", e_size / ONE_GB)
	} else if e_size > ONE_MB {
		format!("{:.2} MB", e_size / ONE_MB)
	} else if e_size > ONE_KB {
		format!("{:.2} KB", e_size / ONE_KB)
	} else {
		format!("{:.0} B", e_size)
	}
}

// We store a cached version of the `HcCache` object on disk at the root of the
// `clones` dir in the cache folder. When instantiating an `HcCache` object, we
// load this cached version and use it to short-cut entry size calculation if
// the repo has not changed since the last calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HcRepoCacheDiskEntry {
	pub size: usize,
	pub commit: String,
	pub modified: SystemTime,
}
impl From<RepoCacheEntry> for (PathBuf, HcRepoCacheDiskEntry) {
	fn from(value: RepoCacheEntry) -> (PathBuf, HcRepoCacheDiskEntry) {
		let path = pathbuf![value.parent.as_path(), value.name.as_str()];
		let entry = HcRepoCacheDiskEntry {
			size: value.size,
			commit: value.commit,
			modified: value.modified,
		};
		(path, entry)
	}
}
type HcRepoCacheDisk = HashMap<PathBuf, HcRepoCacheDiskEntry>;
fn try_load(path: &Path) -> Result<HcRepoCacheDisk> {
	let data = fs::read_to_string(path)?;
	let disk: HcRepoCacheDisk = serde_json::from_str(data.as_str())?;
	Ok(disk)
}
fn load_or_get_empty(path: &Path) -> HcRepoCacheDisk {
	try_load(path).unwrap_or_default()
}

fn try_get_last_modified(path: &Path) -> Result<SystemTime> {
	Ok(fs::metadata(path)?.modified()?)
}
pub fn get_last_modified_or_now(path: &Path) -> SystemTime {
	try_get_last_modified(path).unwrap_or(SystemTime::now())
}

/// Starting from a given cache dir, finds and iterates over git repos as "CacheEntry" structs
struct HcRepoCacheIterator {
	root: PathBuf,
	disk: HcRepoCacheDisk,
	wd: Box<dyn Iterator<Item = StdResult<DirEntry, walkdir::Error>>>,
}
impl HcRepoCacheIterator {
	pub fn new(dir: &Path) -> Self {
		let disk_path = pathbuf![dir, CACHE_FILE_NAME];
		HcRepoCacheIterator {
			root: PathBuf::from(dir),
			disk: load_or_get_empty(disk_path.as_path()),
			wd: Box::new(
				WalkDir::new(dir)
					.max_depth(5) // reduce time wasted churning through repos
					.into_iter()
					.filter_entry(|e| e.path().is_dir()),
			),
		}
	}
}
impl HcRepoCacheIterator {
	fn path_to_cache_entry(&self, path: &Path) -> Result<RepoCacheEntry> {
		let name = path
			.file_name()
			.ok_or(hc_error!("cache directory doesn't have a name"))?
			.to_str()
			.unwrap()
			.to_owned();
		let commit = gix::open(path)?
			.head()?
			.id()
			.ok_or_else(|| hc_error!("HEAD does not have an ID"))?
			.shorten()?
			.to_string();
		let modified = get_last_modified_or_now(path);
		let cache_subdir = pathbuf![path.strip_prefix(self.root.as_path()).unwrap()];
		let mut parent = cache_subdir.clone();
		parent.pop();
		let size: usize = match self.disk.get(&cache_subdir) {
			// If existing cache entry exists and is not outdated, use size
			Some(existing) => {
				if modified != existing.modified || commit != existing.commit {
					fs_extra::dir::get_size(path)? as usize
				} else {
					existing.size
				}
			}
			None => fs_extra::dir::get_size(path)? as usize,
		};
		Ok(RepoCacheEntry {
			name,
			parent,
			commit,
			modified,
			size,
		})
	}
}
impl Iterator for HcRepoCacheIterator {
	type Item = RepoCacheEntry;
	fn next(&mut self) -> Option<Self::Item> {
		loop {
			if let Some(Ok(e)) = self.wd.next() {
				if e.file_name().to_str().map(|s| s == ".git").unwrap_or(false) {
					let mut path = e.into_path();
					path.pop(); // Remove ".git"
					if let Ok(ce) = self.path_to_cache_entry(path.as_path()) {
						return Some(ce);
					}
				}
			} else {
				return None;
			}
		}
	}
}

pub struct HcRepoCache {
	path: PathBuf,
	entries: Vec<RepoCacheEntry>,
}
impl HcRepoCache {
	pub fn new(path: &Path) -> Self {
		let clones_path = pathbuf![path, "clones"];
		let entries: Vec<RepoCacheEntry> =
			HcRepoCacheIterator::new(clones_path.as_path()).collect();
		HcRepoCache {
			path: clones_path,
			entries,
		}
	}
	/// Internal function for sorting CacheEntry Vecs
	fn sort<A: Borrow<RepoCacheEntry>>(entries: &mut [A], sort: RepoCacheSort, invert: bool) {
		// Generic allows sort to handle both owned and borrowed lists
		let sort_func: fn(&RepoCacheEntry, &RepoCacheEntry) -> std::cmp::Ordering =
			match (sort, invert) {
				(RepoCacheSort::Alpha, false) => |a, b| a.name.partial_cmp(&b.name).unwrap(),
				(RepoCacheSort::Alpha, true) => |a, b| b.name.partial_cmp(&a.name).unwrap(),
				(RepoCacheSort::Oldest, false) => {
					|a, b| a.modified.partial_cmp(&b.modified).unwrap()
				}
				(RepoCacheSort::Oldest, true) => {
					|a, b| b.modified.partial_cmp(&a.modified).unwrap()
				}
				(RepoCacheSort::Largest, false) => |a, b| b.size.partial_cmp(&a.size).unwrap(),
				(RepoCacheSort::Largest, true) => |a, b| a.size.partial_cmp(&b.size).unwrap(),
			};
		entries.sort_by(|a1: &A, a2: &A| sort_func(a1.borrow(), a2.borrow()));
	}
	/// Delete cache entries
	pub fn delete(
		&mut self,
		scope: RepoCacheDeleteScope,
		filter: Option<String>,
		force: bool,
	) -> Result<()> {
		// Parse filter to regex if provided
		let opt_pat: Option<Regex> = match filter {
			Some(raw_p) => Some(Regex::new(format!("^{raw_p}$").as_str())?),
			None => None,
		};
		// Drain and partition self.entries into two vecs for saving and deletion
		let (to_keep, to_del): (Vec<RepoCacheEntry>, Vec<RepoCacheEntry>) = match scope {
			RepoCacheDeleteScope::All => self.entries.drain(0..).partition(|e| match &opt_pat {
				Some(pat) => !pat.is_match(e.name.as_str()),
				None => false,
			}),
			RepoCacheDeleteScope::Group { sort, invert, n } => {
				// First sort entries in-place in self.entries
				HcRepoCache::sort(&mut self.entries, sort, invert);
				let mut hits = 0;
				// Now get the first N entries that pass filter
				self.entries.drain(0..).partition(|e| {
					let del = match &opt_pat {
						Some(pat) => pat.is_match(e.name.as_str()),
						None => true,
					};
					// passes filter and below max threshold, delete
					if del && hits < n {
						hits += 1;
						false
					// put in do_keep
					} else {
						true
					}
				})
			}
		};
		// At this point self.entries is empty and we have separated the set of
		// entries to delete
		if !to_del.is_empty() {
			if !force {
				// Ask user for confirmation
				println!("You will delete the following entries:");
				self.display(to_del.iter().collect());
				let conf = Confirm::new()
					.with_prompt("Are you sure you want to delete?")
					.interact()
					.unwrap();
				if !conf {
					// Cleanup by returning entries to storage
					self.entries.extend(to_del);
					self.entries.extend(to_keep);
					return Ok(());
				}
			}
			// Delete entries, returning failures back to the self.entries list
			for entry in to_del {
				if let Err(e) = self.internal_delete(&entry) {
					println!("Failed to delete entry '{}': {e}", entry.name);
					self.entries.push(entry)
				}
			}
		}
		self.entries.extend(to_keep);
		// self.entries contains `to_keep` plus any entries that were unsuccessfully
		// deleted
		Ok(())
	}
	/// List cache entries
	pub fn list(&mut self, scope: RepoCacheListScope, filter: Option<String>) -> Result<()> {
		// Parse filter to a regex if provided
		let opt_pat: Option<Regex> = match filter {
			Some(raw_p) => Some(Regex::new(format!("^{raw_p}$").as_str())?),
			None => None,
		};
		// Get a vec of &CacheEntry, filtering if applicable
		let mut filt_entries = self
			.entries
			.iter()
			.filter(|e| match &opt_pat {
				Some(pat) => pat.is_match(e.name.as_str()),
				None => true,
			})
			.collect::<Vec<&RepoCacheEntry>>();

		// Sort filtered entries
		HcRepoCache::sort(&mut filt_entries, scope.sort, scope.invert);

		// Limit to first N if specified
		if let Some(n) = scope.n {
			filt_entries.truncate(n);
		}

		// Display
		self.display(filt_entries);

		Ok(())
	}
	/// Internal helper that performs the actual dir deletion
	fn internal_delete(&mut self, entry: &RepoCacheEntry) -> Result<()> {
		// @Todo - probably should have an enum that categorizes entries as in
		// `github`, `local`, `unknown` and add as field in `RepoCacheEntry`.
		let is_github = entry.parent.as_path().starts_with("github");

		let parent_path = pathbuf![self.path.as_path(), entry.parent.as_path()];
		let path = pathbuf![&parent_path, &entry.name];
		std::fs::remove_dir_all(path)?;

		// Clear owner/org dir if deleting this entry from 'github' made it empty
		if is_github && parent_path.read_dir()?.next().is_none() {
			std::fs::remove_dir_all(parent_path)?;
		}

		Ok(())
	}
	/// Internal helper for list functions
	fn display(&self, to_show: Vec<&RepoCacheEntry>) {
		println!("{}", Table::new(to_show));
	}
}
// This causes the current state of the cache to be written to the cache index
// file when the HcCache instance is dropped. On instantiation, the index file
// is referenced opportunistically, it is not treated as ground-truth about the
// contents of the Hipcheck cache. Therefore, while it is preferable to ensure
// `drop` gets called before program termination, the next `HcCache` instance
// will not be corrupted if a hard-kill occurs.
impl Drop for HcRepoCache {
	fn drop(&mut self) {
		let file_path = pathbuf![self.path.as_path(), CACHE_FILE_NAME];
		// Create a file-oriented representation of the cache
		let mut hc_disk_cache = HcRepoCacheDisk::new();
		for e in self.entries.drain(0..) {
			let (key, val): (PathBuf, HcRepoCacheDiskEntry) = e.into();
			hc_disk_cache.insert(key, val);
		}
		// Jsonify cache and write to file
		let data = match serde_json::to_string(&hc_disk_cache) {
			Ok(d) => d,
			Err(e) => {
				log::debug!("Failed to jsonify Hipcheck cache info: {e}");
				return;
			}
		};
		if let Err(e) = fs::write(file_path, data) {
			log::debug!("Failed to write Hipcheck cache info: {e}");
		}
	}
}
