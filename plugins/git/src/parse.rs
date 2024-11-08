// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use crate::data::{Contributor, Diff, FileDiff, RawCommit};
use anyhow::{Context as _, Error, Result};
use jiff::Timestamp;
use nom::{
	branch::alt,
	bytes::complete::tag,
	character::complete::{char as character, digit1, newline, not_line_ending, one_of, space1},
	combinator::{map, opt, peek, recognize},
	error::{Error as NomError, ErrorKind},
	multi::{fold_many0, many0, many1, many_m_n},
	sequence::{preceded, terminated, tuple},
	IResult,
};
use std::{iter::Iterator, result::Result as StdResult, sync::Arc};

const HEX_CHARS: &str = "0123456789abcdef";
const GIT_HASH_MIN_LEN: usize = 5;
const GIT_HASH_MAX_LEN: usize = 40;

/// Parse a complete git log.
pub fn git_log(input: &str) -> Result<Vec<RawCommit>> {
	let (_, commits) = commits(input)
		.map_err(|e| Error::msg(e.to_string()))
		.context("can't parse git log")?;
	log::trace!("parsed git commits [commits='{:#?}']", commits);
	Ok(commits)
}

/// Parse a complete set of git diffs.
pub fn git_diff(input: &str) -> Result<Vec<Diff>> {
	let (_, diffs) = diffs(input)
		.map_err(|e| Error::msg(e.to_string()))
		.context("can't parse git diff")?;
	log::trace!("parsed git diffs [diffs='{:#?}']", diffs);
	Ok(diffs)
}

/// Parse a complete set of GitHub diffs.
pub fn github_diff(input: &str) -> Result<Vec<Diff>> {
	let (_, diffs) = gh_diffs(input)
		.map_err(|e| Error::msg(e.to_string()))
		.context("can't parse GitHub diff")?;
	log::trace!("parsed GitHub diffs [diffs='{:#?}']", diffs);
	Ok(diffs)
}

fn hash(input: &str) -> IResult<&str, &str> {
	recognize(many_m_n(GIT_HASH_MIN_LEN, GIT_HASH_MAX_LEN, hex_char))(input)
}

fn hex_char(input: &str) -> IResult<&str, char> {
	one_of(HEX_CHARS)(input)
}

fn date(input: &str) -> StdResult<String, String> {
	let ts: StdResult<Timestamp, String> = input.parse().map_err(|e| {
		format!(
			"Could not parse git commit timestamp as RFC3339: '{}'\
			\nCaused by: {}",
			input, e
		)
	});
	ts.map(|t| t.to_string())
}

fn commit(input: &str) -> IResult<&str, RawCommit> {
	let (input, hash_str) = line(input)?;
	let (input, author_name) = line(input)?;
	let (input, author_email) = line(input)?;
	let (input, written_on_str) = line(input)?;
	let (input, committer_name) = line(input)?;
	let (input, committer_email) = line(input)?;
	let (input, committed_on_str) = line(input)?;
	// At one point our `git log` invocation was configured
	// to return GPG key info, but that was leading to errors
	// with format and GPG key validation, so we removed it
	// from the print specifier

	// There is always an empty line here; ignore it
	let (input, _empty_line) = line(input)?;

	let (_, hash) = hash(hash_str).map_err(|e| {
		log::error!("failed to parse git commit hash [err='{}']", e);
		e
	})?;

	let written_on = date(written_on_str);
	let committed_on = date(committed_on_str);

	let short_hash = &hash[..8];

	if let Err(e) = &written_on {
		log::error!(
			"git commit has invalid written_on timestamp [commit={}, error=\"{}\"]",
			short_hash,
			e
		);
	}

	if let Err(e) = &committed_on {
		log::error!(
			"git commit has invalid committed_on timestamp [commit={}, error=\"{}\"]",
			short_hash,
			e
		);
	}

	let commit = RawCommit {
		hash: hash.to_owned(),
		author: Contributor {
			name: author_name.to_owned(),
			email: author_email.to_owned(),
		},
		written_on,
		committer: Contributor {
			name: committer_name.to_owned(),
			email: committer_email.to_owned(),
		},
		committed_on,
	};

	Ok((input, commit))
}

fn commits(input: &str) -> IResult<&str, Vec<RawCommit>> {
	many0(commit)(input)
}

fn line_ending(input: &str) -> IResult<&str, &str> {
	recognize(alt((
		recognize(character('\n')),
		recognize(tuple((character('\r'), character('\n')))),
	)))(input)
}

