
# Types of Analysis

Hipcheck includes a variety of analysis forms, which each may work on different
sources of data, and have different meanings to understand when configuring
them or interpreting their outputs. This page lists Hipcheck's analyses with
the names they are given in the configuration file, what their data source
is, the details of the analysis performed, what its current limitations are,
and how the results of that analysis are thresholded based on the
configuration.

## Hipcheck Modes

Currently Hipcheck runs in one of two modes:

* `repo` mode analyzes an entire repository.
* `request` mode analyzes a single pull/merge request, whether merged or not.

The analyses Hipcheck uses depend on which mode Hipcheck is running in.

## `repo` Analyses

### Activity

* Configuration name: `analysis.practices.activity`
* Data source: Git (committed date of most recent commit to `HEAD` branch)

Activity analysis looks at the date of the most recent commit to the branch
pointed to by `HEAD` in the repository. In the case of a local repository
source, that may be a branch other than the default. In the case of a remote
repository, it will always be the default branch on the remote host.

Hipcheck identifies the committed date of the most recent commit, and
calculates the number of weeks between that commit and the day Hipcheck is
performing this analysis. It then compares that duration against the
configured threshold (default configuration: 52 weeks / one year). If the
duration in the repository is greater than the configured threshold, then
the analysis will be marked as a failure.

#### Limitations

* __Cases where lack of updates is warranted__: Sometimes work on a piece of
  software stops because it is complete, and there is no longer a need to
  update it. In this case, a repository being flagged as failing this analysis
  may not be truly risky for lack of activity. However, _most of the time_
  we expect that lack of updates ought to be concern, and so considering this
  metric when analyzing software supply chain risk is reasonable. If you
  are in a context where lack of updates is desirable or not concerning, you
  may consider changing the configuration to a different duration, or disabling
  the analysis entirely.

### Affiliation

* Configuration name: `analysis.attacks.commit.affiliation`
* Data source: Git (commit author identities)

Affiliation analysis tries to identify when commit authors or committers
may be affiliated or unaffiliated with some list of countries or organizations.
This determination is based on the email address associated with authors or
committers on each Git commit, compared against a configured list of web hosts
associated with organizations or countries of concern.

The construction of the list is based on an "orgs file," whose path is provided
in the configuration of this form of analysis. This orgs file defines two
things: 1) a list of organizations, including web hosts associated with them,
and the name of the country to which they primarily belong, and 2) a "strategy"
for how the list of to-be-flagged hosts should be constructed.

The strategy defines the list of organizations to be included in the list of
those considered when checking affiliation, and whether the analysis should
flag commits from those _affiliated_ with the list of organizations, or
_independent_ from the list of organizations (for completeness, it also
permits _all_ or _none_, which would flag all commits, or none of them).

If the `strategy` key is used in the configuration, then all organizations
listed in the "orgs file" are implicitly included in the list of organizations
to consider.

If the `strategy_spec` table is used, then `strategy_spec.mode` and
`strategy_spec.list` keys must be defined. The `strategy_spec.mode` key accepts
the same set of values (`affiliated`, `independent`, `all`, or `none`) as the
`strategy` key, while `list` accepts an array of strings in one of two forms:
`"country:<country_name>"` or `"org:<org_name>"`. The first form will include
in the list of organizations all those organizations which are associated with
the named country, while the second form will include in the list a single
organization with the given name.

To illustrate this, imagine the following strategy specification:

```toml
[strategy_spec]
mode = "affiliated"
kind = ["country:United States", "org:MITRE"]
```

This strategy spec would flag any commits those authors or committers can be
identified as being affiliated with any Chinese company listed in the file or
with Google specifically.

#### Limitations

* __The orgs file is limited__: The current construction requires the manual
  definition, in the "orgs file," of companies, their associated web hosts, and
  their primary affiliated country. This manual work is laborious, possibly
  error-prone, requires updating over time, and is less complete than accessing
  more authoritative sources of corporate information.
