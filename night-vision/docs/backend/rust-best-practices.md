
# Rust Best Practices

The following a living document of recommendations for how to write Rust code
effectively. The lessons learned here are from a mix of personal experiences
and folk knowledge passed on from public retrospectives and discussions with
Rustaceans inside and outside of MITRE.

Everyone is welcome to edit this document to incorporate new recommendations
as we encounter issues or learn new patterns we want to share.

Note that these are best practice _recommendations_, and are not hard rules
for what is permissible vs. impermissible in the Night Vision codebase. There
can be good reasons at times for deviating from the recommendations made here.
If you do choose to deviate from a recommendation here, please explain _why_,
ideally both in comments within the relevant code and in your commit messages
and Merge Request description. This will help your code reviewers and future
editors of the code to understand why the code is designed the way it is,
and to make informed decisions about whether the deviation is justified or
ought to be maintained in the future.

Many of the items which _would_ be recommendations listed here are instead
enforced via our custom `cargo clippy` configuration ("clippy" is Rust's
standard linter, which implements a variety of checks beyond those provided
by the Rust compiler itself).

## Table of Contents

[[_TOC_]]

## Code Basics

The following are some basic ideas for writing clear and idiomatic Rust code.

### Follow the Rust API Guidelines on Naming

Rust has specific expected patterns for how to name things (crates, modules, 
types, etc.), which you can find in the [Rust API Guidelines' section on
naming][naming].

## Supply Chain Security

Open source software is a double-edged sword. On the one hand, you get the
spectacular benefits of code reuse and the output of community labor on a
common artifact; on the other, each dependency represents a possible vector
for malware distribution.

Worse, software supply chain attacks are on the rise, including worm-style
attacks that work their way through vulnerable Continuous Integration
pipelines.

Below are steps both prophylactic and reactive which help to reduce the risk
of supply chain compromise for OSS consumers.

### Eliminate Unused Dependencies

Use `cargo udeps` to identify and remove unused dependencies.

### Keep Dependencies Up-to-Date

Use `cargo outdated` to identify and update or remove outdated dependencies.
When the outdated dependencies are transient dependencies, consider reaching
out to the upstream project that integrates the out-of-date dependency to get
them to upgrade or help them to do so.

### Check Dependencies for Known Vulnerabilities

Use `cargo audit` to check your dependencies against the RustSec advisory
database, which tracks known vulnerabilities in the Rust crate ecosystem. Note
that RustSec also tracks unmaintained packages, which may not represent a
vulnerability but are a different kind of risk indicator.

## Build Performance

Slow builds are one of the most common complaints about Rust in the annual
Rust language survey. While some of this is Rust's fault, some of it is because
Rust's language affordances and powerful package system make it easy for us
as users to unintentionally balloon build times.

The following are some tips for keeping builds fast.

### Keep Builds as Parallel as Possible

Cargo tries its best to compile compilation units in parallel as much as
possible. For each codegen unit builds are actually split between a "metadata"
phase which prepares to build, and a "codegen" phase which actually does the
build, with compilation of dependent crates being possible as soon as all
"metadata" phases are done. This means a dependent can start compiling *before*
the thing it depends on has finished!

Build scripts and procedural macros get in the way of that, because they have
to be built (and run, in the case of build scripts) *before* the metadata
phase. So build scripts and procedural macro crates can easily become a
bottleneck that limits pipelining, blocking large numbers of compilation units
that depend on a crate that uses one or both of these features.

The solution is twofold:

- For build scripts: keep them as fast as possible and avoid reruns.
- For procedural macros: use with caution.

Let's dig into these.

#### Keep Build Scripts Fast and Avoid Reruns

Since build scripts need to both build _and run_ before the metadata phase of
their associated crate (and thus, before dependents can start building at all),
you want to ensure that both building and running build scripts is as fast as
possible.

To keep build scripts building fast, minimize build script dependencies, even
if it means you need to write more code yourself. Avoid lots of generic in
build scripts as well, to minimize the time spent on codegen when building
them. Ideal build scripts have only a few small dependencies, if any, and
incorporate no generic code.

