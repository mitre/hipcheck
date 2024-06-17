// SPDX-License-Identifier: Apache-2.0

/// ESLint's JSON output is demonstrated here:
/// https://eslint.org/docs/user-guide/formatters/#json
///
/// This parser has been tested with the output from ESLint v7.31.0
use serde::Deserialize;

// ESLint's JSON output is demonstrated here:
// https://eslint.org/docs/user-guide/formatters/#json
//
// This parser has been tested with the output from ESLint v7.31.0

pub type ESLintReports = Vec<ESLintReport>;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ESLintReport {
	#[serde(rename = "filePath")]
	pub file_path: String,
	pub messages: Vec<ESLintMessage>,
	#[serde(rename = "errorCount")]
	pub error_count: u64,
	#[serde(rename = "warningCount")]
	pub warning_count: u64,
	#[serde(rename = "fixableErrorCount")]
	pub fixable_error_count: u64,
	#[serde(rename = "fixableWarningCount")]
	pub fixable_warning_count: u64,
	pub source: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ESLintMessage {
	#[serde(rename = "ruleId")]
	pub rule_id: String,
	pub severity: u64,
	pub message: String,
	pub line: u64,
	pub column: u64,
	#[serde(rename = "endLine")]
	pub end_line: u64,
	#[serde(rename = "endColumn")]
	pub end_column: u64,
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn parse_message() {
		let json = r#"{
			"ruleId": "no-eval",
			"severity": 2,
			"message": "eval can be harmful.",
			"line": 3,
			"column": 5,
			"nodeType": "CallExpression",
			"messageId": "unexpected",
			"endLine": 3,
			"endColumn": 9
		}"#;

		let msg: ESLintMessage = serde_json::from_str(json).unwrap();
		assert_eq!(msg.rule_id, "no-eval");
		assert_eq!(msg.severity, 2);
	}

	#[test]
	fn parse_report() {
		let json = r#"{
			"filePath": "src/hello.js",
			"messages": [],
			"errorCount": 0,
			"warningCount": 0,
			"fixableErrorCount": 0,
			"fixableWarningCount": 0,
			"usedDeprecatedRules": []
		}"#;

		let report: ESLintReport = serde_json::from_str(json).unwrap();
		assert_eq!(report.file_path, "src/hello.js");
		assert_eq!(report.error_count, 0);
	}

	#[test]
	fn parse_empty_reports() {
		let json = r#"[]"#;

		let reports: ESLintReports = serde_json::from_str(json).unwrap();
		assert_eq!(reports.len(), 0);
	}

	#[test]
	fn parse_reports() {
		let json = r#"[
			{
				"filePath": "no_issues.js",
				"messages": [],
				"errorCount": 0,
				"warningCount": 0,
				"fixableErrorCount": 0,
				"fixableWarningCount": 0,
				"usedDeprecatedRules": []
			},
			{
				"filePath": "problems.js",
				"messages": [
					{
						"ruleId": "no-eval",
						"severity": 2,
						"message": "eval can be harmful.",
						"line": 3,
						"column": 5,
						"nodeType": "CallExpression",
						"messageId": "unexpected",
						"endLine": 3,
						"endColumn": 9
					},
					{
						"ruleId": "no-implied-eval",
						"severity": 2,
						"message": "Implied eval. Consider passing a function instead of a string.",
						"line": 4,
						"column": 5,
						"nodeType": "CallExpression",
						"messageId": "impliedEval",
						"endLine": 4,
						"endColumn": 26
					}
				],
				"errorCount": 2,
				"warningCount": 0,
				"fixableErrorCount": 0,
				"fixableWarningCount": 0,
				"source": "function hello() {\n    console.log('hello');\n    eval('evil');\n    setTimeout('evil', 0);\n}\n",
				"usedDeprecatedRules": []
			}
		]"#;

		let reports: ESLintReports = serde_json::from_str(json).unwrap();
		assert_eq!(reports.len(), 2);

		assert_eq!(reports[0].file_path, "no_issues.js");
		assert_eq!(reports[0].messages.len(), 0);
		assert_eq!(reports[0].source, None);

		assert_eq!(reports[1].file_path, "problems.js");
		assert_eq!(reports[1].messages.len(), 2);
		assert!(reports[1].source.is_some());
	}
}