* __Limits in git's identity system__: Git's identity system is, by default,
  quite weak. Commit author or committer data may be freely spoofed or filled
  with junk information which make identifying the true author or committer of
  a commit impossible. Git _does_ support commit signing, to at least confirm
  that a commit has been authored by the person who owns the relevant signing
  key. This signing can then be checked against known signatures, including
  sources like Keybase which provide easy distribution and checking of known
  signatures against known identities. However, commit signing and checking of
  signatures incorporates the complexity and limitations of cryptographic
  signatures as a technical mechanism for trust, including questions of how to
  handle failures to sign, changes in keys or loss of trust in previously
  trusted keys, and so on. This is an important issue, and incorporation of
  commit signing information in Hipcheck is intended for the future, but
  currently Hipcheck does not use commit signatures in any way. When
  integrated into Hipcheck, the question of how to handle the very common
  case of signatures _not_ being used would arise as well. How to resolve
  this here is an open question.
* __Questions about the best default configuration__: Additionally, there is a
  question in the default configuration of this analysis regarding whether to
  flag commits affiliated with organizations of concern, or commits
  unaffiliated with any known organization. This question relies on assumptions
  of the behavior of malicious actors in this context, and whether malicious
  contributions would be made in commits authored or committed by those using
  their corporate emails.

### Binary

* Configuration name: `analysis.practices.binary`
* Data source: cloned repository (all files in cloned repo filetree)

Binary analysis searches through all of the files in the repository for binary
files (i.e. files not in readable text) that may contain code. There is a high
liklihood that these are deliberately malicious insertions. The precense of such
files could indicate the precense of malicious code in the repository and is a
cause for suspicion.

The analysis works by searching through the entire repository filetree. It
identifies all binary files and filters out files that are obviously not code
(e.g. images or audio files). If, after filtering, more binary files remain than
the configured thershold amount, the repository fails this analysis.

The analysis displays the internal filetree location of each suspicious binary file.
The user can then examine each file to determine if it is malicious or not.

#### Limitations

* __Not all binary files may be malicious__: The repo may use certain binary
  files (beyond image and audio files) for legitimate purposes. This
  analysis does not investigate what the files do, only that they exist.

* __No additional information on binary files__: Hipcheck does not currently
  return any additional information about the suspcious files, only their
  locations in the repo filetree. The user must search for them manually if 
  they wish to learn more about them.

### Churn

* Configuration name: `analysis.attacks.commit.churn`
* Data source: Git (commit diff lines added / deleted, and patch contents)

Churn analysis attempts to identify the high prevalence of very large commits
which may increase the risk of successful malicious contribution. The notion
here being that it's easier to hide malicious content in a large commit than
in a small one, as malicious contribution relies on getting malicious changes
through a normal submission / review process (assuming review is performed).

Churn analysis works by determining the total number of lines and files
changed across all commits containing changes to code in a repository, and
from that the percentage, per commit, of those totals. For each commit, the
file percentage and line percentage are then combined, as file frequency times
line frequency squared, times 1,000,000, to produce a score. These scores are
then normalized into Z-scores, to produce the final churn value for each commit.
These churn values therefore represent how much the size of a given commit
differs from the average for the repository.

Churn cannot run if a repository contains only one commit (or only one commit
that affects a source file). Churn analysis will always give an error when run
against a repo with a single commit.

#### Limitations

* __Whether churn surfaces malicious contributions is an open question__:
  We have ongoing work to confirm that churn does help identify the presence
  of malicious contributions, and therefore is a useful metric for assessing
  supply chain risk against malicious contribution attacks, but at the
  moment this is an assumption made by Hipcheck.
* __Churn's statistical calculations may be insufficient__: There is ongoing
  work to assess the statistical qualities of the churn metric and determine
  whether it needs to be changed.

### Entropy

* Configuration name: `analysis.attacks.commit.entropy`
* Data source: Git (commit diff lines added / deleted, and patch contents)

Entropy analysis attempts to identify commits which contain a high degree of
textual randomness, in the believe that high textual randomness may indicate
the presence of packed malware or obfuscated code which ought to be assessed
for possible malicious content.

