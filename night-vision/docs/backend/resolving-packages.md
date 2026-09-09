
# Resolving Packages from Package Sources

One important action a new user of Night Vision may perform is to provide a
"package source" (like a `package.json` file) so Night Vision can identify
packages relevant to that user. This supports both threat alerting and the MVP
goal described in
[RFD 0001](../rfds/0001-mvp-upgrade-safety-assessments.md): assessing whether a
newer package version is a reasonable move from a known-insecure package version,
with explicit motivation to support Federal Civilian Executive Branch (FCEB)
compliance with CISA's BOD 26-04 by noticing KEV-listed package exposure and
identifying safer versions to upgrade to.

It turns out this is a fairly complex operation! So it's worth breaking down
exactly how it works and why it's designed in the way it is.

## Terminology

One unfortunate challenge when discussing this topic is the common naming
collisions between distinct "objects" in the space. For example "NPM" might
refer to either the NPM package host or the `npm` Command Line Interface (CLI).
To distinguish these kinds of cases, we'll use `npm` to refer to the CLI and
NPM to refer to the package host. More generally, when a name is used in a
monospace font (surrounded by backticks in the Markdown source), it refers to
a Command Line Interface (CLI) or file (if a file extension is included); if
a name is presented without a monospace font and uses either Title Case or
all-caps (in the case of an acronym or initialism), then it refers to an
entity such as a package host. We will do our best to be clear about these
distinctions in context as well.

## What is a Package Source?

A "package source" is our generic term for any file that can be used as a
source for packages to track. In the immediate term, it will solely include
`package.json` files as used by the `npm` package management tool, but in the
future may include other package source formats, both for `npm` itself and for
other language and packaging ecosystems.

Note that this explicitly _excludes_ `package.json` as it may be used by other
package management tools such as `pnpm`, which will be considered for support
in the future. Package sources, such as `package.json`, must be understood as
specifically tied to a package manager such as `npm`, _even when_ other package
managers reuse or remix the same package source (as `pnpm` does for `npm`, for
example).

## Why Not Use a Lockfile?

The first question to answer is why we use a "package manifest" (like `npm`'s
`package.json`) rather than a "lockfile" (like `npm`'s `package-lock.json` or
the similar "shrinkwrap" file `package-shrinkwrap.json`).

To explain, let's start by comparing package manifests and lockfiles. Package
manifests specify dependencies for a project alongside version constraints for
those dependencies.

For example, with `npm`, package versions are traditionally specified with the
`^` range syntax, which effectively permits any version which is
"SemVer-compatible" with the listed version. For example, `^1.2.3` permits any
version `1.2.3` or higher, but not `2.0.0` or higher, meaning it elaborates to
the bounds `>=1.2.3, <2.0.0-0`, where `-0` is a prerelease component used in
the upper-bound comparator. This upper bound excludes not only `2.0.0` and its
successors, but also prerelease versions of `2.0.0`. There are [many other
operators][node_operators] available besides `^`, `>=`, and `<`.

Even when a manifest appears to specify individual dependency versions, in some
ecosystems they're in fact specifying a "version constraint." This is not true
for `npm`, where a version specifier such as `1.2.3` is solely identifying
version `1.2.3`, but _is_ true for Cargo/Crates.io, the Rust build tool and
package host, respectively, where `1.2.3` is implicitly a version constraint
using the `^` syntax: `^1.2.3`, though the specifics of how this constraint is
elaborated are slightly different in the context of Cargo (the specifics of
which will matter if/when we add support for Cargo package sources to Night
Vision).

By contrast, a lockfile (or shrinkwrap file) "locks" all dependencies,
including transitive dependencies, to _specific_ versions. This is done
via a package-manager-specific resolution algorithm which attempts to "unify"
dependency versions when possible (i.e. when multiple dependencies in the
dependency graph have a shared dependency in common, and where it is possible
to resolve a singular version which fulfills the version constraints of both
dependents). The specifics of this "unification" are tied to individual
package manager tools (for example, `npm`, `pnpm`, `yarn`, `bun`, and `deno`
in the JavaScript world).

Lockfiles are an extremely useful mechanism for ensuring that contributors to
a project are all working with a consistent set of dependencies, and that
releases of the software are made using a set of dependency versions which
has been tested and is known to work.

However, lockfiles can also be _brittle_. There are many situations in which a
user may accidentally fail to use a lockfile, or may bump versions in a
lockfile. Such changes may not also always be appreciated as security-relevant
by the overall project when reviewing changes, or may (if no code review is
performed) be committed and distributed to other contributors quickly.

These situations can include: accidentally deleting a lockfile and then
attempting to recreate it from scratch (there is no guarantee that the
versions in the newly-created lockfile are consistent with the versions in
the deleted lockfile), or accidentally running a command which ignores a
lockfile. These are more common than you might realize, based both on CLI
affordances across language ecosystems and on the common situation of projects
encountering version control system conflicts in lockfiles when multiple
contributors make changes which modify dependencies in some manner. In such
conflict situations, it is shockingly common for "fixes" to the conflict to
involve unintentionally over-eager updates to dependency versions, which may
present serious security risks.

Given this brittleness, we should not rely on lockfiles as a security
mechanism when assessing package dependencies for risks. Rather than risk
being under-inclusive by using lockfiles as our input, we ought to use
package manifests, and perform our own maximally-inclusive dependency
resolution to identify packages to track.

