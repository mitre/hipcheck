// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::data::Dependencies;
use crate::data::Lang;
use crate::error::Result;
use crate::metric::MetricProvider;
use crate::util::fs as file;
use maplit::hashmap;
use serde::Deserialize;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::AsRef;
use std::fmt;
use std::fmt::Display;
use std::path::Path;
use std::str;
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct TypoOutput {
	pub typos: Vec<TypoDep>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct TypoDep {
	pub dependency: Arc<String>,
	pub typo: Typo,
}

pub fn typo_metric(db: &dyn MetricProvider) -> Result<Arc<TypoOutput>> {
	log::debug!("running typo metric");

	let typo_file = TypoFile::load_from(&db.typo_file()).context("failed to load typo file")?;

	let dependencies = db.dependencies().context("failed to get dependencies")?;

	let typo_output = match dependencies.language {
		Lang::JavaScript => typos_for_javascript(&typo_file, dependencies),
		Lang::Unknown => Err(crate::error::Error::msg(
			"failed to identify a known language",
		)),
	}?;

	log::info!("completed typo metric");

	Ok(typo_output)
}

fn typos_for_javascript(
	typo_file: &TypoFile,
	dependencies: Arc<Dependencies>,
) -> Result<Arc<TypoOutput>> {
	let mut typos = Vec::new();

	for legit_name in &typo_file.languages.javascript {
		let fuzzer = NameFuzzer::new(legit_name);

		for dependency in &dependencies.deps {
			for typo in fuzzer.fuzz(dependency) {
				typos.push(TypoDep {
					dependency: Arc::clone(dependency),
					typo: typo.clone(),
				})
			}
		}
	}

	Ok(Arc::new(TypoOutput { typos }))
}

#[derive(Debug, Deserialize)]
struct TypoFile {
	languages: Languages,
}

#[derive(Debug, Deserialize)]
struct Languages {
	javascript: Vec<String>,
}

impl TypoFile {
	fn load_from(typo_path: &Path) -> Result<TypoFile> {
		file::exists(typo_path).context("typo file does not exist")?;
		let typo_file = file::read_toml(typo_path).context("failed to open typo file")?;

		Ok(typo_file)
	}
}

#[derive(Debug, Clone)]
pub struct NameFuzzer<'t> {
	// A map of strings which may be typos to the notes for what they may be
	// typos of. Fuzzing then only needs to hash the string and look it up in the
	// typo hash map.
	typos: HashMap<String, Vec<Typo>>,
	// The list of original names.
	name: &'t str,
}

impl<'t> NameFuzzer<'t> {
	/// Construct a new NameFuzzer for the given corpus.
	pub fn new(name: &'t str) -> NameFuzzer<'t> {
		let typos = {
			let keyboards = vec![
				KeyboardLayout::qwerty(),
				KeyboardLayout::qwertz(),
				KeyboardLayout::azerty(),
			];

			let homoglyphs = vec![Homoglyphs::ascii()];

			get_typos(name, &keyboards, &homoglyphs).iter().fold(
				HashMap::new(),
				|mut typos: HashMap<String, Vec<Typo>>, typo| {
					typos
						.entry(typo.to_str().to_owned())
						.and_modify(|val| val.push(typo.clone()))
						.or_insert_with(|| vec![typo.clone()]);

					typos
				},
			)
		};

		NameFuzzer { typos, name }
	}

	/// Check the name against the set of known typos for the corpus to generate
	/// a list of possible typos.
	///
	/// Returns an empty slice if no typos were found.
	pub fn fuzz(&self, name: &str) -> &[Typo] {
		if self.name == name {
			return &[];
		}

		self.typos.get(name).map(AsRef::as_ref).unwrap_or(&[])
	}
}

#[inline]
fn get_typos(name: &str, keyboards: &[KeyboardLayout], homoglyphs: &[Homoglyphs]) -> Vec<Typo> {
	let mut results = Vec::new();

	// Get all the kinds of typos.
	get_addition_typos(&mut results, name);
	get_bitsquatting_typos(&mut results, name);
	get_hyphenation_typos(&mut results, name);
	get_insertion_typos(&mut results, name, keyboards);
	get_omission_typos(&mut results, name);
	get_repetition_typos(&mut results, name);
	get_replacement_typos(&mut results, name, keyboards);
	get_transposition_typos(&mut results, name);
	get_vowel_swap_typos(&mut results, name);
	get_pluralization_typos(&mut results, name);
	get_homoglyph_typos(&mut results, name, homoglyphs);

	// The methods above might generate duplicates. This removes them.
	//
	// Sorting is done with sort() rather than sort_unstable() to ensure that the
	// order of the different kinds of typos is preserved, to make testing easier.
	//
	// Given that a fuzzer should only be constructed once for a corpus, the cost
	// difference of this is expected to be negligible.
	results.sort();
	results.dedup();

	results
}

#[inline]
fn get_addition_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend(
		(b'_'..b'z')
			.map(char::from)
			.map(|c| format!("{}{}", name, c))
			.filter(|t| t != name)
			.map(Typo::addition),
	);
}