To keep build scripts running fast, ensure you're writing efficient code that
avoids unnecessary work (some of our `cargo clippy` lints for Night Vision will
help with that), keep computational complexity as low as you can, and most
importantly: teach Cargo when it can halt build script execution.

By default, Cargo reruns build scripts if any files in the associated crate
have changed. However, it's often the case that the changed files are
irrelevant for the build script. To help, every build script should emit one or
more of the following strings to `stdout`:

- `cargo::rerun-if-changed=PATH`: This tells Cargo to rerun the build script
  only if the pointed-to file or directory has changed. For directories, this
  means that all files within the directory will be checked for updates. If a
  build script _never_ needs to be rerun, use
  `cargo::rerun-if-changed=build.rs`, which will only rerun the script if the
  script itself changes. Note that Cargo tracks last-modified timestamps for
  all tracked files to make the "is updated" determination, so updating that
  timestamp on a file's metadata, even without modifying the contents, is
  sufficient to trigger a rebuild and rerun of the build script.
- `cargo::rerun-if-env-changed=NAME`: This tells Cargo to rerun the build
  script if the value of the named environment variable has changed. Note that
  Cargo already detects use of the `env!` and `option_env!` macros in build
  scripts, and considers their named environment variables as having been
  supplied to `cargo::rerun-if-env-changed=NAME`, so you don't need to
  explicitly track them.

#### Use Procedural Macro Crates Cautiously

Procedural macro crates are special crates that are compiled into compiler
plugins for rustc itself. As such, they need to be built fully before anything
that depends on them (dependents wait on the completion of the codegen phase,
not just the metadata phase, for procedural macro crates).

Unfortunately, the nature of procedural macro crates is that they tend to also
be slow to compile. The most common libraries used in these crates include
`syn`, a very powerful library which provides a complete implementation of
parsing for Rust source code, and which is itself very large.

If you're considering using a third-party procedural macro crate, check if it's
using `syn` as a dependency, and if it is, check the feature it's compiling
`syn` with. `syn`'s feature-set is rich, and the crate offers a variety of
features which can be turned on or off to include or exclude features. Users of
`syn` should ideally trim those features down to only the ones they are using,
to help speed up compilation. In the Night Vision project specifically, `syn`
will always be compiled with the union of all features selected by our
transitive dependencies, thanks to the use of a `workspace-hack` crate managed
by `cargo-hakari`.

If you're considering writing your own procedural macro crate, consider
alternatives to `syn`, such as `unsynn` or `facet`, which trade-off some of
`syn`'s power for much faster compilation.

### Reduce Dependencies and Reduce Features

Another obvious way to speed up builds is to build less code! Reducing
dependencies can be a useful way to reduce build times; and if dependencies
can't be fully removed, investigate their available crate features to see if
you can reduce the amount of code within a dependency that you're actually
using.

## Program Performance

Rust is a language that can run _very fast_, competitively with C and C++.
However, just because Rust is a "fast language," that doesn't mean that all
Rust programs are fast. It's incumbent on us as Rust programmers to design
programs that take advantage of Rust's strengths.

### Avoid Excessive Cloning

Rust's ownership system, where all data has a single owner, can feel
restrictive at times. It's fairly common, especially as a new Rustacean, to
encounter situations where a piece of data's owner does not live long enough
for a borrow of the data to be placed into a desired structure or passed to
a particular function. In those cases, it can be appealing to clone the data
via the `Clone` trait, creating a copy that gets passed to the appropriate
place.

While freely cloning data can be a useful crutch when first learning the
language, it's something to avoid most of the time in production code. Often,
issues with insuffuciently-long lifetimes are an indicator that the code is
structured improperly and needs to be refactored such that the owner *does*
live long enough.