## This is Fiendishly Difficult

Before we get into the specifics of how we'll approach package resolution, it's
worth saying that the problem at hand is very difficult to get precisely right.
Even within a single ecosystem, the rules for package resolution can be quite
complex. To pick on `npm`, for example, a `package.json` has five kinds of
dependencies which may be specified: `dependencies`, `devDependencies`,
`peerDependencies` (and `peerDependenciesMeta`), `optionalDependencies`, and
`bundleDependencies`. Additionally, a top-level `package.json` can specify
overrides for dependencies used transitively via the `overrides` field, and
projects which use `npm`'s "workspaces" feature will implicitly incorporate all
other packages present in the workspace.

Dependencies themselves may also be specified in a variety of ways, including
as plain versions, version constraints, Git URLs (optionally with a specific
commit hash), "GitHub URLs" (a shorthand for specifying source repositories
hosted on GitHub), local paths, and more.

The algorithm described in the following section represents a best-effort
design intended to be maximally-inclusive of packages which may be reachable
from a package source, with the goal of reducing the likelihood of "missed
packages," which are reachable from a package source but do not end up tracked
by Night Vision due to an insufficiency of our package resolution algorithm.
We'd rather be overinclusive and track more packages than are necessary, than
be underinclusive and miss meaningful risks to our users.

## Package Resolution from Package Sources

> [!important]
> The following represents a generic algorithm, the specifics of which will be
> tailored to specific package sources and package management ecosystems. In
> particular, different package sources will have different kinds of
> dependencies, different available version constraints and version resolution
> rules, and different mechanisms of package publication. This is not intended
> to be the final and complete word on how to resolve packages from _all_
> kinds of package sources and _all_ ecosystems, but instead is a sketch of
> the high-level process which will be specialized to new cases as we expand
> Night Vision's support for more formats and ecosystems over time.

The next question to resolve is how we figure out the set of packages to track
for supply chain threats based on a user's provided package source.

The basic solution here is to have a queue of packages to resolve, and
initially add every package specified in the user's package source to the
queue with both the package name and the elaborated version constraints, along
with a "parent package/version combination," which will later be used to
describe the route from the top-level dependency specified in the package
source to the current package being resolved. "Elaborated" here means the
constraints expanded from their more terse form to a more-explicit form
incorporating comma-separated constraints which only use more traditional
less-than / greater-than operators. For example, the constraint `^1.2.3` is
elaborated to `>=1.2.3, <2.0.0-0`.

Each entry in this queue is then resolved against the versions of the package
currently published to the relevant package registry which match the version
constraints. Each resolved version is then added to the set of packages to
track for supply chain threats, and each dependency of each resolved package
is then added to the queue of packages to resolve. This is inherently an
over-approximation, especially in uncommon cases where packages use constraints
such as wildcard versions which may permit a large number of versions which are
not used practically.

At each step, if a package/version combination is already present in the set
of packages to track, it is skipped over to avoid duplicate entries, though
any new known parent package/version combinations for that package are still
added to the known parent package/version combinations which reached that
package/version combination in the to-track set.

When the queue is empty and all packages have been resolved, the set of known
parent package/version combinations is used to resolve a final set of
"derivation paths" for every package/version combination, which represents all
acyclic paths through the dependency graph from a root dependency to a
transitive dependency, with cycle markers indicating where expansion would
repeat a package/version pair. Note these paths distinguish packages by
version, so if Package A has versions X and Y, both of which depend on Package
B at version Z, there would be two distinct derivation paths which reach
Package B and version Z and go through Package A via versions X and Y,
respectively.

The resulting graph after expansion of these derivation paths then represents
every package deemed reachable from the initial package source.

Note that this final set does not distinguish the distinct sets of possibly-
resolvable lockfiles from the initial package source, only the set of packages
deemed reachable from the initial package source.

The specifics of how this resolution is performed will vary somewhat from
package ecosystem to package ecosystem, based in part on the versioning
schemes and version operators available, and the package resolution rules
employed by the package host.

There is also additional complexity associated with the presence of alternative
registries, which may need to be accounted for during resolution, or the
possibility of source-only packages which are not published to a package
registry. Furthermore, ecosystems _without_ a package registry, such as Go
(which employs a direct-from-source dependency model made smoother by a
pass-through proxy) may require additional modifications to the logic.

As such, while the general algorithm described here should be roughly
sufficient across ecosystems, we should also understand there will be
substantial variation in specifics as we both grow across packaging ecosystems
and grow to encompass more _types_ of dependencies which may be expressed
within a single ecosystem.

## A Limitation of the Current Approach

One limitation of this approach, inherent to the choice to solely accept a
single package source file from our users, is that we will not be able to
incorporate information cutting across multiple local packages in a workspace.

It's possible in the future that we may want to enable users to perhaps upload
bundles of their local project workspace (perhaps via a helper CLI), but doing
so would involve substantial challenges and barriers to adoption. For example,
it's doubtful many users would be comfortable potentially exporting highly
sensitive intellectual property from their project out to a third-party
service. By comparison, a single package source file is much easier for a user
to review and to strip of any sensitive IP.

[node_operators]: https://github.com/npm/node-semver#ranges
