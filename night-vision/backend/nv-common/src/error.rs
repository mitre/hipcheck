//! Shared errors and error-handling helpers.

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
