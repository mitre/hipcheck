// SPDX-License-Identifier: Apache-2.0

use crate::{
	types::{Homoglyphs, KeyboardLayout, NpmDependencies, Typo},
	util::fs as file,
};
use anyhow::{Context as _, Result};
use hipcheck_kdl::kdl::KdlNode;
use hipcheck_kdl::ParseKdlNode;
use std::{collections::HashMap, path::Path};

#[derive(Debug)]
pub struct TypoFile {
	languages: Vec<Language>,
}

impl ParseKdlNode for TypoFile {
	fn kdl_key() -> &'static str {
		"languages"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let mut languages = Vec::new();
		for node in node.children()?.nodes() {
			languages.push(Language::parse_node(node)?);
		}
		Some(Self { languages })
	}
}

impl TypoFile {
	pub fn load_from(typo_path: &Path) -> Result<TypoFile> {
		file::exists(typo_path).context("typo file does not exist")?;
		let typo_file = file::read_kdl(typo_path)
			.with_context(|| format!("failed to read typo file at path {:?}", typo_path))?;

		Ok(typo_file)
	}
}

#[derive(Debug)]
struct Language {
	language: LanguageType,
	packages: Vec<String>,
}

impl ParseKdlNode for Language {
	fn kdl_key() -> &'static str {
		""
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		let language = match node.name().to_string().as_str() {
			"javascript" => LanguageType::Javascript,
			_ => return None,
		};
		let mut packages = Vec::new();
		for package in node.entries() {
			packages.push(package.value().as_string()?.to_string());
		}
		Some(Self { language, packages })
	}
}

#[derive(Debug, PartialEq)]
enum LanguageType {
	Javascript,
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

pub(crate) fn typos_for_javascript(
	typo_file: &TypoFile,
	dependencies: NpmDependencies,
) -> Result<Vec<String>> {
	let mut typos = Vec::new();
	let mut javascript_packages = &Vec::new();
	for language in &typo_file.languages {
		if LanguageType::Javascript == language.language {
			javascript_packages = &language.packages;
		}
	}
	for legit_name in javascript_packages {
		let fuzzer = NameFuzzer::new(legit_name);

		// Add a dependency name to the list of typos if the list of possible typos for that name is non-empty
		for dependency in &dependencies.deps {
			if !fuzzer.fuzz(dependency).is_empty() {
				typos.push(dependency.to_string());
			}
		}
	}

	Ok(typos)
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
