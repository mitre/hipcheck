use crate::{
	git::{Commit, GitIdentity},
	github::GitHubCollaborator,
};

/// An "enriched contributor" combines basic Git data with any "extra" data
/// we could identify from an associated GitHub account.
#[derive(Debug)]
pub struct EnrichedContributor {
	/// The contributor's Git data (name and email)
	pub git: GitIdentity,
	/// The contributor's GitHub data (if present)
	pub github: Option<GitHubCollaborator>,
	/// The commits they worked on as author or committer.
	pub commits: Vec<Commit>,
}