fn line(input: &str) -> IResult<&str, &str> {
	terminated(not_line_ending, line_ending)(input)
}

fn num(input: &str) -> IResult<&str, i64> {
	digit1(input).map(|(input, output)| {
		// Unwrap here is fine because we know it's only going to be
		// a bunch of digits. Overflow is possible but we're choosing
		// not to worry about it for now, because if a commit is large
		// enough that the number of lines added or deleted
		// in a single file overflows an i64 we have bigger problems.
		(input, output.parse().unwrap())
	})
}

fn num_or_dash(input: &str) -> IResult<&str, Option<i64>> {
	let some_num = map(num, Some);
	let dash = map(character('-'), |_| None);
	alt((some_num, dash))(input)
}

fn stat(input: &str) -> IResult<&str, Option<Stat<'_>>> {
	tuple((num_or_dash, space1, num_or_dash, space1, line))(input).map(
		|(i, (lines_added, _, lines_deleted, _, file_name))| {
			let Some(lines_added) = lines_added else {
				return (i, None);
			};

			let Some(lines_deleted) = lines_deleted else {
				return (i, None);
			};

			let stat = Stat {
				lines_added,
				lines_deleted,
				file_name,
			};

			(i, Some(stat))
		},
	)
}

pub(crate) fn stats(input: &str) -> IResult<&str, Vec<Stat<'_>>> {
	map(many0(stat), |vec| {
		vec.into_iter().flatten().collect::<Vec<_>>()
	})(input)
}

pub(crate) fn opt_rest_diff_header(input: &str) -> IResult<&str, Diff> {
	opt(tuple((newline, diff)))(input).map(|(i, x)| {
		if let Some((_, d)) = x {
			(i, d)
		} else {
			(
				i,
				Diff {
					additions: None,
					deletions: None,
					file_diffs: vec![],
				},
			)
		}
	})
}

// Some empty commits have no output in the corresponding `git log` command, so we had to add a
// special header to be able to parse and recognize empty diffs and thus make the number of diffs
// and commits equal
pub(crate) fn diff_header(input: &str) -> IResult<&str, Diff> {
	tuple((tag("~~~\n"), opt_rest_diff_header))(input).map(|(i, (_, diff))| (i, diff))
}

pub(crate) fn diff(input: &str) -> IResult<&str, Diff> {
	log::trace!("input is {:#?}", input);
	tuple((stats, line, patches))(input).map(|(i, (stats, _, patches))| {
		log::trace!("patches are {:#?}", patches);
		let mut additions = Some(0);
		let mut deletions = Some(0);

		let file_diffs = Iterator::zip(stats.into_iter(), patches)
			.map(|(stat, patch)| {
				log::trace!(
					"stat is {:#?} added and {:#?} deleted",
					stat.lines_added,
					stat.lines_deleted
				);
				additions = additions.map(|a| a + stat.lines_added);
				deletions = deletions.map(|d| d + stat.lines_deleted);

				FileDiff {
					file_name: Arc::new(stat.file_name.to_owned()),
					additions: Some(stat.lines_added),
					deletions: Some(stat.lines_deleted),
					patch,
				}
			})
			.collect::<Vec<_>>();

		let diff = Diff {
			additions,
			deletions,
			file_diffs,
		};

		(i, diff)
	})
}

fn gh_diff(input: &str) -> IResult<&str, Diff> {
	// Handle reaching the end of the diff text without causing map0 to error
	if input.is_empty() {
		return Err(nom::Err::Error(NomError::new(input, ErrorKind::Many0)));
	}

	patches_with_context(input).map(|(i, patches)| {
		log::trace!("patches are {:#?}", patches);

		// GitHub diffs don't provide these.
		let additions = None;
		let deletions = None;

		let file_diffs = patches
			.into_iter()
			.map(|patch| FileDiff {
				file_name: Arc::new(patch.file_name),
				additions: None,
				deletions: None,
				patch: patch.content,
			})
			.collect();
		log::trace!("file_diffs are {:#?}", file_diffs);

		let diff = Diff {
			additions,
			deletions,
			file_diffs,
		};
		log::trace!("diff is {:#?}", diff);

		(i, diff)
	})
}

pub(crate) fn diffs(input: &str) -> IResult<&str, Vec<Diff>> {
	many0(diff_header)(input)
}

fn gh_diffs(input: &str) -> IResult<&str, Vec<Diff>> {
	log::trace!("input is {}", input);
	many0(gh_diff)(input)
}

fn meta(input: &str) -> IResult<&str, &str> {
	recognize(tuple((single_alpha, line)))(input)
}

