//! A simple key-value configuration format inspired by [Ghostty].
//!
//! ## The Spookey Format
//!
//! "Spookey" is a *very simple* key-value configuration format. It's name is a play on "Ghostty"
//! and the word "key" in "key-value format."
//!
//! A Spookey document looks like this:
//!
//! ```txt
//! # Hello, this is a comment. Blank lines are allowed.
//!
//! this-is-a-key = "this is a value"
//! ```
//!
//! Keys and values can be any Unicode character, though normatively we prefer ASCII letters and
//! numbers separated by hyphens for keys. Keys are case-sensitive.
//!
//! If values are quoted-wrapped, the quotes are stripped during parsing.
//!
//! We return all keys as strings, and leave any further parsing up to the caller.
//!
//! ## Configuration
//!
//! [`parse`] takes a [`ParseConfig`] that indicates the names of required keys and optional keys.
//! These keys must _exactly_ match the keys present in a Spookey document. Keys are case-sensitive.
//!
//! ## Error Handling
//!
//! [`parse`] distinguishes [`Error`]s from [`Warning`]s. `Error`s are problems during
//! parsing that mean the document is invalid, while `Warning`s are problems during parsing that the
//! caller should be alerted to, but that do not leave us unable to parse the document.
//!
//! Currently, `Error`s include I/O problems reading the document or the failure of a document to
//! set all required keys.
//!
//! All other problems, such as duplicate keys, lines that aren't in key-value format, lines that
//! have a value but no key, or unexpected keys are `Warning`s.
//!
//! Warnings are reported in the `warnings` field on [`ParseResult`].
//!
//! ## Dependencies
//!
//! This library does not have any dependencies, and instead uses only the standard library.
//!
//! ## Panic Safety
//!
//! `parse` should not panic for any reason.
//!
//! [Ghostty]: https://ghostty.org/docs/config

use std::{
    collections::{HashMap, HashSet},
    error::Error as StdError,
    fmt::{Display, Formatter, Result as FmtResult},
    io::{BufRead as _, BufReader, Read},
    ops::Not as _,
};

/// Configuration for the Spookey parser.
pub struct ParseConfig {
    /// Fields that are required to be present.
    pub required_keys: Vec<&'static str>,

    /// Fields that may or may not be present.
    pub optional_keys: Vec<&'static str>,
}

/// The result of parsing a Spookey file.
#[derive(Debug)]
pub struct ParseResult {
    /// Required fields.
    pub required_keys: HashMap<&'static str, String>,

    /// Optiona fields.
    pub optional_keys: HashMap<&'static str, Option<String>>,

    /// Any warnings encountered during parsing.
    pub warnings: Vec<Warning>,
}

impl ParseResult {
    fn start(optional_keys: &[&'static str]) -> Self {
        Self {
            required_keys: Default::default(),
            // Since they're optional, initialize them to None.
            optional_keys: optional_keys.iter().map(|f| (*f, None)).collect(),
            warnings: Default::default(),
        }
    }
}

/// Parse a Spookey document based on the configuration.
///
/// ## Usage
///
/// ```
/// # use spookey::{parse, ParseConfig};
/// # use std::{io::BufReader, fs::File};
/// let config = ParseConfig {
///     required_keys: vec!["some-key"],
///     optional_keys: vec!["another-key"],
/// };
///
/// let reader = BufReader::new(File::open("test/example.spookey")?);
///
/// let results = parse(config, reader)?;
///
/// println!("results: {:#?}", results);
/// # Ok::<(), spookey::Error>(())
/// ```
///
pub fn parse<R>(config: ParseConfig, buf_reader: BufReader<R>) -> Result<ParseResult, Error>
where
    R: Read,
{
    let mut result = ParseResult::start(&config.optional_keys);

    for (line_number, line) in buf_reader.lines().enumerate() {
        // I/O errors go up!
        let line = line?;

        // Make line_number 1-indexed, trim whitespace from line.
        let line_number = line_number + 1;
        let line = line.trim();

        // Skip empty lines and comments.
        if line.is_empty() || line.starts_with("#") {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            result.warnings.push(Warning {
                line_number,
                kind: WarningKind::line_not_key_value_format(line),
            });

            continue;
        };

        let key = key.trim().to_string();

        // If the value is quoted-wrapped, then remove the quotes.
        let value = value
            .trim()
            .trim_start_matches('"')
            .trim_end_matches('"')
            .to_string();

        if key.is_empty() {
            result.warnings.push(Warning {
                line_number,
                kind: WarningKind::missing_key(&value),
            });

            continue;
        }

        // We permit empty values and treat them as the key/value pair not being present at all
        // (so the default value is used). This matches the behavior in Ghostty, and lets the
        // configuration file have empty values for all the keys so we can clearly document
        // what the keys are within the config file itself, without setting them or commenting
        // them out.
        if value.is_empty() {
            continue;
        }

        // Update `required_keys` if key matches, warn if already set, but take the second value.
        if let Some(key) = config.required_keys.iter().find(|k| **k == key) {
            if let Some(prior_value) = result.required_keys.insert(key, value.clone()) {
                result.warnings.push(Warning {
                    line_number,
                    kind: WarningKind::key_set_more_than_once(
                        key,
                        &prior_value,
                        &value,
                        line_number,
                    ),
                })
            }

            continue;
        }

        // Update `required_keys` if key matches, warn if already set, but take the second value.
        if let Some(key) = config.optional_keys.iter().find(|k| **k == key) {
            if let Some(prior_value) = result.optional_keys.insert(key, Some(value.clone()))
                && prior_value.is_some()
            {
                result.warnings.push(Warning {
                    line_number,
                    kind: WarningKind::key_set_more_than_once(
                        key,
                        // PANIC SAFETY: We've already tested if `prior_value` is None.
                        &prior_value.unwrap(),
                        &value,
                        line_number,
                    ),
                })
            }

            continue;
        }

        result.warnings.push(Warning {
            line_number,
            kind: WarningKind::unexpected_key(&key),
        })
    }

    if let Some(missing) = missing_required_keys(&config, &result) {
        return Err(Error::missing_required_keys(&missing));
    }

    Ok(result)
}

/// Identify any required fields that are missing.
fn missing_required_keys(config: &ParseConfig, result: &ParseResult) -> Option<Vec<&'static str>> {
    let expected = config.required_keys.iter().collect::<HashSet<_>>();
    let found = result.required_keys.keys().collect::<HashSet<_>>();

