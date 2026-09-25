//! Shared errors and error-handling helpers.

use std::fmt::Write as _;

/// A generic iterator over causes of an error.
pub struct ErrorSourceIter<'a> {
    /// The "current" error the iterator is handling; if empty, the source chain is done.
    current: Option<&'a dyn std::error::Error>,
}

impl<'a> Iterator for ErrorSourceIter<'a> {
    type Item = &'a dyn std::error::Error;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current;
        self.current = self.current.and_then(|e| e.source());
        current
    }
}

/// Extension trait adding `sources_iter` method to all `std::error::Error` types.
pub trait ErrorSourceIterator {
    // We'd prefer to call this "sources," but the standard library has a nightly-only API to do
    // this exact functionality, and it uses that name, so we get a warning about future
    // incompatibility if we use it ourselves. So we have to use this slightly worse name.
    fn sources_iter(&self) -> ErrorSourceIter<'_>;
}

impl<E: std::error::Error> ErrorSourceIterator for E {
    /// Provides an iterator over all sources of an error, _including_ the original error itself.
    ///
    /// To skip the error itself, call `skip(1)` on the iterator.
    fn sources_iter(&self) -> ErrorSourceIter<'_> {
        ErrorSourceIter {
            current: Some(self),
        }
    }
}

/// Format an error and every available source in its cause chain.
///
/// This preserves the top-level context while retaining the lower-level cause
/// that often identifies an external dependency or transport failure.
pub fn format_error_chain<E: std::error::Error>(error: &E) -> String {
    let mut formatted = String::new();

    for (index, source) in error.sources_iter().enumerate() {
        if index == 0 {
            formatted.push_str(&source.to_string());
        } else if index == 1 {
            let _ = write!(formatted, "\n\nCaused by:\n    {source}");
        } else {
            let _ = write!(formatted, "\n    {source}");
        }
    }

    formatted
}

#[cfg(test)]
mod tests {
    use super::format_error_chain;
    use std::{error::Error, fmt};

    #[derive(Debug)]
    struct OuterError(InnerError);

    impl fmt::Display for OuterError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("outer failure")
        }
    }

    impl Error for OuterError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    #[derive(Debug)]
    struct InnerError;

    impl fmt::Display for InnerError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("inner failure")
        }
    }

    impl Error for InnerError {}

    #[test]
    fn format_error_chain_includes_top_level_error_and_all_sources() {
        assert_eq!(
            format_error_chain(&OuterError(InnerError)),
            "outer failure\n\nCaused by:\n    inner failure"
        );
    }
}