#[inline]
fn get_bitsquatting_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend(
		[1, 2, 4, 8, 16, 32, 64, 128]
			.iter()
			.flat_map(|mask| {
				name.bytes().enumerate().map(move |(index, byte)| {
					let c = mask ^ byte;

					// If the corrupted byte is within the proper ASCII range, then
					// produce a new string including the corrupted byte.
					if (c == b'-') || (c == b'_') || c.is_ascii_digit() || c.is_ascii_lowercase() {
						let mut corrupted = name.to_owned();

						// We have already ensured the new byte is a valid ASCII byte, so this
						// use of unsafe is alright.
						let corrupted_bytes = unsafe { corrupted.as_bytes_mut() };
						corrupted_bytes[index] = c;

						Some(corrupted)
					} else {
						None
					}
				})
			})
			.flatten()
			.filter(|t| t != name)
			.map(Typo::bitsquatting),
	);
}

#[inline]
fn get_hyphenation_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend(
		name.chars()
			.enumerate()
			.map(|(index, _)| {
				let mut corrupted = name.to_owned();
				corrupted.insert(index, '-');
				corrupted
			})
			.filter(|t| t != name)
			.map(Typo::hyphenation),
	);
}

#[inline]
fn get_insertion_typos(results: &mut Vec<Typo>, name: &str, keyboards: &[KeyboardLayout]) {
	results.extend(
		keyboards
			.iter()
			.flat_map(|keyboard| {
				name.chars().enumerate().flat_map(move |(index, c)| {
					let mut corruptions = Vec::new();

					if keyboard.neighbors().contains_key(&c) {
						for neighbor in &keyboard.neighbors()[&c] {
							// Before the current character.
							let mut corrupted_before = name.to_owned();
							corrupted_before.insert(index, *neighbor);
							corruptions.push(corrupted_before);

							// After the current character.
							let mut corrupted_after = name.to_owned();
							corrupted_after.insert(index + 1, *neighbor);
							corruptions.push(corrupted_after);
						}
					}

					corruptions
				})
			})
			.filter(|t| t != name)
			.map(Typo::insertion),
	);
}

#[inline]
fn get_omission_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend(
		name.chars()
			.enumerate()
			.map(|(index, _)| {
				let mut corrupted = name.to_owned();
				corrupted.remove(index);
				corrupted
			})
			.filter(|t| t != name)
			.map(Typo::omission),
	);
}

#[inline]
fn get_repetition_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend(
		name.chars()
			.enumerate()
			.map(|(index, c)| {
				let mut corrupted = name.to_owned();
				corrupted.insert(index, c);
				corrupted
			})
			.filter(|t| t != name)
			.map(Typo::repetition),
	);
}

