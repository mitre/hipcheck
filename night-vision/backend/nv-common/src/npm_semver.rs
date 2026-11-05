//! Applies NPM version ranges to concrete lists of published versions.
//!
//! What the resolver needs to know is which published versions of a package a
//! range like `^1.2.3` or `~1.2.3` permits. This module answers that in two
//! steps (see [`docs/backend/resolving-packages.md`] for the design):
//!
//! 1. [`parse_range`] parses a raw range string, exactly as written, using
//!    [node-semver] — a Rust port of npm's own semver library. There's no
//!    intermediate rewriting into some other form of range; node-semver's own
//!    parser and [`NpmRange::satisfies`] already implement npm's range
//!    semantics (`^`/`~` near-zero tightening, `-0` prerelease handling on
//!    exclusive bounds, x-ranges, hyphen ranges, and so on), so there's
//!    nothing for us to re-derive.
//! 2. [`elaborate_npm_version_bounds`] filters a sorted list of published
//!    versions down to the ones the range permits.
//!
//! ```
//! use nv_common::npm_semver::{NpmVersion, elaborate_npm_version_bounds, parse_range};
//!
//! let range = parse_range("^1.2.3").unwrap();
//! let published: Vec<NpmVersion> = ["1.2.2", "1.2.3", "1.9.0", "2.0.0"]
//!     .iter()
//!     .map(|v| NpmVersion::parse(v).unwrap())
//!     .collect();
//!
//! let matching = elaborate_npm_version_bounds(&published, &range).unwrap();
//! assert_eq!(matching, [&published[1], &published[2]]); // 1.2.3 and 1.9.0
//!
//! // Junk is rejected up front, so we don't query a registry for nothing.
//! assert!(parse_range("^1.2.3.4").is_err());
//! ```
//!
//! ## Known node-semver quirks we inherit
//!
//! Because ranges go straight to node-semver with no validation or rewriting
//! layer of our own, we take on its parsing behavior as-is, including two
//! quirks worth knowing about:
//!
//! - It's more lenient than npm's own parser about unrecognized tokens —
//!   `"1.2.3 foo"` parses successfully as `1.2.3`, silently dropping `foo`,
//!   rather than erroring.
//! - As of node-semver 2.2.0, `<=` with a partial version is parsed too
//!   strictly: `<=1.2` becomes `<=1.2.0-0`, which wrongly excludes `1.2.0`
//!   itself (real npm treats it as covering all of `1.2.x`, i.e. `<1.3.0-0`).
//!
//! Both are pinned in this module's tests so an upgrade that changes either
//! behavior doesn't pass silently.
//!
//! [`docs/backend/resolving-packages.md`]: ../../../../docs/backend/resolving-packages.md
//! [node-semver]: https://docs.rs/node-semver

use std::fmt::{Display, Formatter, Result as FmtResult};

// The engine's types, re-exported under names that don't collide with a
// hypothetical `Version`/`Range` of our own elsewhere in this crate.
pub use node_semver::{Range as NpmRange, SemverError, Version as NpmVersion};

/// Parse a raw NPM version range, exactly as written, using node-semver.
///
/// Anything node-semver rejects comes back as an [`NpmRangeError::InvalidRange`], so
/// the resolver can bail before doing a registry lookup.
pub fn parse_range(raw: &str) -> Result<NpmRange, NpmRangeError> {
	NpmRange::parse(raw).map_err(NpmRangeError::InvalidRange)
}

