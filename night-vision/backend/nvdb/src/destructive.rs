#[must_use]
pub struct DestructiveOperationToken {
    _private: (),
}

impl DestructiveOperationToken {
    pub fn new(matches: &clap::ArgMatches) -> Self {
        assert!(
            matches.get_flag("destructive"),
            "destructive operation token requires --destructive"
        );
        Self { _private: () }
    }
}
