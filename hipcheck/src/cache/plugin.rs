// SPDX-License-Identifier: Apache-2.0
#![allow(unused)] // for ease of testing
use crate::{
	cache::repo,
	error::Result,
	hc_error,
	plugin::{PluginId, PluginName, PluginPublisher, PluginVersion},
	StdResult,
};
use dialoguer::Confirm;
use pathbuf::pathbuf;
use regex::Regex;
use semver::{Version, VersionReq};
use std::{
	borrow::Borrow,
	env, fmt,
	path::{Component, Path, PathBuf},
	time::{Duration, SystemTime},
};
use tabled::{Table, Tabled};
use walkdir::{DirEntry, WalkDir};

/// enums used for sorting are the same as the ones used for listing repo cache entries except for
/// does not include Largest
#[derive(Debug, Clone)]
pub enum PluginCacheSort {
	OldestDate, // same as "Oldest" in repo.rs
	Alpha,
	NewestVersion, // sorts the versions of the plugins from newest to oldest. Ex: 0.2.1 would come before 0.2.0
}

#[derive(Debug, Clone)]
pub enum PluginCacheDeleteScope {
	All,
	Group {
		sort: PluginCacheSort,
		invert: bool,
		n: usize,
	},
}

#[derive(Debug, Clone)]
pub struct PluginCacheListScope {
	pub sort: PluginCacheSort,
	pub invert: bool,
	pub n: Option<usize>,
}

#[derive(Debug, Clone, Tabled)]
pub struct PluginCacheEntry {
	pub publisher: String,
	pub name: String,
	pub version: Version,
	#[tabled(display_with("Self::display_modified", self), rename = "last_modified")]
	pub modified: SystemTime,
}

