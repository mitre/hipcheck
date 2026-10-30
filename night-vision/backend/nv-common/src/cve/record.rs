//! CVE Record Format parsing support.

use serde_json::Value;

/// A parsed CVE List record ready for source-shaped database storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCveRecord {
	/// CVE identifier duplicated from `cveMetadata.cveId`.
	pub cve_id: CveId,
	/// CVE Record Format version duplicated from `dataVersion`.
	pub record_format_version: CveRecordFormatVersion,
	/// Full upstream record, preserved unchanged for JSONB storage.
	pub record: Value,
}

/// Parse a CVE List JSON record and extract the fields duplicated in storage.
pub fn parse_cve_record(input: &[u8]) -> Result<ParsedCveRecord, CveRecordParseError> {
	let record: Value = serde_json::from_slice(input).map_err(CveRecordParseError::Json)?;
	let object = record
		.as_object()
		.ok_or(CveRecordParseError::RootNotObject)?;

	let record_format_version = object
		.get("dataVersion")
		.and_then(Value::as_str)
		.ok_or(CveRecordParseError::MissingDataVersion)
		.and_then(CveRecordFormatVersion::parse)?;

	let cve_id = object
		.get("cveMetadata")
		.and_then(Value::as_object)
		.and_then(|metadata| metadata.get("cveId"))
		.and_then(Value::as_str)
		.ok_or(CveRecordParseError::MissingCveId)
		.and_then(CveId::parse)?;

	Ok(ParsedCveRecord {
		cve_id,
		record_format_version,
		record,
	})
}

/// A validated CVE identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CveId(String);

impl CveId {
	/// Parse and validate a CVE identifier.
	pub fn parse(value: impl Into<String>) -> Result<Self, CveRecordParseError> {
		let value = value.into();

		if is_valid_cve_id(&value) {
			Ok(Self(value))
		} else {
			Err(CveRecordParseError::InvalidCveId(value))
		}
	}

	/// Return the CVE identifier as a string slice.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

/// A CVE Record Format version string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CveRecordFormatVersion(String);

impl CveRecordFormatVersion {
	/// Parse a CVE Record Format version.
	pub fn parse(value: impl Into<String>) -> Result<Self, CveRecordParseError> {
		let value = value.into();

		if value.is_empty() {
			Err(CveRecordParseError::InvalidRecordFormatVersion(value))
		} else {
			Ok(Self(value))
		}
	}

	/// Return the record format version as a string slice.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

fn is_valid_cve_id(value: &str) -> bool {
	let mut parts = value.split('-');
	let Some(prefix) = parts.next() else {
		return false;
	};
	let Some(year) = parts.next() else {
		return false;
	};
	let Some(sequence) = parts.next() else {
		return false;
	};

	parts.next().is_none()
		&& prefix == "CVE"
		&& year.len() == 4
		&& year.chars().all(|c| c.is_ascii_digit())
		&& sequence.len() >= 4
		&& sequence.chars().all(|c| c.is_ascii_digit())
}

/// Failure while parsing a CVE List record.
#[derive(Debug)]
pub enum CveRecordParseError {
	/// The record JSON could not be parsed.
	Json(serde_json::Error),
	/// The record root was not a JSON object.
	RootNotObject,
	/// The record was missing a string `dataVersion` field.
	MissingDataVersion,
	/// The record had an invalid `dataVersion` field.
	InvalidRecordFormatVersion(String),
	/// The record was missing a string `cveMetadata.cveId` field.
	MissingCveId,
	/// The record had an invalid CVE identifier.
	InvalidCveId(String),
}

impl std::fmt::Display for CveRecordParseError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Json(_) => write!(f, "failed to parse CVE record JSON"),
			Self::RootNotObject => write!(f, "CVE record root was not a JSON object"),
			Self::MissingDataVersion => write!(f, "CVE record is missing dataVersion"),
			Self::InvalidRecordFormatVersion(version) => {
				write!(f, "CVE record has invalid dataVersion: {version}")
			}
			Self::MissingCveId => write!(f, "CVE record is missing cveMetadata.cveId"),
			Self::InvalidCveId(cve_id) => write!(f, "CVE record has invalid CVE ID: {cve_id}"),
		}
	}
}