/// Which of a package's published versions fall within `range`.
///
/// `known_pkg_versions` must be sorted in increasing order (registry data
/// normally is); unsorted input is an [`NpmRangeError::UnsortedVersions`] rather than
/// a silently wrong answer. Matches come back as references into
/// `known_pkg_versions`, in that same increasing order, so the first and last
/// entries are the concrete bounds of the range. An empty `Vec` means no
/// published version satisfies the range.
///
/// This returns the matching versions themselves rather than one contiguous
/// `&[NpmVersion]` sub-slice, because matches aren't always contiguous: an OR
/// range (`^1 || ^3`) skips everything in between, and prerelease versions
/// sort in between releases (`1.4.0 < 1.5.0-beta.1 < 1.5.0`) while usually
/// not matching.
pub fn elaborate_npm_version_bounds<'a>(
	known_pkg_versions: &'a [NpmVersion],
	range: &NpmRange,
) -> Result<Vec<&'a NpmVersion>, NpmRangeError> {
	if !known_pkg_versions.is_sorted() {
		return Err(NpmRangeError::UnsortedVersions);
	}
	Ok(known_pkg_versions
		.iter()
		.filter(|version| range.satisfies(version))
		.collect())
}

/// Something that keeps us from answering a bounds query.
///
/// Either of these means the resolver shouldn't do a registry lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NpmRangeError {
	/// node-semver rejected the raw range string.
	InvalidRange(SemverError),

	/// The version list given to [`elaborate_npm_version_bounds`] wasn't
	/// sorted in increasing order.
	UnsortedVersions,
}

impl Display for NpmRangeError {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		match self {
			Self::InvalidRange(_) => write!(f, "invalid version range"),
			Self::UnsortedVersions => {
				write!(
					f,
					"known package versions must be sorted in increasing order"
				)
			}
		}
	}
}

