use chrono::{DateTime, FixedOffset, Local};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TimeDisplay {
    #[default]
    Stored,
    Local,
}

impl TimeDisplay {
    pub fn from_matches(matches: &clap::ArgMatches) -> Self {
        if matches.get_flag("local-time") {
            Self::Local
        } else {
            Self::Stored
        }
    }

    pub fn format(self, timestamp: &DateTime<FixedOffset>) -> String {
        match self {
            Self::Stored => timestamp.to_string(),
            Self::Local => timestamp.with_timezone(&Local).to_string(),
        }
    }

    pub fn format_optional(self, timestamp: Option<&DateTime<FixedOffset>>) -> String {
        timestamp.map_or_else(|| "<none>".to_owned(), |timestamp| self.format(timestamp))
    }
}

#[cfg(test)]
mod tests {
    use super::TimeDisplay;
    use chrono::{DateTime, Local};

    #[test]
    fn stored_display_preserves_the_stored_offset() {
        let timestamp = DateTime::parse_from_rfc3339("2026-08-27T21:00:00+05:30").unwrap();

        assert_eq!(
            TimeDisplay::Stored.format(&timestamp),
            timestamp.to_string()
        );
    }

    #[test]
    fn local_display_converts_the_instant_to_the_host_timezone() {
        let timestamp = DateTime::parse_from_rfc3339("2026-08-27T21:00:00+00:00").unwrap();
        let displayed = TimeDisplay::Local.format(&timestamp);
        let parsed = DateTime::parse_from_str(&displayed, "%Y-%m-%d %H:%M:%S %:z").unwrap();

        assert_eq!(parsed, timestamp);
        assert_eq!(displayed, timestamp.with_timezone(&Local).to_string());
        assert!(displayed.ends_with(&timestamp.with_timezone(&Local).format("%:z").to_string()));
    }

    #[test]
    fn optional_display_uses_the_none_marker() {
        assert_eq!(TimeDisplay::Stored.format_optional(None), "<none>");
    }
}
