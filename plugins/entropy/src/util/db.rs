// SPDX-License-Identifier: Apache-2.0

use crate::linguist::SourceFileDetector;

use std::sync::Arc;

#[salsa::query_group(LinguistStorage)]
pub trait LinguistSource: salsa::Database {
	/// Returns the code detector for the current session
	#[salsa::input]
	fn source_file_detector(&self) -> Arc<SourceFileDetector>;

	/// Returns likely source file assessment for `file_name`
	fn is_likely_source_file(&self, file_name: String) -> bool;
}

fn is_likely_source_file(db: &dyn LinguistSource, file_name: String) -> bool {
	db.source_file_detector().is_likely_source_file(file_name)
}

#[derive(Default)]
#[salsa::database(LinguistStorage)]
pub struct Linguist {
	storage: salsa::Storage<Self>,
}
impl Linguist {
	pub fn new() -> Self {
		Linguist {
			storage: Default::default(),
		}
	}
}

impl salsa::Database for Linguist {}
