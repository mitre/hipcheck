---
title: Target Resolution Refactor
weight: 5
slug: 0005
extra:
  rfd: 5
  primary_author: Julian Lanson
  primary_author_link: https://github.com/j-lanson
  status: Accepted
  pr: 266
---

# Target Resolution Refactor

In Hipcheck, the target resolution process finds a repository matching the
specifications a user provides on the command line in the form of a target
specifier string, optional type hint, and additional arguments. It then copies
or clones that repository to the Hipcheck cache and checks it out to a suitable
commit. As the Hipcheck team works to implement [RFD #4][rfd_4], Hipcheck as a
codebase will transition from housing analysis techniques to becoming an engine
for orchestrating analyses implemented by plugins. As a result of this
transition, the target resolution phase will become a more essential and
critical part of what the core Hipcheck repo does.

As the Hipcheck team looks to provide users with more flexibility in repo/commit
specification (such as via the `--ref` argument for `hc check`), we have found
it difficult to debug the existing resolution code and add new features to it,
which is a good indication that the resolution subsystem needs a rewrite. This
RFD aims to do the following:

- Clearly establish the design goals of the refactor
- Offer a new design that meets these goals

## An Illustrating Example

An example of a pain point in the existing codebase involves how package
versions are treated by the later remote repo resolution phase. When a user
targets a package for analysis, they may provide a version number in the target
string itself (e.g. `node-ipc@10.1.0`), or provide a git reference to checkout
via the `--ref` flag. We do perform a sanity-check before we even get to target
resolution that a user hasn't provided both a version string and a `--ref` flag.
However, once we get to doing `RemoteGitRepo` resolution, the version string if
provided collapses down to the same parameter as the `--ref` flag, `refspec`.
We've discovered that (unsurprisingly), a package's version numbering scheme
doesn't always map directly to the tags in its repository (e.g. `hipcheck`
version `3.5.0` has the tag `hipcheck-v3.5.0`, not simply `3.5.0`). Therefore,
specifically when we go through `Package` resolution, we would like to be able
to do "fuzzy matching" of the version to tags in the repository. We need the
local repository clone to be able to do that matching, but once we get to
cloning/updating a remote repository on the local machine, we have lost the
context that we started resolving from a package and that we would like to
handle the ref checkout differently if an error occurs (e.g. because the version
passed as a ref does not directly match a tag in the repository).

Fixing this problem requires designing the resolution in such a way that it
remembers the context of previous resolution steps.

## Refactoring Goals

Overarching goals of the refactor are as follows:

### Increased debugability