impl std::error::Error for NpmRangeError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::InvalidRange(source) => Some(source),
			Self::UnsortedVersions => None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Little helper: parse a list of published-version strings.
	fn published(raw: &[&str]) -> Vec<NpmVersion> {
		raw.iter()
			.map(|v| NpmVersion::parse(v).unwrap_or_else(|e| panic!("bad test version `{v}`: {e}")))
			.collect()
	}

	/// Little helper: run the whole pipeline and render the matches back to
	/// strings for easy comparison.
	fn matching(list: &[NpmVersion], raw_range: &str) -> Vec<String> {
		let range = parse_range(raw_range)
			.unwrap_or_else(|e| panic!("expected `{raw_range}` to parse: {e}"));
		let matches = elaborate_npm_version_bounds(list, &range)
			.unwrap_or_else(|e| panic!("expected bounds query for `{raw_range}` to succeed: {e}"));
		matches
			.into_iter()
			.map(std::string::ToString::to_string)
			.collect()
	}

	#[test]
	fn bounds_filter_published_versions() {
		let list = published(&["1.0.0", "1.2.2", "1.2.3", "1.5.0", "2.0.0"]);
		assert_eq!(matching(&list, "^1.2.3"), ["1.2.3", "1.5.0"]);
	}

	#[test]
	fn bounds_handle_tilde_and_x_ranges_too() {
		let list = published(&["1.1.9", "1.2.0", "1.2.9", "1.3.0"]);
		assert_eq!(matching(&list, "~1.2.3"), ["1.2.9"]);
		assert_eq!(matching(&list, "1.2.x"), ["1.2.0", "1.2.9"]);
	}

	#[test]
	fn bounds_of_or_ranges_are_not_contiguous() {
		// The union skips 2.0.0 in the middle — this is why the API returns
		// the matching versions rather than one contiguous sub-slice.
		let list = published(&["1.0.0", "1.5.0", "2.0.0", "3.0.0", "3.1.0"]);
		assert_eq!(
			matching(&list, "^1.0.0 || ^3.0.0"),
			["1.0.0", "1.5.0", "3.0.0", "3.1.0"]
		);
	}

	#[test]
	fn bounds_skip_interleaved_prereleases() {
		// Prereleases sort between releases (1.4.0 < 1.5.0-beta.1 < 1.5.0) but
		// don't satisfy a release range — the other reason matches can't be
		// one contiguous sub-slice.
		let list = published(&["1.4.0", "1.5.0-beta.1", "1.5.0"]);
		assert_eq!(matching(&list, "^1.2.3"), ["1.4.0", "1.5.0"]);
	}

	#[test]
	fn bounds_include_prereleases_on_the_same_version_tuple() {
		// npm's prerelease rule: a prerelease version can match only when some
		// comparator carries a prerelease on the same major.minor.patch.
		let list = published(&["1.2.3-beta.1", "1.2.3-beta.2", "1.2.3", "1.3.0"]);
		assert_eq!(
			matching(&list, "^1.2.3-beta.1"),
			["1.2.3-beta.1", "1.2.3-beta.2", "1.2.3", "1.3.0"]
		);
	}

	#[test]
	fn bounds_can_be_empty() {
		let list = published(&["1.0.0", "1.1.0"]);
		assert!(matching(&list, "^2.0.0").is_empty());
	}

	#[test]
	fn bounds_reject_unsorted_versions() {
		let list = published(&["2.0.0", "1.0.0"]);
		let range = parse_range("^1.0.0").expect("valid range");
		assert_eq!(
			elaborate_npm_version_bounds(&list, &range),
			Err(NpmRangeError::UnsortedVersions)
		);
	}

	#[test]
	fn parse_range_rejects_malformed_input() {
		parse_range("1.2.3.4").unwrap_err();
		parse_range("not a version").unwrap_err();
	}

	// ========================================================================
	// Known node-semver quirks, pinned as *accepted* behavior
	// ------------------------------------------------------------------------
	// `parse_range` hands raw range strings straight to node-semver with no
	// validation or rewriting layer in front of it, so whatever node-semver
	// does is what this module does. That's a deliberate simplification (see
	// the module docs), but it means two of node-semver's rougher edges are
	// now our behavior too. These tests exist so that if a future node-semver
	// upgrade changes either one, it shows up as a failing test to
	// investigate rather than a silent behavior change.

	/// node-semver drops unrecognized tokens instead of erroring on them:
	/// parsing `"1.2.3 foo"` succeeds and quietly behaves like `"1.2.3"`. A
	/// stricter front-end would reject this outright; we don't have one, so
	/// we accept it.
	#[test]
	fn unrecognized_tokens_are_silently_dropped_not_rejected() {
		for input in ["1.2.3 foo", "foo 1.2.3", "~1.y 1.2.3", "1.2.3 ~1.y"] {
			parse_range(input).unwrap_or_else(|e| {
				panic!(
					"expected node-semver to laxly accept `{input}` by dropping the \
                     unrecognized token; it now errors ({e}) instead — if that's \
                     node-semver becoming stricter, this test can be deleted"
				)
			});
		}
	}

	/// node-semver 2.2.0 parses `<=` with a partial version too strictly:
	/// `<=1.2` becomes `<=1.2.0-0`, excluding `1.2.0` itself. Real npm treats
	/// an inclusive bound on a partial version as covering the whole omitted
	/// range — `<=1.2` should behave like `<1.3.0-0`, matching every `1.2.x`
	/// patch.
	///
	/// This means `1.2.0` (and any other `1.2.x`) is wrongly excluded from a
	/// `<=1.2` or `<=1` range under our current dependency version. If this
	/// starts failing after a `node-semver` bump, the bug may be fixed
	/// upstream — double check against npm's own behavior before deleting
	/// this test.
	#[test]
	fn engine_mishandles_lte_partial_versions() {
		let versions = [
			NpmVersion::parse("1.1.0").unwrap(),
			NpmVersion::parse("1.2.0").unwrap(),
			NpmVersion::parse("1.2.9").unwrap(),
			NpmVersion::parse("1.3.0").unwrap(),
		];

		let range = parse_range("<=1.2").expect("node-semver accepts this syntax");
		let matches = elaborate_npm_version_bounds(&versions, &range).expect("sorted input");
		assert_eq!(
			matches,
			[&versions[0]],
			"expected the known `<=1.2` bug (excluding 1.2.0 and 1.2.9) — if this \
             now includes them, node-semver's `<=` handling may have been fixed"
		);
	}
}
