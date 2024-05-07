// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::path::PathBuf;

use crate::data::es_lint::data::ESLintMessage;
use crate::data::es_lint::data::ESLintReports;
use crate::data::es_lint::get_eslint_reports;
use crate::error::Result;
use crate::hc_error;

pub type CodeQualityReport = Vec<FileFindings>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFindings {
	pub file: PathBuf,
	pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
	pub weakness: Weakness,
	pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Weakness {
	/// Improper Neutralization of Directives in Dynamically Evaluated Code
	/// ('Eval Injection')
	/// https://cwe.mitre.org/data/definitions/95.html
	Cwe95,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
	pub line: u64,
	pub column: u64,
}

pub fn get_eslint_report(base_path: &Path, version: String) -> Result<CodeQualityReport> {
	let mut src_path: PathBuf = base_path.into();
	// Hardcoded assumption of where the source code will be located,
	// relative to the base directory of this Node module.
	src_path.push("src");
	let reports = get_eslint_reports(&src_path, version)?;
	translate_eslint_reports(reports)
}

fn translate_eslint_reports(reports: ESLintReports) -> Result<CodeQualityReport> {
	let mut out = Vec::with_capacity(reports.len());
	for report in reports {
		let file = report.file_path.into();

		let findings = translate_eslint_messages(report.messages)?;

		out.push(FileFindings { file, findings });
	}

	Ok(out)
}

fn translate_eslint_messages(messages: Vec<ESLintMessage>) -> Result<Vec<Finding>> {
	let mut findings = Vec::new();
	for message in messages {
		// It is considered a programmer error if this lookup fails,
		// since Hipcheck has control of which lints are requested from ESLint.
		// Detect unknown lints here and fail completely.
		// Unfortunately this will fail one-at-a-time, rather than erroring
		// with _all_ unknown lints in the report.
		let weakness = lookup_eslint_weakness(&message.rule_id)?;
		let span = Span {
			line: message.line,
			column: message.column,
		};
		findings.push(Finding { weakness, span });
	}
	Ok(findings)
}

fn lookup_eslint_weakness(rule_id: &str) -> Result<Weakness> {
	use Weakness::*;
	match rule_id {
		"no-eval" | "no-implied-eval" => Ok(Cwe95),
		_ => Err(hc_error!("Unknown ESLint rule id '{}'", rule_id)),
	}
}
