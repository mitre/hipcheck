// SPDX-License-Identifier: Apache-2.0
#![allow(unused)]
use crate::{cache::repo, error::Result, plugin::PluginId, StdResult};
use pathbuf::pathbuf;
use std::{
	borrow::Borrow,
	env,
	path::{Component, Path, PathBuf},
	time::{Duration, SystemTime},
};
use tabled::{Table, Tabled};
use walkdir::{DirEntry, WalkDir};
//use super::super::cli::CliConfig;
use regex::Regex; //used for regex
use semver::{Version, VersionReq};

///enums used for sorting are the same as the ones used for listing repo cache entries except for
/// does not include Largest
#[derive(Debug, Clone)]
pub enum PluginCacheSort {
	OldestDate, //same as "Oldest" in repo.rs
	Alpha,
	NewestVersion, //sorts the versions of the plugins from newest to oldest unless inverted
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
	pub version: String,
	#[tabled(display_with("Self::display_modified", self), rename = "last_modified")]
	pub modified: SystemTime,
}

impl PluginCacheEntry {
	//copied the function from repo for simplicity
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
				WalkDir::new(path) //builds a new iterator using WalkDir
					.min_depth(3) //makes sure to get the publisher, name, and version folders
					.max_depth(3)
					.into_iter()
					.filter_entry(|e| e.path().is_dir()), //makes sure the path is a directory
			),
		}
	}
	fn path_to_plugin_entry(&self, path: &Path) -> Result<PluginCacheEntry> {
		let components: Vec<String> = path
			.components()
			.filter_map(|component| {
				if let Component::Normal(name) = component {
					//extracts components separated by "/" from the path
					name.to_str().map(|s| s.to_string()) //converts the string slices into Strings
				} else {
					None
				}
			})
			.collect(); //collects filepath into vector
			   //Todo: add error handling for when the path length is less than 3
		let relevant_components: &[String] = &components[components.len() - 3..];
		let plugin_publisher = relevant_components[0].to_owned();
		let plugin_name = relevant_components[1].to_owned();
		let plugin_version = relevant_components[2].to_owned();
		let last_modified: SystemTime = repo::get_last_modified_or_now(path);
		Ok(PluginCacheEntry {
			publisher: plugin_publisher,
			name: plugin_name,
			version: plugin_version,
			modified: last_modified,
		})
	}
}

///Function will take the full file path and put the last 3 directories (publisher, plugin, and review)
///into the Plugin Cache entry
impl Iterator for HcPluginCacheIterator {
	type Item = PluginCacheEntry;
	fn next(&mut self) -> Option<Self::Item> {
		if let Some(dir_entry_option) = self.wd.next() {
			let dir_entry = dir_entry_option.ok()?; //extracts direntry after converting Result to Option
			self.path_to_plugin_entry(dir_entry.path()).ok()
		} else {
			None
		}
	}
}