    let missing = HashSet::difference(&expected, &found)
        .map(|k| **k)
        .collect::<Vec<_>>();

    if missing.is_empty().not() {
        return Some(missing);
    }

    None
}

/// A fatal error encountered during parsing.
#[derive(Debug)]
pub enum Error {
    /// Required fields are missing.
    MissingRequiredFields(Box<[&'static str]>),

    /// There was an I/O issue.
    Io(std::io::Error),
}

impl Error {
    pub(crate) fn missing_required_keys(fields: &[&'static str]) -> Self {
        Error::MissingRequiredFields(fields.to_owned().into_boxed_slice())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Error::MissingRequiredFields(items) => {
                write!(f, "missing required fields: {}", items.join(", "))
            }
            Error::Io(error) => error.fmt(f),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::MissingRequiredFields(_) => None,
            Error::Io(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Io(value)
    }
}

/// A warning arising during parsing.
///
/// These are non-fatal errors that indicate a mistake in the file, but one
/// which doesn't stop us from parsing the rest of the file.
///
/// Since `Warning` implements the `Error` trait, you can still treat it as
/// a hard error if you want.
#[derive(Debug, Clone)]
pub struct Warning {
    /// The line number of the warning (1-indexed).
    pub line_number: usize,

    /// The kind of issue encountered.
    pub kind: WarningKind,
}

impl Display for Warning {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "line {}: {}", self.line_number, self.kind)
    }
}

impl StdError for Warning {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self.kind {
            WarningKind::MissingKey { .. }
            | WarningKind::LineNotKeyValueFormat { .. }
            | WarningKind::KeySetMoreThanOnce { .. }
            | WarningKind::UnexpectedKey { .. } => None,
        }
    }
}

/// The kind of issue encountered in the document.
#[derive(Debug, Clone)]
pub enum WarningKind {
    /// Line is missing a key.
    MissingKey { value: Box<str> },
    /// Line is not in key-value format (no `=` found).
    LineNotKeyValueFormat { line: Box<str> },
    /// A key has been set more than once.
    KeySetMoreThanOnce {
        key: Box<str>,
        prior_value: Box<str>,
        new_value: Box<str>,
        second_line_number: usize,
    },
    /// A key is unexpected (it's not listed in `config.required_keys` or `config.optional_keys`).
    UnexpectedKey { key: Box<str> },
}

impl WarningKind {
    pub(crate) fn missing_key(value: &str) -> Self {
        Self::MissingKey {
            value: value.to_string().into_boxed_str(),
        }
    }

    pub(crate) fn line_not_key_value_format(line: &str) -> Self {
        Self::LineNotKeyValueFormat {
            line: line.to_string().into_boxed_str(),
        }
    }

    pub(crate) fn key_set_more_than_once(
        key: &str,
        prior_value: &str,
        new_value: &str,
        second_line_number: usize,
    ) -> Self {
        Self::KeySetMoreThanOnce {
            key: key.to_string().into_boxed_str(),
            prior_value: prior_value.to_string().into_boxed_str(),
            new_value: new_value.to_string().into_boxed_str(),
            second_line_number,
        }
    }

