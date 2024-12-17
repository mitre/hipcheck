// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf, Component};
use std::{io::Error,
		  borrow::Borrow,
		  time::SystemTime, 
};
use pathbuf::pathbuf;
use tabled::{Table, Tabled};
use walkdir::{DirEntry, WalkDir};
use crate::plugin::PluginId;
use crate::StdResult;
//use super::super::cli::CliConfig;
use crate::error::Result;
use regex::Regex; //used for regex
use semver::{Version, VersionReq};
use crate::cache::repo;

///enums used for sorting are the same as the ones used for listing repo cache entries except for
/// does not include Largest
#[derive(Debug, Clone)]
pub enum PluginCacheSort {  
	Oldest,
	Alpha, 
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
	pub modified: SystemTime
}

impl PluginCacheEntry { //copied the function from repo for simplicity
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
	wd: Box<dyn Iterator<Item = StdResult<DirEntry, walkdir::Error>>>
}

impl HcPluginCacheIterator { 
	fn new(path: &Path) -> Self {
		HcPluginCacheIterator{
			wd: Box::new(
				WalkDir::new(path) //builds a new iterator using WalkDir
					.min_depth(3) //makes sure to get the publisher, name, and version folders
					.into_iter()
					.filter_entry(|e| e.path().is_dir()), //makes sure the path is a directory
			)
		}	
	}
	fn path_to_plugin_entry(&self, path: &Path) -> Result<PluginCacheEntry> { //does function need to work when you clone a repo?
	   let components: Vec<String> = path
        .components()
        .filter_map(|component| {
            if let Component::Normal(name) = component { //extracts components seperated by "/" from the path
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
		Ok(PluginCacheEntry{
							publisher:plugin_publisher, 
							name:plugin_name, 
							version: plugin_version,
							modified: last_modified
							}) 
		
	}
	
}



///Function will take the full file path and put the last 3 directories (publisher, plugin, and review)
///into the Plugin Cache entry
impl Iterator for HcPluginCacheIterator {
	type Item = PluginCacheEntry;
	fn next(&mut self) -> Option<Self::Item> { 
		if let Some(Ok(e)) = self.wd.next() {
			if let Ok(ce) = self.path_to_plugin_entry(e.path()) {
				return Some(ce);
			} 
			else {
				return None;
			}
		}
		else {
			return None;
		}
	}
}



/// Plugins are stored with the following format `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
pub struct HcPluginCache {
	pub path: PathBuf, //path to the root of the plugin cache
	pub entries: Vec<PluginCacheEntry>, //needs to store a vector of PluginEntries

}

impl HcPluginCache {
	pub fn new(path: &Path) -> Self {
		let plugins_path = pathbuf![path, "plugins"];
		let entries: Vec<PluginCacheEntry> =
			HcPluginCacheIterator::new(plugins_path.as_path()).collect();
		Self { path: plugins_path , entries: entries}
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
				(PluginCacheSort::Oldest, false) => {
					|a, b| a.modified.partial_cmp(&b.modified).unwrap()
				}
				(PluginCacheSort::Oldest, true) => {
					|a, b| b.modified.partial_cmp(&a.modified).unwrap()
				}
			};
			entries.sort_by(|a1: &A, a2: &A| sort_func(a1.borrow(), a2.borrow()));
		}

	///lists all the plugin entries. Works the same as the function in repo.rs except for 
	///user can filter plugins by version and publisher
	pub fn list(&mut self, scope: PluginCacheListScope, name: Option<String>, publisher:Option<String>, version: Option<String>) -> Result<()> { //version is a str to make parsing easier
		let opt_pat: Option<Regex> = match name {  //converts the string to a regex expression
			Some(raw_p) => Some(Regex::new(format!("^{raw_p}$").as_str())?),
			None => None,
		};
		//filters based on the regex pattern
		let mut filt_entries = self
			.entries
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
								 let version_to_compare = VersionReq::parse(a.as_str()).unwrap();
								 let plugin_version = Version::parse(e.version.as_str()).unwrap();
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
		self.display(filt_entries);
		
		Ok(())
	}
	/// Internal helper for list functions
	fn display(&self, to_show: Vec<&PluginCacheEntry>) {
		println!("{}", Table::new(to_show));
	}

	///Helper function for filter_version that converts the version from the plugin cache
	///entry and the version passed in to tuples of three integers
	fn filter_version(&self, version:&String) -> Result<(u32, u32, u32)> {
		let version_as_vec: Vec<&str> = version.split('.').collect();
    	if version_as_vec.len() != 3 {
        	panic!("Version string does not have exactly three parts");
    	}
		let first_part = version_as_vec[0].parse::<u32>()?;
    	let second_part = version_as_vec[1].parse::<u32>()?;
    	let third_part = version_as_vec[2].parse::<u32>()?;
		Ok((first_part, second_part, third_part))
	}
}


//can write a test module with a function that instantiates a plugin cache, don't commit tests
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn test_path_to_plugin_entry() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let hc_plugin_struct = HcPluginCacheIterator::new(path);
		//let plugin_entry = hc_plugin_struct.path_to_plugin_entry(path);
	}
	#[test]
	fn test_next_function() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let mut hc_plugin_struct = HcPluginCacheIterator::new(path);
		let mut plugin_num:i32 = 0;
		while let Some(ce) = hc_plugin_struct.next() {
			println!("{:?}", ce);
			plugin_num += 1;
			println!("Plugin number: {}", plugin_num);
		} 
		assert_eq!(plugin_num, 11)
	}
	#[test]
	/*fn test_list_function_with_no_filters() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let mut hc_plugin_struct = HcPluginCache::new(path);
		let no_filters = hc_plugin_struct.list(None,None);
	}
	#[test]
	fn test_list_function_with_regex_filters() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("Function with git filter");
		let git_filters = hc_plugin_struct.list(Some(String::from("git.*")), None);
		println!("Function with activity filter");
		let activity_filters = hc_plugin_struct.list(Some(String::from("activity")), None);
		println!("Function with exclusive filter");
		let exclusive_filters = hc_plugin_struct.list(Some(String::from("activity3")), None);
	}*/
	/*#[test]
	fn test_list_function_with_version_filters() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("Versions above 0.0.6");
		hc_plugin_struct.list(None, None, Some(String::from(">0.0.6")));
		println!("Versions below 0.0.6");
		hc_plugin_struct.list(None, None, Some(String::from("<0.0.6")));
		println!("Versions above 0.0.6 and less than or equal to 0.1.0");
		hc_plugin_struct.list(None, None, Some(String::from(">0.0.6, <=0.1.0")));
		println!("Invalid version");
		hc_plugin_struct.list(None, None, Some(String::from("0.9")));
	}*/	
	#[test]
	fn test_list_function_with_plugin_cache_list() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let alpha_sort = PluginCacheListScope {
													sort:PluginCacheSort::Alpha, 
													invert:false, 
													n:None,
		};
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("Ordered alphabetically");
		hc_plugin_struct.list(alpha_sort, None, None, None);
		
		let alpha_sort_first_five = PluginCacheListScope {
			sort:PluginCacheSort::Alpha, 
			invert:false, 
			n:Some(5),
		};
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("Ordered alphabetically with first 5");
		hc_plugin_struct.list(alpha_sort_first_five, None, None, None);

		let reverse_alpha_sort = PluginCacheListScope {
			sort:PluginCacheSort::Alpha, 
			invert:true, 
			n:None,
		};
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("Reverse alphabetical");
		hc_plugin_struct.list(reverse_alpha_sort, None, None, None);

		let time_sort = PluginCacheListScope {
			sort:PluginCacheSort::Oldest, 
			invert:false, 
			n:None,
		};
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("time modified");
		hc_plugin_struct.list(time_sort, None, None, None);

		let reverse_time_sort = PluginCacheListScope {
			sort:PluginCacheSort::Oldest, 
			invert:true, 
			n:None,
		};
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("Reverse time modified");
		hc_plugin_struct.list(reverse_time_sort, None, None, None);


	}
	/*#[test]
	fn test_list_function_with_publisher_filters() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let mut hc_plugin_struct = HcPluginCache::new(path);
		println!("Publisher Mitre");
		let all_plugins_included = hc_plugin_struct.list(None, Some(String::from("mitre")), None);
		println!("Publisher Billybob");
		let no_plugins_included = hc_plugin_struct.list(None, Some(String::from("billybob")), None);
	}	
	#[test]
	fn test_list_function_with_all_filters() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let mut hc_plugin_struct = HcPluginCache::new(path);
		let activity_plugin_show = hc_plugin_struct.list(Some(String::from("activity")),None,Some(String::from("0.2.0")));
		let git_plugin_show = hc_plugin_struct.list(Some(String::from("git.*")), None, Some(String::from("0.2.0")));
		let git_plugin_no_show = hc_plugin_struct.list(Some(String::from("git.*")),Some(String::from("miter")), Some(String::from("0.1.0")));
		let all_plugin_show = hc_plugin_struct.list(None,Some(String::from("mitre")),None);
	}*/

	/*#[test]
	fn test_filter_version() {
		let path = Path::new(r"C:\Users\ninaagrawal\AppData\Local\hipcheck");
		let hc_plugin_struct = HcPluginCache::new(path);
		let str_1 = String::from("0.1.2");
		let str_2 = String::from("0.1.0");
		let result = hc_plugin_struct.filter_version(&str_1, &str_2);
		//assert!(result);
		let str_3 = String::from("0.1.0");
		let result_two = hc_plugin_struct.filter_version(&str_2, &str_3);
		//assert!(result_two);
		//let str_4 = String::from("0.1.1");
		//let result_three = hc_plugin_struct.filter_version(&str_3, &str_4);
		//assert!(!result_three);
		
	}*/
		
}




	