/// Plugins are stored with the following format `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
pub struct HcPluginCache {
	path: PathBuf, //path to the root of the plugin cache
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
	///Sort function is the same as in repo.cache but has been modified to get rid of the enum variant Largest
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
				//calls a helper function that will parse strings into versions before comparing
				(PluginCacheSort::NewestVersion, false) => {
					|a, b| Self::compare_versions(a, b, false)
				}
				(PluginCacheSort::NewestVersion, true) => |a, b| Self::compare_versions(a, b, true),
			};

		entries.sort_by(|a1: &A, a2: &A| sort_func(a1.borrow(), a2.borrow()));
	}
	fn compare_versions(
		a: &PluginCacheEntry,
		b: &PluginCacheEntry,
		invert: bool,
	) -> std::cmp::Ordering {
		let a_version = Version::parse(a.version.as_str()).unwrap();
		let b_version = Version::parse(b.version.as_str()).unwrap();
		if !invert {
			b_version.partial_cmp(&a_version).unwrap() //need to reverse the parameters to order the versions from newest to oldest
		} else {
			a_version.partial_cmp(&b_version).unwrap()
		}
	}

	///lists all the plugin entries. Works the same as the function in repo.rs except for
	///user can filter plugins by version and publisher
	pub fn list(
		mut self,
		scope: PluginCacheListScope,
		name: Option<String>,
		publisher: Option<String>,
		version: Option<String>,
	) -> Result<()> {
		//using borrowed data in list_inner in order to avoid transferring ownership
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
		version: Option<String>,
	) -> Result<Vec<&PluginCacheEntry>> {
		let opt_pat: Option<Regex> = match name {
			//converts the string to a regex expression
			Some(raw_p) => Some(Regex::new(format!("^{raw_p}$").as_str())?),
			None => None,
		};
		//filters based on the regex pattern
		let mut filt_entries = entries
			.iter()
			.filter(|e| match &opt_pat {
				Some(pat) => pat.is_match(e.name.as_str()),
				None => true, //if there is no regex pattern passed in, the default is true
			})
			.filter(|e| match &publisher {
				Some(a) => e.publisher == *a, //must dereference the borrowed value of a to compare it to the plugin's publisher
				None => true,
			})
			.filter(|e| match &version {
				Some(a) => {
					let version_to_compare = VersionReq::parse(a.as_str())
						.expect("String cannot be converted to Version Requirement");
					let plugin_version = Version::parse(e.version.as_str())
						.expect("String cannot be converted to version");
					version_to_compare.matches(&plugin_version)
				}
				None => true,
			})
			.collect::<Vec<&PluginCacheEntry>>(); //creates borrowed data to pass to display

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
	use base64::display;

	use super::*;
	fn test_data() -> Vec<PluginCacheEntry> {
		let cache_entry_1 = PluginCacheEntry {
			publisher: String::from("randomhouse"),
			name: String::from("bugs bunny"),
			version: String::from("0.2.0"),
			modified: SystemTime::UNIX_EPOCH + Duration::from_secs(3),
		};
		let cache_entry_2 = PluginCacheEntry {
			publisher: String::from("mitre"),
			name: String::from("affiliation"),
			version: String::from("0.1.0"),
			modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
		};
		let cache_entry_3 = PluginCacheEntry {
			publisher: String::from("mitre2"),
			name: String::from("activity"),
			version: String::from("0.1.1"),
			modified: SystemTime::UNIX_EPOCH + Duration::from_secs(2),
		};
		let cache_entry_4 = PluginCacheEntry {
			publisher: String::from("mitre"),
			name: String::from("difference"),
			version: String::from("0.0.5"),
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
		//Time sort
		let entries = &test_data();
		let results = HcPluginCache::list_inner(entries, time_sort, None, None, None).unwrap();
		let expected_output = vec!["affiliation", "activity", "bugs bunny", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		//Reverse time sort
		let results =
			HcPluginCache::list_inner(entries, reverse_time_sort, None, None, None).unwrap();
		let expected_output = vec!["difference", "bugs bunny", "activity", "affiliation"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		//Alphabetical sort
		let results = HcPluginCache::list_inner(entries, alpha_sort, None, None, None).unwrap();
		let expected_output = vec!["activity", "affiliation", "bugs bunny", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		//Reverse alphabetical sort
		let results =
			HcPluginCache::list_inner(entries, reverse_alpha_sort, None, None, None).unwrap();
		let expected_output = vec!["difference", "bugs bunny", "affiliation", "activity"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		//Reverse time sort with only two results
		let results =
			HcPluginCache::list_inner(entries, reverse_time_sort_with_two, None, None, None)
				.unwrap();
		let expected_output = vec!["difference", "bugs bunny"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		//Version sort (newest to oldest)
		let results = HcPluginCache::list_inner(entries, version_sort, None, None, None).unwrap();
		let expected_output = vec!["bugs bunny", "activity", "affiliation", "difference"];
		let mut actual_output: Vec<&String> = results.iter().map(|x| &x.name).collect();
		assert_eq!(actual_output, expected_output);

		//Reverse version sort
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
			Some(String::from(">0.0.5")),
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
			Some(String::from("<=0.1.0")),
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
			Some(String::from("<=0.2.0")),
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
}