    pub(crate) fn unexpected_key(key: &str) -> Self {
        Self::UnexpectedKey {
            key: key.to_string().into_boxed_str(),
        }
    }
}

impl Display for WarningKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            WarningKind::MissingKey { value } => write!(f, "missing key; value is '{}'", value),
            WarningKind::LineNotKeyValueFormat { line } => {
                write!(f, "line is not key-value format: '{}'", line)
            }
            WarningKind::KeySetMoreThanOnce {
                key,
                prior_value,
                new_value,
                second_line_number,
            } => write!(
                f,
                "key '{}' set more than once; was previously '{}', overriden by '{}' on line {}",
                key, prior_value, new_value, second_line_number
            ),
            WarningKind::UnexpectedKey { key } => write!(f, "unexpected key '{}'", key),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error as _,
        io::{self, BufReader, Cursor, Read},
    };

    use super::*;

    fn parse_str(config: ParseConfig, document: &str) -> Result<ParseResult, Error> {
        parse(config, BufReader::new(Cursor::new(document)))
    }

    #[test]
    fn missing_required_keys_returns_missing_required_fields_error() {
        let error = parse_str(
            ParseConfig {
                required_keys: vec!["host", "port"],
                optional_keys: vec![],
            },
            "host = localhost\n",
        )
        .expect_err("missing required key should be a fatal parse error");

        let Error::MissingRequiredFields(fields) = error else {
            panic!("expected MissingRequiredFields, got {error:?}");
        };

        assert_eq!(&*fields, &["port"]);
    }

    #[test]
    fn missing_required_keys_ignores_empty_values() {
        let error = parse_str(
            ParseConfig {
                required_keys: vec!["host"],
                optional_keys: vec![],
            },
            "host = \n",
        )
        .expect_err("empty required key value should count as missing");

        let Error::MissingRequiredFields(fields) = error else {
            panic!("expected MissingRequiredFields, got {error:?}");
        };

        assert_eq!(&*fields, &["host"]);
    }

    #[test]
    fn io_errors_are_returned_as_io_errors_with_source() {
        #[derive(Debug)]
        struct BrokenReader;

        impl Read for BrokenReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            }
        }

        let error = parse(
            ParseConfig {
                required_keys: vec![],
                optional_keys: vec![],
            },
            BufReader::new(BrokenReader),
        )
        .expect_err("reader I/O failure should be a fatal parse error");

        let Error::Io(io_error) = &error else {
            panic!("expected Io, got {error:?}");
        };

        assert_eq!(io_error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(io_error.to_string(), "denied");
        assert!(error.source().is_some());
    }

    #[test]
    fn duplicate_required_key_returns_warning_and_uses_new_value() {
        let result = parse_str(
            ParseConfig {
                required_keys: vec!["host"],
                optional_keys: vec![],
            },
            "host = old\nhost = new\n",
        )
        .expect("duplicate key is a warning, not a fatal parse error");

        assert_eq!(
            result.required_keys.get("host").map(String::as_str),
            Some("new")
        );
        assert_eq!(result.warnings.len(), 1);

        let warning = &result.warnings[0];
        assert_eq!(warning.line_number, 2);
        let WarningKind::KeySetMoreThanOnce {
            key,
            prior_value,
            new_value,
            second_line_number,
        } = &warning.kind
        else {
            panic!("expected KeySetMoreThanOnce, got {warning:?}");
        };

        assert_eq!(&**key, "host");
        assert_eq!(&**prior_value, "old");
        assert_eq!(&**new_value, "new");
        assert_eq!(*second_line_number, 2);
    }

    #[test]
    fn duplicate_optional_key_returns_warning_and_uses_new_value() {
        let result = parse_str(
            ParseConfig {
                required_keys: vec![],
                optional_keys: vec!["theme"],
            },
            "theme = light\ntheme = dark\n",
        )
        .expect("duplicate optional key is a warning, not a fatal parse error");

        assert_eq!(
            result
                .optional_keys
                .get("theme")
                .and_then(|value| value.as_deref()),
            Some("dark")
        );
        assert_eq!(result.warnings.len(), 1);

        let WarningKind::KeySetMoreThanOnce {
            key,
            prior_value,
            new_value,
            second_line_number,
        } = &result.warnings[0].kind
        else {
            panic!("expected KeySetMoreThanOnce, got {:?}", result.warnings[0]);
        };

        assert_eq!(&**key, "theme");
        assert_eq!(&**prior_value, "light");
        assert_eq!(&**new_value, "dark");
        assert_eq!(*second_line_number, 2);
    }

    #[test]
    fn invalid_lines_are_returned_as_warnings() {
        let result = parse_str(
            ParseConfig {
                required_keys: vec![],
                optional_keys: vec!["known"],
            },
            "not key value\n= value\nunknown = value\nknown = set\n",
        )
        .expect("invalid lines should be warnings when no required keys are missing");

        assert_eq!(result.warnings.len(), 3);

        let WarningKind::LineNotKeyValueFormat { line } = &result.warnings[0].kind else {
            panic!(
                "expected LineNotKeyValueFormat, got {:?}",
                result.warnings[0]
            );
        };
        assert_eq!(result.warnings[0].line_number, 1);
        assert_eq!(&**line, "not key value");

        let WarningKind::MissingKey { value } = &result.warnings[1].kind else {
            panic!("expected MissingKey, got {:?}", result.warnings[1]);
        };
        assert_eq!(result.warnings[1].line_number, 2);
        assert_eq!(&**value, "value");

        let WarningKind::UnexpectedKey { key } = &result.warnings[2].kind else {
            panic!("expected UnexpectedKey, got {:?}", result.warnings[2]);
        };
        assert_eq!(result.warnings[2].line_number, 3);
        assert_eq!(&**key, "unknown");
    }
}