#[inline]
fn get_replacement_typos(results: &mut Vec<Typo>, name: &str, keyboards: &[KeyboardLayout]) {
	results.extend(
		keyboards
			.iter()
			.flat_map(|keyboard| {
				name.chars().enumerate().flat_map(move |(index, c)| {
					let mut corruptions = Vec::new();

					if keyboard.neighbors().contains_key(&c) {
						for neighbor in &keyboard.neighbors()[&c] {
							let mut corrupted = name.to_owned();
							corrupted.replace_range(index..=index, &neighbor.to_string());
							corruptions.push(corrupted);
						}
					}

					corruptions
				})
			})
			.filter(|t| t != name)
			.map(Typo::replacement),
	);
}

#[inline]
fn get_transposition_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend({
		// Credit for this code to shepmaster on Stack Overflow.
		//
		// https://codereview.stackexchange.com/questions/155294/transposing-characters-in-a-string
		let bytes = name.as_bytes();

		(1..bytes.len())
			.map(move |i| {
				let mut transpose = bytes.to_owned();
				transpose.swap(i - 1, i);
				String::from_utf8(transpose).expect("Invalid UTF-8")
			})
			.filter(|t| t != name)
			.map(Typo::transposition)
	});
}

#[inline]
fn get_vowel_swap_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend(
		name.chars()
			.enumerate()
			.flat_map(|(index, c)| {
				let vowels = ['a', 'e', 'i', 'o', 'u'];
				let mut corruptions = Vec::new();

				for vowel in &vowels {
					if vowels.contains(&c) {
						let mut corrupted = name.to_owned();
						corrupted.replace_range(index..=index, &vowel.to_string());
						corruptions.push(corrupted);
					}
				}

				corruptions
			})
			.filter(|t| t != name)
			.map(Typo::vowel_swap),
	);
}

#[inline]
fn get_pluralization_typos(results: &mut Vec<Typo>, name: &str) {
	results.extend(
		name.chars()
			.enumerate()
			.map(|(index, _c)| {
				let mut corrupted = name.to_owned();
				corrupted.insert(index + 1, 's');
				corrupted
			})
			.filter(|t| t != name)
			.map(Typo::pluralization),
	);
}

#[inline]
fn get_homoglyph_typos(results: &mut Vec<Typo>, name: &str, homoglyphs: &[Homoglyphs]) {
	results.extend(
		homoglyphs
			.iter()
			.flat_map(|homoglph| {
				name.chars().enumerate().flat_map(move |(index, c)| {
					let mut corruptions = Vec::new();

					if homoglph.glyphs().contains_key(&c) {
						for glyph in &homoglph.glyphs()[&c] {
							let mut corrupted = name.to_owned();
							corrupted.replace_range(index..=index, &glyph.to_string());
							corruptions.push(corrupted);
						}
					}

					corruptions
				})
			})
			.filter(|t| t != name)
			.map(Typo::homoglyph),
	);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Typo {
	kind: TypoKind,
	typo: String,
}

impl Typo {
	#[inline]
	pub fn addition(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Addition,
			typo,
		}
	}

	#[inline]
	pub fn bitsquatting(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Bitsquatting,
			typo,
		}
	}

	#[inline]
	pub fn hyphenation(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Hyphenation,
			typo,
		}
	}

	#[inline]
	pub fn insertion(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Insertion,
			typo,
		}
	}

	#[inline]
	pub fn omission(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Omission,
			typo,
		}
	}

	#[inline]
	pub fn repetition(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Repetition,
			typo,
		}
	}

	#[inline]
	pub fn replacement(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Replacement,
			typo,
		}
	}

	#[inline]
	pub fn transposition(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Transposition,
			typo,
		}
	}

	#[inline]
	pub fn vowel_swap(typo: String) -> Typo {
		Typo {
			kind: TypoKind::VowelSwap,
			typo,
		}
	}

	#[inline]
	pub fn pluralization(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Pluralization,
			typo,
		}
	}

	#[inline]
	pub fn homoglyph(typo: String) -> Typo {
		Typo {
			kind: TypoKind::Homoglyph,
			typo,
		}
	}

	#[inline]
	pub fn to_str(&self) -> &str {
		&self.typo
	}
}