pub(crate) fn metas(input: &str) -> IResult<&str, Vec<&str>> {
	many1(meta)(input)
}

fn single_alpha(input: &str) -> IResult<&str, &str> {
	recognize(one_of(
		"qwertyuioplokjhgfdsazxcvbnmQWERTYUIOPLOKJHGFDSAZXCVBNM",
	))(input)
}

fn triple_plus_minus_line(input: &str) -> IResult<&str, &str> {
	recognize(tuple((alt((tag("+++"), tag("---"))), line)))(input)
}

pub(crate) fn patch_header(input: &str) -> IResult<&str, &str> {
	recognize(tuple((
		metas,
		opt(triple_plus_minus_line),
		opt(triple_plus_minus_line),
	)))(input)
}

fn chunk_prefix(input: &str) -> IResult<&str, &str> {
	recognize(one_of("+-\\"))(input)
}

fn line_with_ending(input: &str) -> IResult<&str, &str> {
	recognize(tuple((not_line_ending, line_ending)))(input)
}

fn chunk_line(input: &str) -> IResult<&str, &str> {
	preceded(chunk_prefix, line_with_ending)(input)
}

fn chunk_body(input: &str) -> IResult<&str, String> {
	fold_many0(chunk_line, String::new, |mut patch, line| {
		if line == " No newline at end of file\n" {
			return patch;
		}

		patch.push_str(line);
		patch
	})(input)
}

fn chunk_header(input: &str) -> IResult<&str, &str> {
	recognize(tuple((peek(character('@')), line)))(input)
}

fn chunk(input: &str) -> IResult<&str, String> {
	preceded(chunk_header, chunk_body)(input)
}

fn chunks(input: &str) -> IResult<&str, String> {
	fold_many0(chunk, String::new, |mut patch, line| {
		patch.push_str(&line);
		patch
	})(input)
}

fn no_newline(input: &str) -> IResult<&str, &str> {
	recognize(tuple((peek(character('\\')), line)))(input)
}

fn patch_footer(input: &str) -> IResult<&str, Option<&str>> {
	opt(no_newline)(input)
}

pub(crate) fn patch(input: &str) -> IResult<&str, String> {
	tuple((patch_header, opt(chunks), patch_footer))(input)
		.map(|(i, (_, chunks, _))| (i, chunks.unwrap_or_else(String::new)))
}

fn gh_meta(input: &str) -> IResult<&str, &str> {
	recognize(tuple((single_alpha, line)))(input)
}

fn gh_metas(input: &str) -> IResult<&str, Vec<&str>> {
	many1(gh_meta)(input)
}

fn gh_patch_header(input: &str) -> IResult<&str, &str> {
	recognize(tuple((gh_metas, line, line)))(input)
}

fn chunk_line_with_context(input: &str) -> IResult<&str, &str> {
	recognize(tuple((not_line_ending, line_ending)))(input).and_then(|(i, parsed)| {
		if parsed.starts_with("diff --git") {
			Err(nom::Err::Error(NomError::new(i, ErrorKind::Many0)))
		} else {
			Ok((i, parsed))
		}
	})
}

fn chunk_body_with_context(input: &str) -> IResult<&str, String> {
	fold_many0(chunk_line_with_context, String::new, |mut patch, line| {
		if line.starts_with('+') || line.starts_with('-') {
			// Omit the first character.
			patch.push_str(&line[1..]);
		}

		patch
	})(input)
}

fn chunk_with_context(input: &str) -> IResult<&str, String> {
	preceded(chunk_header, chunk_body_with_context)(input)
}

fn chunks_with_context(input: &str) -> IResult<&str, String> {
	fold_many0(chunk_with_context, String::new, |mut patch, line| {
		patch.push_str(&line);
		patch
	})(input)
}

fn patch_with_context(input: &str) -> IResult<&str, GhPatch> {
	tuple((gh_patch_header, chunks_with_context, patch_footer))(input).map(
		|(i, (header, content, _))| {
			let file_name = file_name_from_header(header);

			let gh_patch = GhPatch { file_name, content };

			(i, gh_patch)
		},
	)
}

fn file_name_from_header(header: &str) -> String {
	let uf = "<unknown file>";

	// Extract the file name from a known-valid diff header.
	//
	// Example: diff --git a/README.md b/README.md
	header
		.split_whitespace()
		.nth(3)
		.unwrap_or(uf)
		.strip_prefix("b/")
		.unwrap_or(uf)
		.trim()
		.into()
}

