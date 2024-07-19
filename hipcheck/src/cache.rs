// SPDX-License-Identifier: Apache-2.0

use crate::error::Result;
use crate::hc_error;
use clap::{ArgAction, Args, Parser, Subcommand};
use pathbuf::pathbuf;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;
use std::time::SystemTime;
use walkdir::{DirEntry, FilterEntry, IntoIter, WalkDir};

#[derive(Debug, Clone, Subcommand)]
#[command(arg_required_else_help = true)]
pub enum CacheSubcmds {
	/// List existing caches.
	List(CacheTarget),
	/// Delete existing caches.
	Delete(CacheTarget),
}

#[derive(Debug, Clone, Args)]
#[command(arg_required_else_help = true)]
pub struct CacheTarget {
	#[clap(subcommand)]
	pub command: Option<CacheOpTarget>,

	pub pattern: Option<String>,
}

#[derive(Copy, Clone, Debug, Subcommand)]

pub enum CacheSelect {
	Oldest {
		n: usize,
	},

	Largest {
		n: usize,
	},

	Alpha {
		n: usize
	}
}

#[derive(Debug, Clone)]
pub enum CacheSort {
	Oldest,
	Largest,
	Alpha,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CacheOpTarget {
	#[command(flatten)]
	Group(CacheSelect),

	All,
}

#[derive(Debug, Clone)]
struct CacheEntry {
	pub name: String,
	pub parent: PathBuf,
	pub modified: SystemTime,
	pub size: usize,
}
impl TryFrom<DirEntry> for CacheEntry {
	type Error = crate::error::Error;
	fn try_from(value: DirEntry) -> Result<Self> {
		let mut path = value.into_path();
		// Remove ".git"
		path.pop();
		let name: String = path
			.as_path()
			.file_name()
			.ok_or(hc_error!("cache directory doesn't have a name"))?
			.to_str()
			.unwrap()
			.to_owned();
		let md = std::fs::metadata(path.as_path())?;
		let mut parent = path.clone();
		parent.pop();
		Ok(CacheEntry {
			name,
			parent,
			modified: md.modified()?,
			size: 0,
		})
	}
}

struct HcCacheIterator {
	root: PathBuf,
	wd: Box<dyn Iterator<Item = StdResult<DirEntry, walkdir::Error>>>,
}
impl HcCacheIterator {
	pub fn new(dir: &Path) -> Self {
		HcCacheIterator {
			root: PathBuf::from(dir),
			wd: Box::new(
				WalkDir::new(dir)
					.into_iter()
					.filter_entry(|e| e.path().is_dir()),
			),
		}
	}
}
impl Iterator for HcCacheIterator {
	type Item = CacheEntry;
	fn next(&mut self) -> Option<Self::Item> {
		loop {
			if let Some(Ok(e)) = self.wd.next() {
				if e.file_name().to_str().map(|s| s == ".git").unwrap_or(false) {
					match TryInto::<CacheEntry>::try_into(e) {
						Ok(ce) => {
							return Some(ce);
						}
						Err(err) => {
							println!("Err: {err:?}");
						}
					}
				}
			} else {
				return None;
			}
		}
	}
}

pub struct HcCache {
	path: PathBuf,
	entries: Vec<CacheEntry>,
}
impl HcCache {
	pub fn new(path: &Path) -> Self {
		let clones_path = pathbuf![path, "clones"];
		let entries: Vec<CacheEntry> = HcCacheIterator::new(clones_path.as_path()).collect();
		HcCache {
			path: clones_path,
			entries,
		}
	}
	pub fn delete_n(&mut self, sort: CacheSort, n: usize) {}
	pub fn delete_match(&mut self, pattern: String) {}
	pub fn clear(&mut self) {}
	pub fn list_n(&self, sort: CacheSort, n: Option<usize>) {}
	pub fn list_match(&self, pattern: String) {}
}
