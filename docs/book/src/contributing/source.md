
# Source

"Source" is Hipcheck's term for a repository to be analyzed. Sources are
"resolved" from a string specifying either a local directory or a remote
URL.

If a local directory is provided, Hipcheck will attempt to identify
if the current branch pointed to by `HEAD` in the repository is tracking
some remote branch, and if it is, what the URL of that remote is. This
automated discovery enables review analysis to be performed on local
clones of GitHub repositories.

If a remote repository is provided, Hipcheck attempts to identify what
hostname is present. If GitHub, then review analysis against the GitHub
API is enabled. If another host, review analysis will not be possible.
