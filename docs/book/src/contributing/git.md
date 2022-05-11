
# Git

Hipcheck relies on Git as a key source of data for analysis. Hipcheck
uses the `git` CLI to collect information about the repository to be
analyzed, and if necessary to create a local clone if the repository
is remote. It also uses `git log` and `git diff` to collect commit
contributor information (for affiliation and identity analyses) and
diff information (for entropy and churn analyses)

Hipcheck interacts with Git through the CLI exclusively, rather than
attempting to parse Git data itself.

Interaction with this CLI is mediated by the `GitCommand` type in
Hipcheck, which ensures the `git` executable is found in a manner
which works across platforms, and then results are processed and
packaged in a manner that enables ease of use and clear error
reporting.