impl PartialOrd for Typo {
	#[inline]
	fn partial_cmp(&self, other: &Typo) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for Typo {
	#[inline]
	fn cmp(&self, other: &Typo) -> Ordering {
		self.typo.cmp(&other.typo)
	}
}

impl PartialEq<&Typo> for Typo {
	#[inline]
	fn eq(&self, other: &&Typo) -> bool {
		self.eq(*other)
	}
}

impl PartialEq<Typo> for &Typo {
	#[inline]
	fn eq(&self, other: &Typo) -> bool {
		(*self).eq(other)
	}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
enum TypoKind {
	Addition,
	Bitsquatting,
	Hyphenation,
	Insertion,
	Omission,
	Repetition,
	Replacement,
	Transposition,
	VowelSwap,
	Pluralization,
	Homoglyph,
}

impl Display for TypoKind {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			TypoKind::Addition => write!(f, "addition"),
			TypoKind::Bitsquatting => write!(f, "bitsquatting"),
			TypoKind::Hyphenation => write!(f, "hyphenation"),
			TypoKind::Insertion => write!(f, "insertion"),
			TypoKind::Omission => write!(f, "omission"),
			TypoKind::Repetition => write!(f, "repetition"),
			TypoKind::Replacement => write!(f, "replacement"),
			TypoKind::Transposition => write!(f, "transposition"),
			TypoKind::VowelSwap => write!(f, "vowel swap"),
			TypoKind::Pluralization => write!(f, "pluralization"),
			TypoKind::Homoglyph => write!(f, "homoglyph"),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Homoglyphs(HashMap<char, Vec<char>>);

impl Homoglyphs {
	#[inline]
	pub fn new(homoglyphs: HashMap<char, Vec<char>>) -> Homoglyphs {
		Homoglyphs(homoglyphs)
	}

	#[inline]
	pub fn ascii() -> Homoglyphs {
		Homoglyphs::new(hashmap! {
			'O' => vec!['0'],
			'0' => vec!['O'],
			'l' => vec!['I'],
			'I' => vec!['l'],
		})
	}