fn patches_with_context(input: &str) -> IResult<&str, Vec<GhPatch>> {
	many0(patch_with_context)(input)
}

fn patches(input: &str) -> IResult<&str, Vec<String>> {
	many0(patch)(input)
}

#[derive(Debug)]
struct GhPatch {
	file_name: String,
	content: String,
}

pub struct Stat<'a> {
	pub lines_added: i64,
	pub lines_deleted: i64,
	pub file_name: &'a str,
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn parse_diff_header() {
		let input = "\
~~~\n\
~~~\n\
\n\
1\t0\trequirements/test_requirements.txt\n\
\n\
diff --git a/requirements/test_requirements.txt b/requirements/test_requirements.txt\n\
index 4e53f86d35..856ecf115e 100644\n\
--- a/requirements/test_requirements.txt\n\
+++ b/requirements/test_requirements.txt\n\
@@ -7,0 +8 @@ pytest==7.4.0\n\
+scipy-doctest\n";
		let (leftover, diffs) = diffs(input).unwrap();
		assert!(leftover.is_empty());
		assert_eq!(diffs.len(), 2);
		assert!(diffs.get(0).unwrap().file_diffs.is_empty());
		assert!(!(diffs.get(1).unwrap().file_diffs.is_empty()));
	}

	#[test]
	fn parse_stat() {
		let line = "7       0       Cargo.toml\n";

		let (remaining, stat) = stat(line).unwrap();
		let stat = stat.unwrap();

		assert_eq!("", remaining);
		assert_eq!(7, stat.lines_added);
		assert_eq!(0, stat.lines_deleted);
		assert_eq!("Cargo.toml", stat.file_name);
	}

	#[test]
	fn parse_stats() {
		let input = "\
7       0       Cargo.toml\n\
18      0       README.md\n\
3       0       src/main.rs\n";

		let (remaining, stats) = stats(input).unwrap();

		assert_eq!("", remaining);

		assert_eq!(7, stats[0].lines_added);
		assert_eq!(0, stats[0].lines_deleted);
		assert_eq!("Cargo.toml", stats[0].file_name);

		assert_eq!(18, stats[1].lines_added);
		assert_eq!(0, stats[1].lines_deleted);
		assert_eq!("README.md", stats[1].file_name);

		assert_eq!(3, stats[2].lines_added);
		assert_eq!(0, stats[2].lines_deleted);
		assert_eq!("src/main.rs", stats[2].file_name);
	}

	#[test]
	fn parse_patch_header() {
		let input = "\
diff --git a/src/main.rs b/src/main.rs\n\
new file mode 100644\n\
index 0000000..e7a11a9\n\
--- /dev/null\n\
+++ b/src/main.rs\n";

		let (remaining, header) = patch_header(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(input, header);
	}

	#[test]
	fn parse_patches() {
		let input = "\
diff --git a/src/main.rs b/src/main.rs\n\
new file mode 100644\n\
index 0000000..e7a11a9\n\
--- /dev/null\n\
+++ b/src/main.rs\n\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n\
diff --git a/src/main.rs b/src/main.rs\n\
new file mode 100644\n\
index 0000000..e7a11a9\n\
--- /dev/null\n\
+++ b/src/main.rs\n\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n";

		let expected_1 = "\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n";

		let expected_2 = "\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n";

		let (remaining, patches) = patches(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected_1, patches[0]);
		assert_eq!(expected_2, patches[1]);
	}

	#[test]
	fn parse_patch() {
		let input = "\
diff --git a/src/main.rs b/src/main.rs\n\
new file mode 100644\n\
index 0000000..e7a11a9\n\
--- /dev/null\n\
+++ b/src/main.rs\n\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n";

		let expected = "\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n";

		let (remaining, patch) = patch(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected, patch);
	}

	#[test]
	fn parse_chunks() {
		let input = "\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n";

		let expected = "\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n";

		let (remaining, patch) = chunks(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected, patch);
	}

	#[test]
	fn parse_chunk() {
		let input = "\
@@ -0,0 +1,116 @@\n\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n";

		let expected = "\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n";

		let (remaining, patch) = chunk(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected, patch);
	}

	#[test]
	fn parse_chunk_header() {
		let input = "@@ -0,0 +1,116 @@\n";
		let expected = "@@ -0,0 +1,116 @@\n";

		let (remaining, header) = chunk_header(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected, header);
	}

	#[test]
	fn parse_chunk_body() {
		let input = "\
+use clap::{Arg, App, SubCommand};\n\
+use serde::{Serialize, Deserialize};\n";

		let expected = "\
use clap::{Arg, App, SubCommand};\n\
use serde::{Serialize, Deserialize};\n";

		let (remaining, body) = chunk_body(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected, body);
	}

	#[test]
	fn parse_chunk_line() {
		let input = "+use clap::{Arg, App, SubCommand};\n";
		let expected = "use clap::{Arg, App, SubCommand};\n";

		let (remaining, line) = chunk_line(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected, line);
	}

	#[test]
	fn parse_plus_or_minus() {
		let input_plus = "+";
		let expected_plus = "+";

		let (remaining, c) = chunk_prefix(input_plus).unwrap();
		assert_eq!("", remaining);
		assert_eq!(expected_plus, c);

		let input_minus = "-";
		let expected_minus = "-";
		let (remaining, c) = chunk_prefix(input_minus).unwrap();
		assert_eq!("", remaining);
		assert_eq!(expected_minus, c);
	}

	#[test]
	fn parse_line_with_ending() {
		let input = "use clap::{Arg, App, SubCommand};\n";
		let expected = "use clap::{Arg, App, SubCommand};\n";

		let (remaining, line) = line_with_ending(input).unwrap();
		assert_eq!("", remaining);
		assert_eq!(expected, line);
	}

	#[test]
	fn parse_diff() {
		let input = r#"10      0       .gitignore
4       0       Cargo.toml
127     1       src/main.rs

diff --git a/.gitignore b/.gitignore
new file mode 100644
index 0000000..50c8301
--- /dev/null
+++ b/.gitignore
@@ -0,0 +1,10 @@
+# Generated by Cargo
+# will have compiled files and executables
+/target/
+
+# Remove Cargo.lock from gitignore if creating an executable, leave it for libraries
+# More information here https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html
+Cargo.lock
+
+# These are backup files generated by rustfmt
+**/*.rs.bk
\ No newline at end of file
diff --git a/Cargo.toml b/Cargo.toml
index 191135b..d91dabb 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -7,0 +8,4 @@ edition = "2018"
+clap = "2.33.0"
+petgraph = "0.4.13"
+serde = { version = "1.0.91", features = ["derive"] }
+serde_json = "1.0.39"
\ No newline at end of file
diff --git a/src/main.rs b/src/main.rs
index e7a11a9..4894a2e 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -0,0 +1,116 @@
+use clap::{Arg, App, SubCommand};
+use serde::{Serialize, Deserialize};
+// use petgraph::{Graph, Directed};
+// use std::collections::Vec;
+use std::process::Command;
+use std::str;
+
+// 1. Check that you're in a Git repo.
+//  * If not, error out.
+// 2. Run a command to get the Git log data.
+// 3. Deserialize that data with Serde, into a GitLog data structure.
+// 4. Convert the GitLog data structure into a CommitGraph.
+// 5. Run analyses on the CommitGraph.
+
+/*
+struct CommitGraph {
+    graph: Graph<Commit, String, Directed>,
+}
+
+struct AnalysisReport {
+    records: Vec<AnalysisRecord>,
+}
+
+trait Analysis {
+    fn analyze(commit_graph: &CommitGraph) -> AnalysisReport;
+}
+*/
+
+#[derive(Deserialize, Debug)]
+struct GitContributor {
+    name:   String,
+    email:  String,
+    date:   String,
+}
+
+#[derive(Deserialize, Debug)]
+struct GitCommit {
+    commit:                 String,
+    abbreviated_commit:     String,
+    tree:                   String,
+    abbreviated_tree:       String,
+    parent:                 String,
+    abbreviated_parent:     String,
+    refs:                   String,
+    encoding:               String,
+    subject:                String,
+    sanitized_subject_line: String,
+    body:                   String,
+    commit_notes:           String,
+    verification_flag:      String,
+    signer:                 String,
+    signer_key:             String,
+    author:                 GitContributor,
+    committer:              GitContributor,
+}
+
+#[derive(Deserialize, Debug)]
+struct GitLog {
+    commits: Vec<GitCommit>,
+}
+
+fn strip_characters(original: &str, to_strip: &str) -> String {
+    original.chars().filter(|&c| !to_strip.contains(c)).collect()
+}
+
+fn get_git_log() -> String {
+    // The format string being passed to Git, to get commit data.
+    // Note that this matches the GitLog struct above.
+    let format = "                                                  \
+        --pretty=format:                                            \
+            {                                               %n      \
+                \"commit\":                   \"%H\",       %n      \
+                \"abbreviated_commit\":       \"%h\",       %n      \
+                \"tree\":                     \"%T\",       %n      \
+                \"abbreviated_tree\":         \"%t\",       %n      \
+                \"parent\":                   \"%P\",       %n      \
+                \"abbreviated_parent\":       \"%p\",       %n      \
+                \"refs\":                     \"%D\",       %n      \
+                \"encoding\":                 \"%e\",       %n      \
+                \"subject\":                  \"%s\",       %n      \
+                \"sanitized_subject_line\":   \"%f\",       %n      \
+                \"body\":                     \"%b\",       %n      \
+                \"commit_notes\":             \"%N\",       %n      \
+                \"verification_flag\":        \"%G?\",      %n      \
+                \"signer\":                   \"%GS\",      %n      \
+                \"signer_key\":               \"%GK\",      %n      \
+                \"author\": {                               %n      \
+                    \"name\":                     \"%aN\",  %n      \
+                    \"email\":                    \"%aE\",  %n      \
+                    \"date\":                     \"%aD\"   %n      \
+                },                                          %n      \
+                \"commiter\": {                             %n      \
+                    \"name\":                     \"%cN\",  %n      \
+                    \"email\":                    \"%cE\",  %n      \
+                    \"date\":                     \"%cD\"   %n      \
+                }                                           %n      \
+            },";
+    let format = strip_characters(format, " ");
+
+    // Run the git command and extract the stdout as a string, stripping the trailing comma.
+    let output = Command::new("git")
+                    .args(&["log", &format])
+                    .output()
+                    .expect("failed to execute process");
+    let output = str::from_utf8(&output.stdout).unwrap().to_string();
+    let output = (&output[0..output.len() - 2]).to_string(); // Remove trailing comma.
+
+    // Wrap the result in brackets.
+    let mut result = String::new();
+    result.push('[');
+    result.push_str(&output);
+    result.push(']');
+
+    result
+}
+
@@ -2 +118,11 @@ fn main() {
-    println!("Hello, world!");
+    let matches = App::new("hipcheck")
+        .version("0.1")
+        .author("Andrew Lilley Brinker <abrinker@mitre.org>")
+        .about("Check Git history for concerning patterns")
+        .get_matches();
+
+    let log_string = get_git_log();
+
+    let gl: GitLog = serde_json::from_str(&log_string).unwrap();
+
+    println!("{:?}", gl);
"#;

		let expected = Diff {
			additions: Some(141),
			deletions: Some(1),
			file_diffs: vec![
				FileDiff {
					file_name: Arc::new(String::from(".gitignore")),
					additions: Some(10),
					deletions: Some(0),
					patch: String::from(
						r#"# Generated by Cargo
# will have compiled files and executables
/target/

# Remove Cargo.lock from gitignore if creating an executable, leave it for libraries
# More information here https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html
Cargo.lock

# These are backup files generated by rustfmt
**/*.rs.bk
"#,
					),
				},
				FileDiff {
					file_name: Arc::new(String::from("Cargo.toml")),
					additions: Some(4),
					deletions: Some(0),
					patch: String::from(
						r#"clap = "2.33.0"
petgraph = "0.4.13"
serde = { version = "1.0.91", features = ["derive"] }
serde_json = "1.0.39"
"#,
					),
				},
				FileDiff {
					file_name: Arc::new(String::from("src/main.rs")),
					additions: Some(127),
					deletions: Some(1),
					patch: String::from(
						r#"use clap::{Arg, App, SubCommand};
use serde::{Serialize, Deserialize};
// use petgraph::{Graph, Directed};
// use std::collections::Vec;
use std::process::Command;
use std::str;

// 1. Check that you're in a Git repo.
//  * If not, error out.
// 2. Run a command to get the Git log data.
// 3. Deserialize that data with Serde, into a GitLog data structure.
// 4. Convert the GitLog data structure into a CommitGraph.
// 5. Run analyses on the CommitGraph.

/*
struct CommitGraph {
    graph: Graph<Commit, String, Directed>,
}

struct AnalysisReport {
    records: Vec<AnalysisRecord>,
}

trait Analysis {
    fn analyze(commit_graph: &CommitGraph) -> AnalysisReport;
}
*/

#[derive(Deserialize, Debug)]
struct GitContributor {
    name:   String,
    email:  String,
    date:   String,
}

#[derive(Deserialize, Debug)]
struct GitCommit {
    commit:                 String,
    abbreviated_commit:     String,
    tree:                   String,
    abbreviated_tree:       String,
    parent:                 String,
    abbreviated_parent:     String,
    refs:                   String,
    encoding:               String,
    subject:                String,
    sanitized_subject_line: String,
    body:                   String,
    commit_notes:           String,
    verification_flag:      String,
    signer:                 String,
    signer_key:             String,
    author:                 GitContributor,
    committer:              GitContributor,
}

#[derive(Deserialize, Debug)]
struct GitLog {
    commits: Vec<GitCommit>,
}

fn strip_characters(original: &str, to_strip: &str) -> String {
    original.chars().filter(|&c| !to_strip.contains(c)).collect()
}

fn get_git_log() -> String {
    // The format string being passed to Git, to get commit data.
    // Note that this matches the GitLog struct above.
    let format = "                                                  \
        --pretty=format:                                            \
            {                                               %n      \
                \"commit\":                   \"%H\",       %n      \
                \"abbreviated_commit\":       \"%h\",       %n      \
                \"tree\":                     \"%T\",       %n      \
                \"abbreviated_tree\":         \"%t\",       %n      \
                \"parent\":                   \"%P\",       %n      \
                \"abbreviated_parent\":       \"%p\",       %n      \
                \"refs\":                     \"%D\",       %n      \
                \"encoding\":                 \"%e\",       %n      \
                \"subject\":                  \"%s\",       %n      \
                \"sanitized_subject_line\":   \"%f\",       %n      \
                \"body\":                     \"%b\",       %n      \
                \"commit_notes\":             \"%N\",       %n      \
                \"verification_flag\":        \"%G?\",      %n      \
                \"signer\":                   \"%GS\",      %n      \
                \"signer_key\":               \"%GK\",      %n      \
                \"author\": {                               %n      \
                    \"name\":                     \"%aN\",  %n      \
                    \"email\":                    \"%aE\",  %n      \
                    \"date\":                     \"%aD\"   %n      \
                },                                          %n      \
                \"commiter\": {                             %n      \
                    \"name\":                     \"%cN\",  %n      \
                    \"email\":                    \"%cE\",  %n      \
                    \"date\":                     \"%cD\"   %n      \
                }                                           %n      \
            },";
    let format = strip_characters(format, " ");

    // Run the git command and extract the stdout as a string, stripping the trailing comma.
    let output = Command::new("git")
                    .args(&["log", &format])
                    .output()
                    .expect("failed to execute process");
    let output = str::from_utf8(&output.stdout).unwrap().to_string();
    let output = (&output[0..output.len() - 2]).to_string(); // Remove trailing comma.

    // Wrap the result in brackets.
    let mut result = String::new();
    result.push('[');
    result.push_str(&output);
    result.push(']');

    result
}

    println!("Hello, world!");
    let matches = App::new("hipcheck")
        .version("0.1")
        .author("Andrew Lilley Brinker <abrinker@mitre.org>")
        .about("Check Git history for concerning patterns")
        .get_matches();

    let log_string = get_git_log();

    let gl: GitLog = serde_json::from_str(&log_string).unwrap();

    println!("{:?}", gl);
"#,
					),
				},
			],
		};

		let (remaining, diff) = diff(input).unwrap();

		assert_eq!("", remaining);
		assert_eq!(expected, diff);
	}

	#[test]
	fn parse_patch_with_context() {
		let input = r#"diff --git a/README.md b/README.md
index 20b42ecfdf..b0f30e8e35 100644
--- a/README.md
+++ b/README.md
@@ -432,24 +432,31 @@ Other Style Guides
		});

		// bad
-    inbox.filter((msg) => {
-      const { subject, author } = msg;
-      if (subject === 'Mockingbird') {
-        return author === 'Harper Lee';
-      } else {
-        return false;
-      }
-    });
+    var indexMap = myArray.reduce(function(memo, item, index) {
+      memo[item] = index;
+    }, {});

-    // good
-    inbox.filter((msg) => {
-      const { subject, author } = msg;
-      if (subject === 'Mockingbird') {
-        return author === 'Harper Lee';
-      }

-      return false;
+    // good
+    var indexMap = myArray.reduce(function(memo, item, index) {
+      memo[item] = index;
+      return memo;
+    }, {});
+
+
+    // bad
+    const alpha = people.sort((lastOne, nextOne) => {
+      const [aLast, aFirst] = lastOne.split(', ');
+      const [bLast, bFirst] = nextOne.split(', ');
		});
+
+    // good
+    const alpha = people.sort((lastOne, nextOne) => {
+      const [aLast, aFirst] = lastOne.split(', ');
+      const [bLast, bFirst] = nextOne.split(', ');
+      return aLast > bLast ? 1 : -1;
+    });
+
		```

	<a name="arrays--bracket-newline"></a>
"#;

		let expected = r#"    inbox.filter((msg) => {
      const { subject, author } = msg;
      if (subject === 'Mockingbird') {
        return author === 'Harper Lee';
      } else {
        return false;
      }
    });
    var indexMap = myArray.reduce(function(memo, item, index) {
      memo[item] = index;
    }, {});
    // good
    inbox.filter((msg) => {
      const { subject, author } = msg;
      if (subject === 'Mockingbird') {
        return author === 'Harper Lee';
      }
      return false;
    // good
    var indexMap = myArray.reduce(function(memo, item, index) {
      memo[item] = index;
      return memo;
    }, {});


    // bad
    const alpha = people.sort((lastOne, nextOne) => {
      const [aLast, aFirst] = lastOne.split(', ');
      const [bLast, bFirst] = nextOne.split(', ');

    // good
    const alpha = people.sort((lastOne, nextOne) => {
      const [aLast, aFirst] = lastOne.split(', ');
      const [bLast, bFirst] = nextOne.split(', ');
      return aLast > bLast ? 1 : -1;
    });

"#;

		let (remaining, patch) = patch_with_context(input).unwrap();

		assert_eq!(
			"", remaining,
			"expected nothing remaining, got '{}'",
			remaining
		);
		assert_eq!(expected, patch.content);
	}

	#[test]
	fn parse_gh_diff() {
		let input = r#"diff --git a/README.md b/README.md
index 20b42ecfdf..b0f30e8e35 100644
--- a/README.md
+++ b/README.md
@@ -432,24 +432,31 @@ Other Style Guides
		});

		// bad
-    inbox.filter((msg) => {
-      const { subject, author } = msg;
-      if (subject === 'Mockingbird') {
-        return author === 'Harper Lee';
-      } else {
-        return false;
-      }
-    });
+    var indexMap = myArray.reduce(function(memo, item, index) {
+      memo[item] = index;
+    }, {});

-    // good
-    inbox.filter((msg) => {
-      const { subject, author } = msg;
-      if (subject === 'Mockingbird') {
-        return author === 'Harper Lee';
-      }

-      return false;
+    // good
+    var indexMap = myArray.reduce(function(memo, item, index) {
+      memo[item] = index;
+      return memo;
+    }, {});
+
+
+    // bad
+    const alpha = people.sort((lastOne, nextOne) => {
+      const [aLast, aFirst] = lastOne.split(', ');
+      const [bLast, bFirst] = nextOne.split(', ');
		});
+
+    // good
+    const alpha = people.sort((lastOne, nextOne) => {
+      const [aLast, aFirst] = lastOne.split(', ');
+      const [bLast, bFirst] = nextOne.split(', ');
+      return aLast > bLast ? 1 : -1;
+    });
+
		```

	<a name="arrays--bracket-newline"></a>
"#;

		let expected = r#"    inbox.filter((msg) => {
      const { subject, author } = msg;
      if (subject === 'Mockingbird') {
        return author === 'Harper Lee';
      } else {
        return false;
      }
    });
    var indexMap = myArray.reduce(function(memo, item, index) {
      memo[item] = index;
    }, {});
    // good
    inbox.filter((msg) => {
      const { subject, author } = msg;
      if (subject === 'Mockingbird') {
        return author === 'Harper Lee';
      }
      return false;
    // good
    var indexMap = myArray.reduce(function(memo, item, index) {
      memo[item] = index;
      return memo;
    }, {});


    // bad
    const alpha = people.sort((lastOne, nextOne) => {
      const [aLast, aFirst] = lastOne.split(', ');
      const [bLast, bFirst] = nextOne.split(', ');

    // good
    const alpha = people.sort((lastOne, nextOne) => {
      const [aLast, aFirst] = lastOne.split(', ');
      const [bLast, bFirst] = nextOne.split(', ');
      return aLast > bLast ? 1 : -1;
    });

"#;

		let (remaining, diff) = gh_diff(input).unwrap();

		assert_eq!(
			"", remaining,
			"expected nothing remaining, got '{}'",
			remaining
		);

		assert_eq!(None, diff.additions);
		assert_eq!(None, diff.deletions);

		assert_eq!("README.md", diff.file_diffs[0].file_name.as_ref());
		assert_eq!(None, diff.file_diffs[0].additions);
		assert_eq!(None, diff.file_diffs[0].deletions);
		assert_eq!(expected, diff.file_diffs[0].patch)
	}
}