impl PluginCacheEntry {
	// copied the function from repo for simplicity
	pub fn display_modified(&self) -> String {
		let Ok(dur) = self.modified.duration_since(SystemTime::UNIX_EPOCH) else {
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
}

struct HcPluginCacheIterator {
	wd: Box<dyn Iterator<Item = StdResult<DirEntry, walkdir::Error>>>,
}

impl HcPluginCacheIterator {
	fn new(path: &Path) -> Self {
		HcPluginCacheIterator {
			wd: Box::new(
				WalkDir::new(path) // builds a new iterator using WalkDir
					.min_depth(3) // makes sure to get the publisher, name, and version folders
					.max_depth(3)
					.into_iter()
					.filter_entry(|e| e.path().is_dir()), // makes sure the path is a directory
			),
		}
	}
	fn path_to_plugin_entry(&self, path: &Path) -> Result<PluginCacheEntry> {
		let components: Vec<String> = path
			.components()
			.filter_map(|component| {
				if let Component::Normal(name) = component {
					// extracts components separated by "/" from the path
					name.to_str().map(|s| s.to_string()) // converts the string slices into Strings
				} else {
					None
				}
			})
			.collect();
		if components.len() < 3 {
			return Err(hc_error!(
				"Error, the path to the plugin in cache has fewer than 3 parent folders"
			));
		}
		let relevant_components: &[String] = &components[components.len() - 3..];
		let plugin_publisher = relevant_components[0].to_owned();
		let plugin_name = relevant_components[1].to_owned();
		let plugin_version_string = relevant_components[2].as_str(); // variable for created for convenience.
		let plugin_version_result = Version::parse(plugin_version_string);
		let plugin_version = match plugin_version_result {
			Ok(version) => version,
			Err(e) => {
				return Err(hc_error!(
					"Error, invalid version for plugin. Version: {}, Publisher: {}, Name: {}",
					plugin_version_string,
					plugin_publisher,
					plugin_name
				))
			}
		};
		let last_modified: SystemTime = repo::get_last_modified_or_now(path);
		Ok(PluginCacheEntry {
			publisher: plugin_publisher,
			name: plugin_name,
			version: plugin_version,
			modified: last_modified,
		})
	}
}

/// Function will take the full file path and put the last 3 directories (publisher, plugin, and review)
/// into the Plugin Cache entry
impl Iterator for HcPluginCacheIterator {
	type Item = PluginCacheEntry;
	fn next(&mut self) -> Option<Self::Item> {
		if let Some(dir_entry_option) = self.wd.next() {
			let dir_entry = dir_entry_option.ok()?; // extracts direntry after converting Result to Option
			self.path_to_plugin_entry(dir_entry.path()).ok()
		} else {
			None
		}
	}
}

/// Plugins are stored with the following format `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
pub struct HcPluginCache {
	path: PathBuf, // path to the root of the plugin cache
	entries: Vec<PluginCacheEntry>,
}

impl HcPluginCache {
	pub fn new(path: &Path) -> Self {
		let plugins_path = pathbuf![path, "plugins"];
		let entries: Vec<PluginCacheEntry> =
			HcPluginCacheIterator::new(plugins_path.as_path()).collect();
		Self {
			path: plugins_path,
			entries,
		}
	}
	/// Initialization function intended to be used only when testing
	#[cfg(test)]
	pub fn new_for_test(path: &Path, entries: Vec<PluginCacheEntry>) -> Self {
		let plugins_path = pathbuf![path];
		let entries: Vec<PluginCacheEntry> = entries;
		Self {
			path: plugins_path,
			entries,
		}
	}

	/// The folder in which a specific PluginID will be stored
	///
	/// `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
	pub fn plugin_download_dir(&self, plugin_id: &PluginId) -> PathBuf {
		self.path
			.join(plugin_id.publisher().as_ref())
			.join(plugin_id.name().as_ref())
			.join(plugin_id.version().as_ref())
	}

	/// The path to where the `plugin.kdl` file for a specific PluginId will be stored
	///
	/// `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>/plugin.kdl`
	pub fn plugin_kdl(&self, plugin_id: &PluginId) -> PathBuf {
		self.plugin_download_dir(plugin_id).join("plugin.kdl")
	}
	/// Sort function is the same as in repo.cache but has been modified to get rid of the enum variant Largest
	fn sort<A: Borrow<PluginCacheEntry>>(entries: &mut [A], sort: PluginCacheSort, invert: bool) {
		// Generic allows sort to handle both owned and borrowed lists
		let sort_func: fn(&PluginCacheEntry, &PluginCacheEntry) -> std::cmp::Ordering =
			match (sort, invert) {
				(PluginCacheSort::Alpha, false) => |a, b| a.name.partial_cmp(&b.name).unwrap(),
				(PluginCacheSort::Alpha, true) => |a, b| b.name.partial_cmp(&a.name).unwrap(),
				(PluginCacheSort::OldestDate, false) => {
					|a, b| a.modified.partial_cmp(&b.modified).unwrap()
				}
				(PluginCacheSort::OldestDate, true) => {
					|a, b| b.modified.partial_cmp(&a.modified).unwrap()
				}
				// higher versions will appear first when using the Newest Version filter so the partial_cmp parameters must be reversed
				(PluginCacheSort::NewestVersion, false) => {
					|a, b| b.version.partial_cmp(&a.version).unwrap()
				}
				(PluginCacheSort::NewestVersion, true) => {
					|a, b| a.version.partial_cmp(&b.version).unwrap()
				}
			};

		entries.sort_by(|a1: &A, a2: &A| sort_func(a1.borrow(), a2.borrow()));
	}
	/// All functions up to list involve deleting entries
	pub fn delete(
		&mut self,
		scope: PluginCacheDeleteScope,
		filter: Option<String>,
		force: bool,
	) -> Result<()> {
		let partitioned_vectors =
			HcPluginCache::delete_inner(&mut self.entries, scope, filter, force);
		let to_del = partitioned_vectors.0;
		let to_keep = partitioned_vectors.1;
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
			self.entries.extend(to_keep);
			// self.entries contains `to_keep` plus any entries that were unsuccessfully
			// deleted
		}
		Ok(())
	}
	/// modified internal_delete function from repo.rs
	fn internal_delete(&mut self, entry: &PluginCacheEntry) -> Result<()> {
		std::fs::remove_dir_all(self.get_path_to_plugin(entry))?;
		Ok(())
	}
	/// helper function gets path for internal_delete
	fn get_path_to_plugin(&self, entry: &PluginCacheEntry) -> PathBuf {
		let plugin_id = PluginId::new(
			PluginPublisher(entry.publisher.to_string()),
			PluginName(entry.name.to_string()),
			PluginVersion(entry.version.to_string()),
		);
		let path_to_plugin = self.plugin_download_dir(&plugin_id);
		let full_path = pathbuf![&self.path, &path_to_plugin];
		full_path
	}
	/// created an inner function for ease of testing and to return the partitioned lists
	fn delete_inner(
		entries: &mut Vec<PluginCacheEntry>,
		scope: PluginCacheDeleteScope,
		filter: Option<String>,
		force: bool,
	) -> (Vec<PluginCacheEntry>, Vec<PluginCacheEntry>) {
		// Parse filter to regex if provided
		let opt_pat: Option<Regex> =
			filter.map(|raw_p| Regex::new(format!("^{raw_p}$").as_str()).unwrap());
		// similar function to repo.rs but reverses the to_del and to_keep for simplicity
		let (to_del, to_keep): (Vec<PluginCacheEntry>, Vec<PluginCacheEntry>) = match scope {
			PluginCacheDeleteScope::All => entries.drain(0..).partition(|e| match &opt_pat {
				Some(pat) => pat.is_match(e.name.as_str()),
				None => true,
			}),
			PluginCacheDeleteScope::Group { sort, invert, n } => {
				// First sort entries in-place in entries
				HcPluginCache::sort(entries, sort, invert);
				let mut hits = 0;
				// Now get the first N entries that pass filter
				entries.drain(0..).partition(|e| {
					let del = match &opt_pat {
						Some(pat) => pat.is_match(e.name.as_str()),
						None => true,
					};
					// passes filter and below max threshold, delete
					if del && hits < n {
						hits += 1;
						true
					// put in do_keep
					} else {
						false
					}
				})
			}
		};
		(to_del, to_keep)
	}

	/// lists all the plugin entries. Works the same as the function in repo.rs except for
	/// user can filter plugins by version and publisher
	pub fn list(
		mut self,
		scope: PluginCacheListScope,
		name: Option<String>,
		publisher: Option<String>,
		version: Option<VersionReq>,
	) -> Result<()> {
		// using borrowed data in list_inner in order to avoid transferring ownership
		let filtered_entries =
			HcPluginCache::list_inner(&self.entries, scope, name, publisher, version);
		match filtered_entries {
			Ok(v) => self.display(v),
			Err(e) => return Err(e),
		}
		Ok(())
	}
	fn list_inner(
		entries: &[PluginCacheEntry],
		scope: PluginCacheListScope,
		name: Option<String>,
		publisher: Option<String>,
		version: Option<VersionReq>,
	) -> Result<Vec<&PluginCacheEntry>> {
		let opt_pat: Option<Regex> = match name {
			// converts the string to a regex expression
			Some(raw_p) => Some(Regex::new(format!("^{raw_p}$").as_str())?),
			None => None,
		};
		// filters based on the regex pattern
		let mut filt_entries = entries
			.iter()
			.filter(|e| match &opt_pat {
				Some(pat) => pat.is_match(e.name.as_str()),
				None => true, // if there is no regex pattern passed in, the default is true
			})
			.filter(|e| match &publisher {
				Some(a) => e.publisher == *a, // must dereference the borrowed value of a to compare it to the plugin's publisher
				None => true,
			})
			.filter(|e| match &version {
				Some(a) => a.matches(&e.version),
				None => true,
			})
			.collect::<Vec<&PluginCacheEntry>>(); // creates borrowed data to pass to display

		// Sort filtered entries
		HcPluginCache::sort(&mut filt_entries, scope.sort, scope.invert);

		// Limit to first N if specified
		if let Some(n) = scope.n {
			filt_entries.truncate(n);
		}
		Ok(filt_entries)
	}

	/// Internal helper for list functions
	fn display(&self, to_show: Vec<&PluginCacheEntry>) {
		println!("{}", Table::new(to_show));
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	fn test_data() -> Vec<PluginCacheEntry> {
		let cache_entry_1 = PluginCacheEntry {
			publisher: String::from("randomhouse"),
			name: String::from("bugs bunny"),
			version: Version::parse("0.2.0").unwrap(),
			modified: SystemTime::UNIX_EPOCH + Duration::from_secs(3),
		};
		let cache_entry_2 = PluginCacheEntry {
			publisher: String::from("mitre"),
			name: String::from("affiliation"),
			version: Version::parse("0.1.0").unwrap(),
			modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
		};
		let cache_entry_3 = PluginCacheEntry {
			publisher: String::from("mitre2"),
			name: String::from("activity"),
			version: Version::parse("0.1.1").unwrap(),
			modified: SystemTime::UNIX_EPOCH + Duration::from_secs(2),
		};
		let cache_entry_4 = PluginCacheEntry {
			publisher: String::from("mitre"),
			name: String::from("difference"),
			version: Version::parse("0.0.5").unwrap(),
			modified: SystemTime::now(),
		};
		vec![cache_entry_1, cache_entry_2, cache_entry_3, cache_entry_4]
	}

	#[test]
	fn test_scope_works_as_expected() {
		let time_sort = PluginCacheListScope {
			sort: PluginCacheSort::OldestDate,
			invert: false,
			n: None,
		};
		let reverse_time_sort = PluginCacheListScope {
			sort: PluginCacheSort::OldestDate,
			invert: true,
			n: None,
		};
		let reverse_time_sort_with_two = PluginCacheListScope {
			sort: PluginCacheSort::OldestDate,
			invert: true,
			n: Some(2),
		};
		let alpha_sort = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};
		let reverse_alpha_sort = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: true,
			n: None,
		};
		let version_sort = PluginCacheListScope {
			sort: PluginCacheSort::NewestVersion,
			invert: false,
			n: None,
		};
		let reverse_version_sort = PluginCacheListScope {
			sort: PluginCacheSort::NewestVersion,
			invert: true,
			n: None,
		};
		// Time sort
		let entries = &test_data();
		let results = HcPluginCache::list_inner(entries, time_sort, None, None, None).unwrap();
		let expected_output = vec!["affiliation", "activity", "bugs bunny", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		// Reverse time sort
		let results =
			HcPluginCache::list_inner(entries, reverse_time_sort, None, None, None).unwrap();
		let expected_output = vec!["difference", "bugs bunny", "activity", "affiliation"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		// Alphabetical sort
		let results = HcPluginCache::list_inner(entries, alpha_sort, None, None, None).unwrap();
		let expected_output = vec!["activity", "affiliation", "bugs bunny", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		// Reverse alphabetical sort
		let results =
			HcPluginCache::list_inner(entries, reverse_alpha_sort, None, None, None).unwrap();
		let expected_output = vec!["difference", "bugs bunny", "affiliation", "activity"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		// Reverse time sort with only two results
		let results =
			HcPluginCache::list_inner(entries, reverse_time_sort_with_two, None, None, None)
				.unwrap();
		let expected_output = vec!["difference", "bugs bunny"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		// Version sort (newest to oldest)
		let results = HcPluginCache::list_inner(entries, version_sort, None, None, None).unwrap();
		let expected_output = vec!["bugs bunny", "activity", "affiliation", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		// Reverse version sort
		let results =
			HcPluginCache::list_inner(entries, reverse_version_sort, None, None, None).unwrap();
		let expected_output: Vec<&str> =
			vec!["difference", "affiliation", "activity", "bugs bunny"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);
	}

	#[test]
	fn test_version_filtering() {
		let alpha_sort = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};

		let entries = &test_data();
		let results = HcPluginCache::list_inner(
			entries,
			alpha_sort,
			None,
			None,
			Some(VersionReq::parse(">0.0.5").unwrap()),
		)
		.unwrap();
		let expected_output = vec!["activity", "affiliation", "bugs bunny"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		let alpha_sort_2 = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};
		let results = HcPluginCache::list_inner(
			entries,
			alpha_sort_2,
			None,
			None,
			Some(VersionReq::parse("<=0.1.0").unwrap()),
		)
		.unwrap();
		let expected_output = vec!["affiliation", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		let alpha_sort_3 = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};
		let results = HcPluginCache::list_inner(
			entries,
			alpha_sort_3,
			None,
			None,
			Some(VersionReq::parse("<=0.2.0").unwrap()),
		)
		.unwrap();
		let expected_output = vec!["activity", "affiliation", "bugs bunny", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);
	}
	#[test]
	fn test_name_filtering() {
		let alpha_sort = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};
		let entries = &test_data();
		let results = HcPluginCache::list_inner(
			entries,
			alpha_sort,
			Some(String::from("bugs bunny")),
			None,
			None,
		)
		.unwrap();
		let expected_output = vec!["bugs bunny"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		let alpha_sort = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};
		let results =
			HcPluginCache::list_inner(entries, alpha_sort, Some(String::from("a.*")), None, None)
				.unwrap();
		let expected_output = vec!["activity", "affiliation"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);
	}
	#[test]
	fn test_publisher_filtering() {
		let alpha_sort = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};
		let entries = &test_data();
		let results =
			HcPluginCache::list_inner(entries, alpha_sort, None, Some(String::from("mitre")), None)
				.unwrap();
		let expected_output = vec!["affiliation", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		let alpha_sort = PluginCacheListScope {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: None,
		};
		let entries = &test_data();
		let results =
			HcPluginCache::list_inner(entries, alpha_sort, None, Some(String::from("")), None)
				.unwrap();
		let expected_output: Vec<&String> = vec![];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);
	}
	/// Tests for inner delete logic
	#[test]
	fn test_delete_scope_all() {
		let delete_all = PluginCacheDeleteScope::All;
		let entries = &mut test_data();
		let mut expected_keep = ["affiliation", "bugs bunny", "difference"];
		let mut expected_delete = ["activity"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_all, Some(String::from("activity")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		actual_delete.sort(); // sorting for ease of testing
		actual_keep.sort();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);
	}
	#[test]
	fn test_delete_sort_by_alphabetical() {
		let delete_alpha = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: 2,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["bugs bunny", "difference"];
		let mut expected_delete = ["activity", "affiliation"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_alpha, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_keep, expected_keep);

		// test with n = 1
		let delete_alpha = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: 1,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["affiliation", "bugs bunny", "difference"];
		let mut expected_delete = ["activity"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_alpha, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);

		// test with reverse
		let delete_alpha = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::Alpha,
			invert: true,
			n: 2,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["difference", "bugs bunny"];
		let mut expected_delete = ["affiliation", "activity"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_alpha, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);
	}
	#[test]
	fn test_delete_sort_by_date() {
		let delete_date = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::OldestDate,
			invert: false,
			n: 2,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["bugs bunny", "difference"];
		let mut expected_delete = ["affiliation", "activity"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_date, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_keep, expected_keep);

		// test with n = 1
		let delete_date = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::OldestDate,
			invert: false,
			n: 1,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["activity", "bugs bunny", "difference"];
		let mut expected_delete = ["affiliation"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_date, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);

		// test with reverse
		let delete_date = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::OldestDate,
			invert: true,
			n: 2,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["difference", "bugs bunny"];
		let mut expected_delete = ["activity", "affiliation"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_date, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);
	}
	#[test]
	fn test_delete_sort_by_version() {
		let delete_version = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::NewestVersion,
			invert: false,
			n: 2,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["bugs bunny", "difference"];
		let mut expected_delete = ["activity", "affiliation"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_version, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_keep, expected_keep);

		// test with n = 1
		let delete_version = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::NewestVersion,
			invert: false,
			n: 1,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["bugs bunny", "affiliation", "difference"];
		let mut expected_delete = ["activity"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_version, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);

		// test with reverse
		let delete_version = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::NewestVersion,
			invert: true,
			n: 2,
		};
		let entries = &mut test_data();
		let mut expected_keep = ["difference", "bugs bunny"];
		let mut expected_delete = ["affiliation", "activity"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_version, Some(String::from("a.*")), false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);
	}
	/// tests for when you provide no filter
	#[test]
	fn test_delete_with_no_filter() {
		let delete_all = PluginCacheDeleteScope::All;
		let entries = &mut test_data();
		let mut expected_keep: Vec<&str> = Vec::new();
		let mut expected_delete = ["activity", "affiliation", "bugs bunny", "difference"];
		let delete_function = HcPluginCache::delete_inner(entries, delete_all, None, false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		actual_delete.sort();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);

		/// Running the no_filter test where the scope is Some. Ensures that nothing should be deleted regardless of the scope.
		// Version sort
		let delete_some_version = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::NewestVersion,
			invert: false,
			n: 4,
		};
		let entries = &mut test_data();
		let mut expected_keep: Vec<&str> = Vec::new();
		let mut expected_delete = vec!["bugs bunny", "activity", "affiliation", "difference"];
		let delete_function =
			HcPluginCache::delete_inner(entries, delete_some_version, None, false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		actual_keep.sort();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);

		// Date sort
		let delete_some_date = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::OldestDate,
			invert: false,
			n: 4,
		};
		let entries = &mut test_data();
		let mut expected_keep: Vec<&str> = Vec::new();
		let mut expected_delete = vec!["affiliation", "activity", "bugs bunny", "difference"];
		let delete_function = HcPluginCache::delete_inner(entries, delete_some_date, None, false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);

		// Alphabetical sort
		let delete_some_alpha = PluginCacheDeleteScope::Group {
			sort: PluginCacheSort::Alpha,
			invert: false,
			n: 4,
		};
		let entries = &mut test_data();
		let mut expected_keep: Vec<&str> = Vec::new();
		let mut expected_delete = ["activity", "affiliation", "bugs bunny", "difference"];
		let delete_function = HcPluginCache::delete_inner(entries, delete_some_alpha, None, false);
		let mut actual_delete: Vec<&String> = delete_function.0.iter().map(|x| &x.name).collect();
		let mut actual_keep: Vec<&String> = delete_function.1.iter().map(|x| &x.name).collect();
		assert_eq!(actual_delete, expected_delete);
		assert_eq!(actual_keep, expected_keep);
	}
	/// tests that the path given to delete_internal() is the correct one.
	#[test]
	fn test_get_correct_path_to_deletion() {
		let path = std::env::current_dir().unwrap();
		let entries = test_data();
		let mut hc_plugin_cache = HcPluginCache::new_for_test(path.as_path(), entries);
		let entry = &test_data()[0];
		let actual_path = hc_plugin_cache.get_path_to_plugin(entry);
		let mut expected_path = hc_plugin_cache.path;
		let expected_path = pathbuf![
			&expected_path,
			&entry.publisher,
			&entry.name,
			&entry.version.to_string()
		];
		assert_eq!(actual_path, expected_path);

		// Test for incorrect case, wrong entry being passed to get_actual_path
		let path = std::env::current_dir().unwrap();
		let entries = test_data();
		let mut hc_plugin_cache = HcPluginCache::new_for_test(path.as_path(), entries);
		let correct_entry = &test_data()[0];
		let incorrect_entry = &test_data()[1];
		let actual_path = hc_plugin_cache.get_path_to_plugin(incorrect_entry);
		let mut expected_path = hc_plugin_cache.path;
		let expected_path = pathbuf![
			&expected_path,
			&correct_entry.publisher,
			&correct_entry.name,
			&correct_entry.version.to_string()
		];
		assert_ne!(actual_path, expected_path);
	}
}