	#[inline]
	pub fn glyphs(&self) -> &HashMap<char, Vec<char>> {
		&self.0
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardLayout {
	neighbors: HashMap<char, Vec<char>>,
}

impl KeyboardLayout {
	#[inline]
	pub fn new(neighbors: HashMap<char, Vec<char>>) -> KeyboardLayout {
		KeyboardLayout { neighbors }
	}

	#[inline]
	pub fn qwerty() -> KeyboardLayout {
		KeyboardLayout::new(hashmap! {
			'1' => vec!['2', 'q'],
			'2' => vec!['3', 'w', 'q', '1'],
			'3' => vec!['4', 'e', 'w', '2'],
			'4' => vec!['5', 'r', 'e', '3'],
			'5' => vec!['6', 't', 'r', '4'],
			'6' => vec!['7', 'y', 't', '5'],
			'7' => vec!['8', 'u', 'y', '6'],
			'8' => vec!['9', 'i', 'u', '7'],
			'9' => vec!['0', 'o', 'i', '8'],
			'0' => vec!['p', 'o', '9'],
			'q' => vec!['1', '2', 'w', 'a'],
			'w' => vec!['3', 'e', 's', 'a', 'q', '2'],
			'e' => vec!['4', 'r', 'd', 's', 'w', '3'],
			'r' => vec!['5', 't', 'f', 'd', 'e', '4'],
			't' => vec!['6', 'y', 'g', 'f', 'r', '5'],
			'y' => vec!['7', 'u', 'h', 'g', 't', '6'],
			'u' => vec!['8', 'i', 'j', 'h', 'y', '7'],
			'i' => vec!['9', 'o', 'k', 'j', 'u', '8'],
			'o' => vec!['0', 'p', 'l', 'k', 'i', '9'],
			'p' => vec!['l', 'o', '0'],
			'a' => vec!['q', 'w', 's', 'z'],
			's' => vec!['e', 'd', 'x', 'z', 'a', 'w'],
			'd' => vec!['r', 'f', 'c', 'x', 's', 'e'],
			'f' => vec!['t', 'g', 'v', 'c', 'd', 'r'],
			'g' => vec!['y', 'h', 'b', 'v', 'f', 't'],
			'h' => vec!['u', 'j', 'n', 'b', 'g', 'y'],
			'j' => vec!['i', 'k', 'm', 'n', 'h', 'u'],
			'k' => vec!['o', 'l', 'm', 'j', 'i'],
			'l' => vec!['k', 'o', 'p'],
			'z' => vec!['a', 's', 'x'],
			'x' => vec!['z', 's', 'd', 'c'],
			'c' => vec!['x', 'd', 'f', 'v'],
			'v' => vec!['c', 'f', 'g', 'b'],
			'b' => vec!['v', 'g', 'h', 'n'],
			'n' => vec!['b', 'h', 'j', 'm'],
			'm' => vec!['n', 'j', 'k'],
		})
	}

	#[inline]
	pub fn qwertz() -> KeyboardLayout {
		KeyboardLayout::new(hashmap! {
			'1' => vec!['2', 'q'],
			'2' => vec!['3', 'w', 'q', '1'],
			'3' => vec!['4', 'e', 'w', '2'],
			'4' => vec!['5', 'r', 'e', '3'],
			'5' => vec!['6', 't', 'r', '4'],
			'6' => vec!['7', 'z', 't', '5'],
			'7' => vec!['8', 'u', 'z', '6'],
			'8' => vec!['9', 'i', 'u', '7'],
			'9' => vec!['0', 'o', 'i', '8'],
			'0' => vec!['p', 'o', '9'],
			'q' => vec!['1', '2', 'w', 'a'],
			'w' => vec!['3', 'e', 's', 'a', 'q', '2'],
			'e' => vec!['4', 'r', 'd', 's', 'w', '3'],
			'r' => vec!['5', 't', 'f', 'd', 'e', '4'],
			't' => vec!['6', 'z', 'g', 'f', 'r', '5'],
			'z' => vec!['7', 'u', 'h', 'g', 't', '6'],
			'u' => vec!['8', 'i', 'j', 'h', 'z', '7'],
			'i' => vec!['9', 'o', 'k', 'j', 'u', '8'],
			'o' => vec!['0', 'p', 'l', 'k', 'i', '9'],
			'p' => vec!['l', 'o', '0'],
			'a' => vec!['q', 'w', 's', 'y'],
			's' => vec!['e', 'd', 'x', 'y', 'a', 'w'],
			'd' => vec!['r', 'f', 'c', 'x', 's', 'e'],
			'f' => vec!['t', 'g', 'v', 'c', 'd', 'r'],
			'g' => vec!['z', 'h', 'b', 'v', 'f', 't'],
			'h' => vec!['u', 'j', 'n', 'b', 'g', 'z'],
			'j' => vec!['i', 'k', 'm', 'n', 'h', 'u'],
			'k' => vec!['o', 'l', 'm', 'j', 'i'],
			'l' => vec!['k', 'o', 'p'],
			'y' => vec!['a', 's', 'x'],
			'x' => vec!['y', 's', 'd', 'c'],
			'c' => vec!['x', 'd', 'f', 'v'],
			'v' => vec!['c', 'f', 'g', 'b'],
			'b' => vec!['v', 'g', 'h', 'n'],
			'n' => vec!['b', 'h', 'j', 'm'],
			'm' => vec!['n', 'j', 'k'],
		})
	}

	#[inline]
	pub fn azerty() -> KeyboardLayout {
		KeyboardLayout::new(hashmap! {
			'1' => vec!['2', 'a'],
			'2' => vec!['3', 'z', 'a', '1'],
			'3' => vec!['4', 'e', 'z', '2'],
			'4' => vec!['5', 'r', 'e', '3'],
			'5' => vec!['6', 't', 'r', '4'],
			'6' => vec!['7', 'y', 't', '5'],
			'7' => vec!['8', 'u', 'y', '6'],
			'8' => vec!['9', 'i', 'u', '7'],
			'9' => vec!['0', 'o', 'i', '8'],
			'0' => vec!['p', 'o', '9'],
			'a' => vec!['2', 'z', 'q', '1'],
			'z' => vec!['3', 'e', 's', 'q', 'a', '2'],
			'e' => vec!['4', 'r', 'd', 's', 'z', '3'],
			'r' => vec!['5', 't', 'f', 'd', 'e', '4'],
			't' => vec!['6', 'y', 'g', 'f', 'r', '5'],
			'y' => vec!['7', 'u', 'h', 'g', 't', '6'],
			'u' => vec!['8', 'i', 'j', 'h', 'y', '7'],
			'i' => vec!['9', 'o', 'k', 'j', 'u', '8'],
			'o' => vec!['0', 'p', 'l', 'k', 'i', '9'],
			'p' => vec!['l', 'o', '0', 'm'],
			'q' => vec!['z', 's', 'w', 'a'],
			's' => vec!['e', 'd', 'x', 'w', 'q', 'z'],
			'd' => vec!['r', 'f', 'c', 'x', 's', 'e'],
			'f' => vec!['t', 'g', 'v', 'c', 'd', 'r'],
			'g' => vec!['y', 'h', 'b', 'v', 'f', 't'],
			'h' => vec!['u', 'j', 'n', 'b', 'g', 'y'],
			'j' => vec!['i', 'k', 'n', 'h', 'u'],
			'k' => vec!['o', 'l', 'j', 'i'],
			'l' => vec!['k', 'o', 'p', 'm'],
			'm' => vec!['l', 'p'],
			'w' => vec!['s', 'x', 'q'],
			'x' => vec!['w', 's', 'd', 'c'],
			'c' => vec!['x', 'd', 'f', 'v'],
			'v' => vec!['c', 'f', 'g', 'b'],
			'b' => vec!['v', 'g', 'h', 'n'],
			'n' => vec!['b', 'h', 'j'],
		})
	}

	#[inline]
	pub fn neighbors(&self) -> &HashMap<char, Vec<char>> {
		&self.neighbors
	}
}

#[cfg(test)]
mod test {
	use super::NameFuzzer;
	use super::Typo;

	macro_rules! test_typos {
        ( from: $name:ident, to: $to:literal, expected: [ $( $expected:ident ),* ] ) => {
			let fuzzer = NameFuzzer::new(&$name);
            let result = fuzzer.fuzz($to);

            let expected = vec![ $(
                Typo::$expected($to.into()),
            )* ];

            assert_eq!(result, &expected[..]);
        };
    }

	const NAME: &'static str = "hello";

	#[test]
	fn fuzz_hello_to_hallo() {
		test_typos! { from: NAME, to: "hallo", expected: [bitsquatting, vowel_swap] }
	}

	#[test]
	fn fuzz_hello_to_helo() {
		test_typos! { from: NAME, to: "helo", expected: [omission] }
	}

	#[test]
	fn fuzz_hello_to_helllo() {
		test_typos! { from: NAME, to: "helllo", expected: [insertion, repetition] }
	}

	#[test]
	fn fuzz_hello_to_hrllo() {
		test_typos! { from: NAME, to: "hrllo", expected: [replacement] }
	}

	#[test]
	fn fuzz_hello_to_hlelo() {
		test_typos! { from: NAME, to: "hlelo", expected: [transposition] }
	}

	#[test]
	fn fuzz_hello_to_hellop() {
		test_typos! { from: NAME, to: "hellop", expected: [addition, insertion] }
	}

	#[test]
	fn fuzz_hello_to_h_ello() {
		test_typos! { from: NAME, to: "h-ello", expected: [hyphenation] }
	}

	#[test]
	fn fuzz_hello_to_hellos() {
		test_typos! { from: NAME, to: "hellos", expected: [addition, pluralization] }
	}
}