A developer's ability to debug the target resolution subsystem will be
proportional to how clearly the code itself reflects the business logic we want
the resolution to follow. This includes clearly delineating the "phases" of
resolution and what class of logic belongs where. For example, in [the example
above](#an-illustrating-example), is version fuzzy-matching logic something that
the "package resolution" function should handle when the inner "remote repo
resolution" function fails, or should it be a part of the "checkout" function
that is aware of the context that it is operating on something that was
previously a package?

### Maintaining resolution state

Knowing what previous resolution steps were taken, being able to access what
they produced, being able to mutate internal state of the yet-unfinished
`Target` struct are important to allow custom logic for particular resolution
"paths". As the [illustrating example](#an-illustrating-example) shows, the
current resolution subsystem does not adequately convey context to subsequent
resolution step.

### Distinguish between valid and unfinished resolutions

A principled design will make it easy to distinguish between a "finished"
representation of a `Target` and one that is actively being modified as part of
resolution.

## A Potential Design

### `TargetResolver`

```rust
struct TargetSeed {
    // What was passed on the CLI
    pub specifier: String,
    // What we resolved from it
    pub kind: TargetSeedKind,
    // Options related to repo resolution
    pub refspec: Option<String>,
}

struct Target {
    pub specifier: String,
	// All instances must point to a valid repository
    pub local: LocalGitRepo,
    pub remote: Option<RemoteGitRepo>,
    pub package: Option<Package>
}
```

One of the issues with the current design is that we do a direct transition
from `TargetSeed` to `Target`, with no intermedate state. `Target` was designed
to represent a ground-truth, "finished" resolution, and so the `local` field is
non-optional - indicating a `Target` instance should always be pointing to a
valid local repository. So although our refactor design goals involve
maintaining state during resolution, `Target` itself is not ideal for this role
as we need to put a dummy value for `local` until we finish cloning a
repository to the cache when our seed is anything but existing local
repository.

```rust
struct TargetResolver {
    seed: TargetSeed,
    local: Option<LocalGitRepo>,
    remote: Option<RemoteGitRepo>,
    package: Option<Package>,
    maven: Option<MavenPackage>,
    sbom: Option<Sbom>,
}
```

Enter the `TargetResolver`. This struct takes over from `Target` as the
intermediate resolution state. All of the fields are meant to be mutable except
for `seed`, which ensures resolution steps can always refer to exactly what was
originally passed. As intermediate resolution steps are taken, corresponding
optional fields (initialized to `None`) are set to `Some(_)`. Not all fields
need be filled with a `Some` value, but eventually once `local.is_some()` we
know the resolution is finished and we can synthesize and return a `Target`.
This achieves the design goal of distinguishing finished and unfinished
resolutions; once another piece of code gets a `Target` instance, all the
package version, git ref, and other information can be trusted to be accurate.
Note that this contains `maven`, and `sbom` fields that are not in `Target`. We
expect that once a `Target` is resolved, these fields will not be of interest
for analysis.

### Resolution phase organization

To define how resolution goes from a loose bag of `Option` values to a `Target`,
we applied the [Strategy][strategy_pattern] design pattern. We define a trait
`ResolveRepo` for each `TargetSeedKind` variant to determine the steps it will
take.

```rust
trait ResolveRepo {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo>;
}
```

Generally, the idea is for a type to produce another `TargetSeedKind` type, and
then invoke that type's `resolve()` implementation. The function takes a
mutable reference to `TargetResolver`, allowing the state to be mutated and
then passed to the next `resolve()`.

```rust
impl ResolveRepo for Sbom {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
        let sbom = t.sbom.as_ref().unwrap();
        // sbom_to_package()
        let p: Package = todo!();
        // Update the resolver context
        t.package = Some(p.clone());
        // Move onto resolving the package
        p.resolve(t)
    }
}
```

Instead of returning a `Result<Target>`, we return a `Result<LocalGitRepo>`,
allowing the `TargetResolver` class itself to produce and return the `Target`.

```rust
impl TargetResolver {
	pub fn resolve(seed: TargetSeed) -> Result<Target> {
        let mut resolver = TargetResolver {
            seed: seed.clone(),
            local: None,
            remote: None,
            package: None,
            maven: None,
            sbom: None,
        };
        use TargetSeedKind::*;
		// Resolution logic depends on seed
        let local = match seed.kind {
            Sbom(sbom) => {
                resolver.sbom = Some(sbom.clone());
                sbom.resolve(&mut resolver)
            },
            MavenPackage(maven) => {
                resolver.maven = Some(maven.clone());
                maven.resolve(&mut resolver)
            },
            Package(pkg) => {
                resolver.package = Some(pkg.clone());
                pkg.resolve(&mut resolver)
            },
            RemoteRepo(repo) => {
                resolver.remote = Some(repo.clone());
                repo.resolve(&mut resolver)
            },
            LocalRepo(local) => {
                resolver.local = Some(local.clone());
                local.resolve(&mut resolver)
            },
        }?;
        // Finally piece together the Target with the non-optional local repo
        Ok(Target {
            seed: resolver.seed,
            local,
            remote: resolver.remote,
            package: resolver.package,
        })
    }
}
```

### Context-aware logic

With this design, we make the choice to organize logic by subphase rather than
by `TargetSeedKind` type. Referring back to the [illustrating
example](#an-illustrating-example), although the fuzzy version matching logic
is specific to resolution paths with a `Package` ancestry, instead of in
`Package::resolve()` we will implement that logic in `LocalGitRepo::resolve()`,
leveraging the context provided by the passed `TargetResolver` to allow us to
express this logic. For example:

```rust
impl TargetResolver {
    pub fn get_checkout_target(&mut self) -> Result<String> {
        let res = if let Some(pkg) = &self.package {
            // if version != "no version", try fuzzy match against repo, we know
            // we already have a local repo to use
            // if version fuzzy match fails, and self.seed.ignore_version_errors, use "origin/HEAD"
            todo!()
        } else if let Some(refspec) = &self.seed.refspec {
            // if ref provided on CLI, use that
            todo!()
        } else {
            use TargetSeedKind::*;
            match &self.seed.kind {
                LocalRepo(_) => "HEAD",
                RemoteRepo(_) => "origin/HEAD",
                _ => { return Err(anyhow!("please provide --ref")); }
            }.to_owned()
        };
        Ok(res)
    }
}

impl ResolveRepo for LocalGitRepo {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
        let checkout_tgt: String = t.get_checkout_target()?;
        git::checkout(&checkout_tgt)?;
        todo!()
    }
}
```

This `get_checkout_target()` function returns what ref the local repo should be
changed to point to. See that we are able to express the custom version
fuzzy-matching if the resolution ancestry includes a package, plus custom
default targets if the `TargetSeedKind` was a local or remote git repo.

#### Full Pseudocode

```rust
struct TargetSeed {
    // What was passed on the CLI
    pub specifier: String,
    // What we resolved from it
    pub kind: TargetSeedKind,
    // Options related to repo resolution
    pub refspec: Option<String>,
    pub ignore_version_errors: bool,
}

struct Target {
    pub seed: TargetSeed,
    pub local: LocalGitRepo,
    pub remote: Option<RemoteGitRepo>,
    pub package: Option<Package>
}

struct TargetResolver {
    seed: TargetSeed,
    local: Option<LocalGitRepo>,
    remote: Option<RemoteGitRepo>,
    package: Option<Package>,
    maven: Option<MavenPackage>,
    sbom: Option<Sbom>,
}

impl TargetResolver {
    pub fn get_checkout_target(&mut self) -> Result<String> {
        let res = if let Some(pkg) = &self.package {
            // if version != "no version", try fuzzy match against repo, we know
            // we already have a local repo to use
            // if version fuzzy match fails, and self.seed.ignore_version_errors, use "origin/HEAD"
            todo!()
        } else if let Some(refspec) = &self.seed.refspec {
            // if ref provided on CLI, use that
            todo!()
        } else {
            use TargetSeedKind::*;
            match &self.seed.kind {
                LocalRepo(_) => "HEAD",
                RemoteRepo(_) => "origin/HEAD",
                _ => { return Err(anyhow!("please provide --ref")); }
            }.to_owned()
        };
        Ok(res)
    }

    pub fn resolve(seed: TargetSeed) -> Result<Target> {
        let mut resolver = TargetResolver {
            seed: seed.clone(),
            local: None,
            remote: None,
            package: None,
            maven: None,
            sbom: None,
        };

        use TargetSeedKind::*;

        // Resolution logic depends on seed
        let local = match seed.kind {
            Sbom(sbom) => {
                resolver.sbom = Some(sbom.clone());
                sbom.resolve(&mut resolver)
            },
            MavenPackage(maven) => {
                resolver.maven = Some(maven.clone());
                maven.resolve(&mut resolver)
            },
            Package(pkg) => {
                resolver.package = Some(pkg.clone());
                pkg.resolve(&mut resolver)
            },
            RemoteRepo(repo) => {
                resolver.remote = Some(repo.clone());
                repo.resolve(&mut resolver)
            },
            LocalRepo(local) => {
                resolver.local = Some(local.clone());
                local.resolve(&mut resolver)
            },
        }?;

        // Finally piece together the Target with the non-optional local repo
        Ok(Target {
            seed: resolver.seed,
            local,
            remote: resolver.remote,
            package: resolver.package,
        })
    }
}

trait ResolveRepo {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo>;
}

impl ResolveRepo for Package {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
        todo!()
    }
}

impl ResolveRepo for MavenPackage {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
        todo!()
    }
}

impl ResolveRepo for Sbom {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
        let sbom = t.sbom.as_ref().unwrap();
        // sbom_to_package()
        let p: Package = todo!();
        // Update the resolver context
        t.package = Some(p.clone());
        // Move onto resolving the package
        p.resolve(t)
    }
}

impl ResolveRepo for RemoteGitRepo {
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
        todo!()
    }
}

impl ResolveRepo for LocalGitRepo {
    // logic for refspec/version resolution, can look at t.seed to distinguish
    // between the two
    fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
        let checkout_tgt: String = t.get_checkout_target()?;
        // git::checkout(&checkout_tgt)?;
        todo!()
    }
}
```

[rfd_4]: ./0004-plugin-api.md
[strategy_pattern]: https://refactoring.guru/design-patterns/strategy