impl std::error::Error for CveRecordParseError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::Json(err) => Some(err),
			Self::RootNotObject
			| Self::MissingDataVersion
			| Self::InvalidRecordFormatVersion(_)
			| Self::MissingCveId
			| Self::InvalidCveId(_) => None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn parse_cve_record_extracts_database_fields_and_preserves_record() {
		let input = br#"{
            "dataType": "CVE_RECORD",
            "dataVersion": "5.2",
            "cveMetadata": {
                "cveId": "CVE-2026-12345",
                "state": "PUBLISHED"
            },
            "containers": {
                "cna": {
                    "providerMetadata": {
                        "orgId": "00000000-0000-0000-0000-000000000000"
                    }
                }
            },
            "unknownFutureField": {
                "kept": true
            }
        }"#;

		let parsed = parse_cve_record(input).expect("record should parse");

		assert_eq!(parsed.cve_id.as_str(), "CVE-2026-12345");
		assert_eq!(parsed.record_format_version.as_str(), "5.2");
		assert_eq!(
			parsed.record["unknownFutureField"],
			json!({
				"kept": true
			})
		);
	}

	#[test]
	fn parse_cve_record_rejects_malformed_json() {
		let err = parse_cve_record(b"{").expect_err("record should fail");

		assert!(matches!(err, CveRecordParseError::Json(_)));
	}

	#[test]
	fn parse_cve_record_rejects_non_object_roots() {
		let err = parse_cve_record(b"[]").expect_err("record should fail");

		assert!(matches!(err, CveRecordParseError::RootNotObject));
	}

	#[test]
	fn parse_cve_record_rejects_missing_data_version() {
		let err = parse_cve_record(
			br#"{
                "cveMetadata": {
                    "cveId": "CVE-2026-12345"
                }
            }"#,
		)
		.expect_err("record should fail");

		assert!(matches!(err, CveRecordParseError::MissingDataVersion));
	}

	#[test]
	fn parse_cve_record_rejects_empty_data_version() {
		let err = parse_cve_record(
			br#"{
                "dataVersion": "",
                "cveMetadata": {
                    "cveId": "CVE-2026-12345"
                }
            }"#,
		)
		.expect_err("record should fail");

		assert!(matches!(
			err,
			CveRecordParseError::InvalidRecordFormatVersion(version) if version.is_empty()
		));
	}

	#[test]
	fn parse_cve_record_rejects_missing_cve_metadata() {
		let err = parse_cve_record(
			br#"{
                "dataVersion": "5.2"
            }"#,
		)
		.expect_err("record should fail");

		assert!(matches!(err, CveRecordParseError::MissingCveId));
	}

	#[test]
	fn parse_cve_record_rejects_missing_cve_id() {
		let err = parse_cve_record(
			br#"{
                "dataVersion": "5.2",
                "cveMetadata": {}
            }"#,
		)
		.expect_err("record should fail");

		assert!(matches!(err, CveRecordParseError::MissingCveId));
	}

	#[test]
	fn parse_cve_record_rejects_invalid_cve_ids() {
		for cve_id in [
			"",
			"cve-2026-1234",
			"CVE-26-1234",
			"CVE-2026-123",
			"CVE-2026-abcd",
			"CVE-2026-1234-extra",
		] {
			let input = format!(
				r#"{{
                    "dataVersion": "5.2",
                    "cveMetadata": {{
                        "cveId": "{cve_id}"
                    }}
                }}"#
			);

			let err = parse_cve_record(input.as_bytes()).expect_err("record should fail");

			assert!(matches!(err, CveRecordParseError::InvalidCveId(_)));
		}
	}

	#[test]
	fn cve_id_accepts_four_or_more_sequence_digits() {
		for cve_id in ["CVE-2026-1234", "CVE-2026-12345"] {
			let parsed = CveId::parse(cve_id).expect("CVE ID should parse");

			assert_eq!(parsed.as_str(), cve_id);
		}
	}
}