Alternatively, it may be that data should more rightly be modeled as having
multiple owners. In that case, use `std::rc::Rc` or `std::sync::Arc`
(as appropriate) to share access to the same underlying data without copying.
Refcount increments and decrements, even atomic ones (in the case of `Arc`) are
still preferable to cloning unless data is quite small.

### Avoid Materializing Collections Unnecessarily

Rust's standard collection types such as `Vec` and `HashMap` are very useful.
However, it's common when working with them to end up unintentionally
creating intermediate collections that aren't actually necessary.

When transforming collections, prefer to work with iterators where possible,
which enable you to describe pipelines of operations on a collection without
materializing actual intermediate structures. During compilation, iterator
pipelines often compile down to very minimal and efficient code; materializing
full collection types can serve as a major performance barrier by contrast.

Be wary of calls to `clone`, `cloned`, `copied`, and `collect` when working
with collections and their iterators.

## Program Size

Rust's system of generics is extremely powerful, but it's easy to go overboard,
and that overuse can have a serious negative consequence on code generation,
which impacts both build performance and program size.

It's surprising to most Rust programmers to learn that the majority of build
time for most individual crates is spent in code generation, not in
typechecking, but it's true!

### Use the Non-Generic Inner Function Pattern When Possible

When possible, minimize the use of generics. When generics are used, try to
minimize the size of the function bodies of generic functions through the
["non-generic inner function"][ngif] pattern.

## Testing

Guidance for how to effectively write and run tests.

### Use `cargo-nextest` To Run Tests

Tests are fantastic, and should be written liberally and run frequently.
Unfortunately, Rust's default test harness and test CLI leave a lot to be
desired. Thankfully, we have `cargo-nextest`, an alternative test harness and
CLI that bring an enormous number of improvements, such as:

- A much clearer user interface.
- Enormous performance improvements (tests often run an order of magnitude
  faster than the default test harness and CLI).
- A more powerful set of features for selecting subsets of tests to run.
- Automatic detection of slow tests.
- The ability to stress-run tests, repeating them many times.
- Per-test configurability, to control retries, test scheduling, and more.
- The ability to record and replay test runs.
- The ability to output Perfetto traces of tests, for performance analysis.
- The ability to partition tests across CI runners.
- Support for setup scripts to run before tests.
- A bunch of additional tool integrations for debugging, mutation testing, 
  coverage analysis, and more.

Use `cargo nextest r` to run tests, instead of the default `cargo text`
command.

### Use `insta` and `cargo-insta` For Snapshot Tests

Sometimes you want to have tests which validate that some output does not
change over time. In our context for `nv-server`, this can be very useful for
validating the endpoint responses do not change over time, catching things
like error code regressions.

For snapshot testing, use the library `insta` and the `cargo-insta` CLI.
`insta` assists with writing snapshot tests, while `cargo-insta` provides an
interface for reacting to output changes across test runs, enabling you to
confirm when changes are expected and overwrite the saved "expected" value.

### Consider Property-Based Testing With `proptest`

Property-based testing is an approach that generates an enormous number of
test inputs based on a user-defined generation scheme, and then validates that
all generated inputs fulfill the desired constraints. When an input fails to
validate, `proptest` and other property-based testing libraries will "shrink"
the input, attempting to discover smaller versions of the input which still
demonstrate the failure, to make debugging the failure easier.

Property-based testing serves as an excellent complement to more traditional
case-specific unit tests by exploring edge cases which human developers may
forget to validate.

When specific cases are found to fail by a property-based test, and the API
being tested has been fixed, the previously-failing case should be added as
a regression test to ensure success going forward.

### Add Regression Tests With Every Fix

Whenever you fix a bug in the program, add regression tests.

Regression tests are tests which check situations which previously caused
incorrect behavior to ensure the system does not _regress_ and behave
incorrectly in those situations again.

[api]: https://rust-lang.github.io/api-guidelines/about.html
[naming]: https://rust-lang.github.io/api-guidelines/naming.html#casing-conforms-to-rfc-430-c-case
[ngif]: https://www.possiblerust.com/pattern/non-generic-inner-functions
