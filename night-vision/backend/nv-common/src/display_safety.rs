//! Display-safe projections of untrusted assessment evidence.
//!
//! These helpers are intentionally for API and UI output. They do not replace
//! the bounded raw evidence retained for maintainer investigation.

use crate::config::redact_url_authentication;
use std::fmt::Write as _;
use url::Url;

pub const MAX_DISPLAY_SUMMARY_BYTES: usize = 1_024;
pub const MAX_DISPLAY_DIAGNOSTIC_BYTES: usize = 8 * 1_024;
pub const MAX_DISPLAY_RAW_JSON_BYTES: usize = 16 * 1_024;
pub const MAX_DISPLAY_URL_BYTES: usize = 2_048;

const TRUNCATION_MARKER: &str = "… [truncated]";

/// Return bounded, single-line text suitable for a normal API response.
pub fn summary_text(value: &str) -> String {
    display_text(value, MAX_DISPLAY_SUMMARY_BYTES, false)
}

/// Return bounded diagnostic text. Newlines are preserved; other controls are
/// escaped so terminal and browser output cannot be manipulated.
pub fn diagnostic_text(value: &str) -> String {
    display_text(value, MAX_DISPLAY_DIAGNOSTIC_BYTES, true)
}

/// Return a bounded, inert preview of raw JSON for an explicitly requested
/// maintainer/debug response.
pub fn raw_json_preview(value: &str) -> String {
    display_text(value, MAX_DISPLAY_RAW_JSON_BYTES, true)
}

/// Return a bounded URL label with embedded credentials redacted. Invalid URLs
/// remain inert display text rather than being repaired or made clickable.
pub fn url_label(value: &str) -> String {
    match Url::parse(value) {
        Ok(url) => display_text(
            redact_url_authentication(&url).as_str(),
            MAX_DISPLAY_URL_BYTES,
            false,
        ),
        Err(_) => summary_text(&redact_unparsed_url_authentication(value)),
    }
}

/// Return a URL that may be rendered as an external link.
///
/// Only absolute HTTPS URLs without credentials and within the display limit
/// are linkable. Callers must render every other value as inert text.
pub fn vetted_https_url(value: &str) -> Option<String> {
    if value.len() > MAX_DISPLAY_URL_BYTES {
        return None;
    }
    let url = Url::parse(value).ok()?;
    (url.scheme() == "https"
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.into())
}

fn display_text(value: &str, max_bytes: usize, preserve_newlines: bool) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let mut displayed = String::new();
    let mut displayed_part_lengths = Vec::new();
    for character in normalized.chars() {
        let escaped = escaped_character(character, preserve_newlines);
        let displayed_len = displayed
            .len()
            .checked_add(escaped.len())
            .expect("display text length cannot overflow usize");
        if displayed_len > max_bytes {
            let marker_limit = max_bytes
                .checked_sub(TRUNCATION_MARKER.len())
                .expect("display text limit must fit its truncation marker");
            while displayed.len() > marker_limit {
                let part_len = displayed_part_lengths
                    .pop()
                    .expect("displayed text must contain a complete part");
                let new_len = displayed
                    .len()
                    .checked_sub(part_len)
                    .expect("complete display part cannot exceed text length");
                displayed.truncate(new_len);
            }
            displayed.push_str(TRUNCATION_MARKER);
            return displayed;
        }
        displayed.push_str(&escaped);
        displayed_part_lengths.push(escaped.len());
    }
    displayed
}

fn escaped_character(character: char, preserve_newlines: bool) -> String {
    if character == '\n' && preserve_newlines {
        return "\n".to_owned();
    }
    match character {
        '\n' => "\\n".to_owned(),
        '\t' => "\\t".to_owned(),
        character if character.is_control() || is_bidirectional_format_control(character) => {
            let mut escaped = String::new();
            write!(escaped, "\\u{{{:04x}}}", character as u32)
                .expect("writing to a string cannot fail");
            escaped
        }
        _ => character.to_string(),
    }
}

fn is_bidirectional_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn redact_unparsed_url_authentication(value: &str) -> String {
    let Some((prefix, remainder)) = value.split_once("://") else {
        return value.to_owned();
    };
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let (authority, suffix) = remainder.split_at(authority_end);
    let Some((_, host)) = authority.rsplit_once('@') else {
        return value.to_owned();
    };
    format!("{prefix}://<redacted>@{host}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_DISPLAY_SUMMARY_BYTES, diagnostic_text, raw_json_preview, summary_text, url_label,
        vetted_https_url,
    };

    #[test]
    fn summary_escapes_controls_and_bidi_formatting() {
        assert_eq!(
            summary_text("line\n\t\u{001b}[31m\u{202e}text"),
            "line\\n\\t\\u{001b}[31m\\u{202e}text"
        );
    }

    #[test]
    fn diagnostics_preserve_normalized_newlines_but_escape_other_controls() {
        assert_eq!(
            diagnostic_text("one\r\ntwo\rthree\t\u{0000}"),
            "one\ntwo\nthree\\t\\u{0000}"
        );
    }

    #[test]
    fn display_text_truncates_without_splitting_utf8() {
        let value = format!("{}é", "x".repeat(MAX_DISPLAY_SUMMARY_BYTES));
        let displayed = summary_text(&value);
        assert!(displayed.ends_with("… [truncated]"));
        assert!(displayed.len() <= MAX_DISPLAY_SUMMARY_BYTES);
        assert!(displayed.is_char_boundary(displayed.len()));
    }

    #[test]
    fn display_text_keeps_values_that_exactly_fit_the_limit() {
        let value = "x".repeat(MAX_DISPLAY_SUMMARY_BYTES);

        assert_eq!(summary_text(&value), value);
    }

    #[test]
    fn display_text_adds_a_marker_only_when_content_exceeds_the_limit() {
        let value = "x".repeat(MAX_DISPLAY_SUMMARY_BYTES + 1);
        let displayed = summary_text(&value);

        assert!(displayed.ends_with("… [truncated]"));
        assert_eq!(displayed.len(), MAX_DISPLAY_SUMMARY_BYTES);
    }

    #[test]
    fn url_display_redacts_credentials_and_only_vets_https_links() {
        assert_eq!(
            url_label("https://user:password@example.com/path"),
            "https://%3Credacted%3E:%3Credacted%3E@example.com/path"
        );
        assert_eq!(
            vetted_https_url("https://example.com/path"),
            Some("https://example.com/path".to_owned())
        );
        assert_eq!(vetted_https_url("javascript:alert(1)"), None);
        assert_eq!(vetted_https_url("https://user:password@example.com"), None);
        assert_eq!(vetted_https_url("http://example.com"), None);
        assert_eq!(
            url_label("https://user:password@exa mple.com/path"),
            "https://<redacted>@exa mple.com/path"
        );
    }

    #[test]
    fn raw_json_preview_keeps_script_like_content_inert_text() {
        assert_eq!(
            raw_json_preview(r#"{"script":"<script>alert(1)</script>"}"#),
            r#"{"script":"<script>alert(1)</script>"}"#
        );
    }
}