Entropy analysis works by determining the total number of occurrences for all
unicode graphemes which appear in a repository's Git diffs for commits which
include code. In then converts these occurence counts into frequencies based on
the total number of each individual grapheme divided by the total number of
all graphemes in the combined set of Git diffs. It also determines grapheme
frequencies for each commit individually. These individual and total grapheme
frequencies are then combined into a score as an individual frequency times
the log base 2 of the individual frequency divided by the total frequency.
These individual grapheme scores are then summed to produce a per-commit score,
which is normalized into a Z-score same as the churn metric. These entropy
values therefore represent how much the grapheme frequency map of a given
commit differs from the average set of grapheme frequencies across all commits.

Entropy cannot run if a repository contains only one commit (or only one commit
that affects a source file). Entropy analysis will always give an error when run
against a repo with a single commit.

#### Limitations

* __Whether entropy surfaces malicious contributions is an open question__:
  We have ongoing work to confirm that entropy does help identify the presence
  of malicious contributions, and therefore is a useful metric for assessing
  supply chain risk against malicious contribution attacks, but at the
  moment this is an assumption made by Hipcheck.
* __Entropy's statistical calculations may be insufficient__: There is ongoing
  work to assess the statistical qualities of the entropy metric and determine
  whether it needs to be changed.

### Fuzz

* Configuration name: `analysis.practices.fuzz`

Repos being checked by Hipcheck may receive regular fuzz testing. This analysis
checks if the repo is participating in the OSS Fuzz program. If it is fuzzed,
this is considered a signal of a repository being lower risk.

#### Limitations

* __Not all languagues supported__: Robust fuzzing tools do not exist for every
  language. It is possible fuzz testing was not done because no good option for it
  existed at the time. Lack of fuzzing in those cases would still indicate a higher
  risk, but it would not necessarily indicate bad software development practices.

* __Only OSS Fuzz checked__: At this time, Hipcheck only checks if the repo
  participates in Google's OSS Fuzz. Other fuzz testing programs exist, but a repo
  will not pass this analysis if it uses one of those instead.
### Identity

* Configuration name: `analysis.practices.identity`
* Data source: Git (commit author and committer identities)

Identity analysis looks at whether the author and committer identities for
each commit are the same, as part of gauging the likelihood that commits
are receiving some degree of review before being merged into a repository.

When author and committer identity are the same, that may indicate that a
commit did _not_ receive review, which could be a cause for concern. At the
larger level, having a large percentage of commits with the same author
and committer identities may indicate a project that lacks code review.

#### Limitations

* __Not every project uses a workflow that accords with this analysis__:
  While some Git projects may use a workflow that involves the generation
  of patchfiles to then be reviewed and applied by project maintainers,
  many may not. In some cases, a workflow may produce final commits where
  the author and committer identity are the same, even though the commit
  received review.

### Review

* Configuration name: `analysis.practices.review`
* Data source: remote Git host API (currently supports: GitHub)

Review analysis looks at whether pull requests on GitHub (currently the
only supported remote host for this analysis) receive at least one
review prior to being merged.

If too few pull requests receive review prior to merging, then this
analysis will flag that as a supply chain risk.

This works with the GitHub API, and requires a token in the configuration.
Hipcheck only needs permissions for accessing public repository data, so
those  are the only permissions to assign to your generated token.

#### Limitations

* __Not every project uses GitHub__: While GitHub is a very popular host
  for Git repositories, it is by no means the _only_ host. This analysis'
  current limitation to GitHub makes it less useful than it could be.
* __Projects which do use GitHub may not use GitHub Reviews for code review__:
  GitHub Reviews is a specific GitHub feature for performing code reviews
  which projects may not all use. There may be repositories which are older
  than the availability of this feature, and so don't have reviews on older
  pull requests.

### Typo

* Configuration name: `analysis.attacks.typo`
* Data source: dependency definition for repository (currently supports: NPM \[JavaScript\])

Typo analysis attempts to identify possible typosquatting attacks in the
dependency list for any projects which are analyzed and use a supported
language (currently: JavaScript w/ the NPM package manager).

