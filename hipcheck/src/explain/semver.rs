// SPDX-License-Identifier: Apache-2.0

use crate::{hc_error, Result};
use semver::{Comparator, Op};

pub(crate) fn explain_comparator(c: &Comparator) -> Result<String> {
	match c.op {
		Op::Exact => Ok(explain_exact(c)),
		Op::Greater => Ok(explain_greater(c)),
		Op::GreaterEq => Ok(explain_greater_eq(c)),
		Op::Less => Ok(explain_less(c)),
		Op::LessEq => Ok(explain_less_eq(c)),
		Op::Tilde => Ok(explain_tilde(c)),
		Op::Caret => explain_caret(c),
		Op::Wildcard => explain_wildcard(c),
		unknown => Err(hc_error!(
			"error: unknown comparator encountered: '{:?}'",
			unknown
		)),
	}
}

fn explain_exact(c: &Comparator) -> String {
	match (c.major, c.minor, c.patch) {
		(maj, Some(min), Some(pat)) => format!("={}.{}.{}", maj, min, pat),
		(maj, Some(min), None) => format!("={}.{}.0", maj, min),
		(maj, None, _) => format!("={}.0.0", maj),
	}
}

fn explain_greater(c: &Comparator) -> String {
	match (c.major, c.minor, c.patch) {
		(maj, Some(min), Some(pat)) => format!(">{}.{}.{}", maj, min, pat),
		(maj, Some(min), None) => format!(">={}.{}.0", maj, bump(min)),
		(maj, None, _) => format!(">={}.0.0", bump(maj)),
	}
}

fn explain_greater_eq(c: &Comparator) -> String {
	match (c.major, c.minor, c.patch) {
		(maj, Some(min), Some(pat)) => format!(">={}.{}.{}", maj, min, pat),
		(maj, Some(min), None) => format!(">={}.{}.0", maj, min),
		(maj, None, _) => format!(">={}.0.0", maj),
	}
}

fn explain_less(c: &Comparator) -> String {
	match (c.major, c.minor, c.patch) {
		(maj, Some(min), Some(pat)) => format!("<{}.{}.{}", maj, min, pat),
		(maj, Some(min), None) => format!("<{}.{}.0", maj, min),
		(maj, None, _) => format!("<{}.0.0", maj),
	}
}

fn explain_less_eq(c: &Comparator) -> String {
	match (c.major, c.minor, c.patch) {
		(maj, Some(min), Some(pat)) => format!("<={}.{}.{}", maj, min, pat),
		(maj, Some(min), None) => format!("<{}.{}.0", maj, bump(min)),
		(maj, None, _) => format!("<{}.0.0", bump(maj)),
	}
}

fn explain_tilde(c: &Comparator) -> String {
	match (c.major, c.minor, c.patch) {
		(maj, Some(min), Some(pat)) => {
			format!(">={}.{}.{}, <{}.{}.0", maj, min, pat, maj, bump(min))
		}
		(maj, Some(min), None) => format!(">={}.{}.0, <{}.{}.0", maj, min, maj, bump(min)),
		(maj, None, _) => format!(">={}.0.0, <{}.0.0", maj, bump(maj)),
	}
}

fn explain_caret(c: &Comparator) -> Result<String> {
	Ok(match (c.major, c.minor, c.patch) {
		(maj, Some(min), Some(pat)) if maj > 0 => {
			format!(">={}.{}.{}, <{}.0.0", maj, min, pat, bump(maj))
		}
		(maj, Some(min), Some(pat)) if maj == 0 && min > 0 => {
			format!(">=0.{}.{}, <0.{}.0", min, pat, bump(min))
		}
		(maj, Some(min), Some(pat)) if maj == 0 && min == 0 => format!("=0.0.{}", pat),

		(maj, Some(min), None) if maj > 0 => {
			format!(">={}.{}.0, <{}.0.0", maj, min, bump(maj))
		}
		(maj, Some(min), None) if maj == 0 && min > 0 => {
			format!(">=0.{}.0, <0.{}.0", min, bump(min))
		}
		(maj, Some(min), None) if maj == 0 && min == 0 => "=0.0.0".to_string(),
		(maj, None, _) => format!(">={}.0.0, <{}.0.0", maj, bump(maj)),
		(_maj, Some(_min), Some(..) | None) => {
			return Err(hc_error!("unrecognized bound {}", c));
		}
	})
}

fn explain_wildcard(c: &Comparator) -> Result<String> {
	match (c.major, c.minor, c.patch) {
		(_maj, Some(_min), Some(_pat)) => Err(hc_error!(
			"can't have a wildcard where every version part is specified"
		)),
		(maj, Some(min), None) => Ok(format!(">={}.{}.0, <{}.{}.0", maj, min, maj, bump(min))),
		(maj, None, _) => Ok(format!(">={}.0.0, <{}.0.0", maj, bump(maj))),
	}
}

fn bump(version_num: u64) -> u64 {
	version_num + 1
}
