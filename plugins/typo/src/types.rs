// SPDX-License-Identifier: Apache-2.0

use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::{
	cmp::Ordering,
	collections::HashMap,
	fmt::{self, Display},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpmDependencies {
	pub language: Lang,
	pub deps: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum Lang {
	JavaScript,
	Unknown,
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