The analysis works by identifying a programming language based on the presence
of a dependency file in the root of the repository, then attempting to get the
full list of direct and transitive dependencies for that project. It then
compares that list against a list of known popular repositories for that
language to see if any in the dependencies list are possible typos of popular
package name.

Typo detection is based on the generation of possible typos for known names,
according to a collection of typo possibilities, including single-character
deletion, substitution, swapping, and more.

#### Limitations

* __Only works for some languages__: Right now, this analysis only supports
  JavaScript projects. It requires the implementation of language-specific code
  to work with different dependency files and generate the full list of
  dependencies, and requires legwork to produce the list of popular package
  names, which are not currently pulled from any external API or authoritative
  source.

## `request` Analyses

### Pull Request Affiliation
* Configuration name: `analysis.attacks.commit.pr_affiliation` ("orgs file" is
  in `analysis.attacks.commit.affiliation`)
* Data source: Git (pull request commit author identities)

This analysis is identical to the **Affiliation** analysis, but it only looks
at the commit authors or committers that contributed to the pull/merge request.

See **Affiliation** above for a description of how this analysis works and what
its limitations are.

### Pull Request Contributor Trust

* Configuration name: `analysis.attacks.commit.contributor_trust`
* Data source: Git (commit author contribution history)

This analysis checks all of the commits in the pull/merge request to see if any of the
commit authors are "trusted" or not. The current metric for detemining trust is how
often the author has contributed to the repository.

The analysis starts by looking at all commits in the repository dating back a
configuration-specified number of months. It records the author of each commit and
considers an author to be a trusted contributor if, in that time, it authored a number
of commits greater than or equal to a threshold specified in the configuration. (i.e.
a contributor is trusted if it authored M commits in N recent months).

Authors are tracked by their e-mail address, to account for authors with the same name.

Once a contributor trust map is generated this way, the analysis looks at the commits in
the pull request. Each commit's author is checked against the contributor trust map to
see if that author is trusted or not. Commits with untrusted authors are flagged. The
percentage of flagged commits out of the total number of commits is compared to a
configuration threshold. If too many commits have untrusted authors, the analysis fails.

If the analysis fails, untustred contributors are reported to the user as concerns.

**NB** The analysis currently counts the commits in the pull request when adding up the
total commits by an author. Remember to account for this when setting the minimum commits
needed for an author to be considered a trusted contributor.

#### Limitations

* __Simple trust metric__: At present, the only way that Hipcheck determines a
  contributor's trust is by seeing how many prior commits they have made. More complex
  measures of contributor trust exist, but these are not yet implemented.

* __Follow up is needed for flagged contributors__: There are genuine reasons why a
  contributor might not have contributed to many commits (e.g. contributor is new to the
  project, repository was created recently). Best practice if a pull request fails this
  analysis would be to follow up on who the flagged contributors are and what their commits
  do.

* __Contributors tracked by e-mail__ The same contributor may have multiple e-mail addresses,
and it may be possible to spoof an e-mail address.

### Pull Request Module Contributors

* Configuration name: `analysis.attacks.commit.pr_module_contributors`
* Data source: Git (commit author and committer identities) and JavaScript modules

This analysis determines what fraction of contributors to a pull/merge request are
modifying a module for the first time. This is considered to be suspicious behavior.

The analysis examines each commit of the pull request. It identifies the modules
affected by the commit, the commit author, and the committer. It then digs into the
repository's history to determine if either contributor is modifying a module for the
first time. If a contributor modifies at least one new module, they are flagged as
potentially untrustworthy.

The percentage of flagged contributors out of the total number of contributors is compared
to a configuration threshold. If too many of the contributors to the pull request are
modifying new modules, the analysis fails.

#### Limitations

* __Only works on repositories with a Javascript module structure__: If Hipcheck cannot
  find the module structure of the repository, this analysis will report `Errored`.

* __Follow up is needed for flagged contributors__: There are genuine reasons why a
  contributor might modify a module for the first time (e.g. contributor is new to
  the project, new module, repository was created recently). Best practice if a pull
  request fails this analysis would be to follow up on who the flagged contributors are
  and what their commits do.