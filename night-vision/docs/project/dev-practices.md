
# Dev Practices

The Night Vision team is intended to be a collaborative, high-pace team that
works together to solve difficult problems.

The following are a set of expectations and recommendations for the team.

## Writing Code

The number one task we do day-to-day on this project is writing code. As such,
it's good to describe how we can do that most effectively.

### Documentation

Documentation, the act of writing down your assumptions, expectations,
practices, and designs, is an essential part of building software _together_.

We believe in code comments for Night Vision. Not useless comments that simply
repeat what's obvious from the text of code, but useful comments that explain
_why_ something works the way it does. These not only help your teammates; they
also help you in the future, when you return to the code after forgetting the
context that lead you when writing it.

We also believe in documenting APIs well. For the backend, this means writing
clear doc comments that answer questions like 1) what the API is for, 2)
whether the code in question can panic, 3) if the code is async, whether it
is cancellation-safe, 4) gives examples for how to use the API. We also
recommend using `cargo doc` liberally to ensure you're clearly structuring
and explaining the API. These doc comments also benefit yourself and other
contributors because they are surfaced by [Rust Analyzer] when working in the
codebase.

Documentation is also useful for working with AI agents effectively. If there
are details about the system which you think are important to understand and
which don't make sense to include within API documentation, consider adding
them to the `docs/` folder, both for your fellow teammates and for any AI
agents you or others want to use to contribute. Note that use of AI is subject
to the team's [AI Policy](./ai-policy.md).

### Debugging

Debugging is a fact of life in software development. Here are some tips for
debugging Night Vision.

#### `nvdb`

`nvdb` is a custom project-specific debugger for the backend. When you are
debugging the system and need to perform queries to REST API endpoints or
inspect database state, consider adding those as new commands in `nvdb` instead
of writing one-off `bash` or `psql` commands. Not only does this make your
own workflows easier to remember and reuse, it also conveniently shares those
mechanisms with your teammates.

The goal is that over time `nvdb` grows to become more and more useful for
answering questions about the operation of `nv-server` so we can get to root
issues faster.

#### `tokio-console`

Since `nv-server` uses [Tokio](https://tokio.rs/) as its async runtime of
choice, we have the ability to use
[`tokio-console`](https://github.com/tokio-rs/console) to aid in debugging
runtime issues. `tokio-console` is a tool for collecting and displaying
diagnostic data pulled from the Tokio runtime. This includes warnings such as
task futures being too large, tasks self-waking too frequently, or tasks which
are excessively blocking, alongside a `top`-style view of the runtime's current
tasks.

#### Debugging Calls

Debugging is a team sport!

On the Night Vision team, we highly recommend turning debugging sessions into
opportunities for collaboration. If you're stuck on a problem and attempting
to work through it, consider starting a call in the night-vision-dev Teams chat
and inviting others to join in and review the problem.

Joining a debugging call is always optional, but can be a useful way both to
better understand parts of the system you don't directly work on, and to assist
your teammates in fixing issues and delivering more quickly.

Even better, consider recording these calls, as the information contained in
them can often be very useful to refer back to, for example when writing up
bugs or encountering new issues in the future.

### Testing

Writing and regularly running tests is important. For the backend, we use
[`cargo-nextest`](https://nexte.st/), an alternative test runner with
per-process test isolation, massive improvements for parallelism, test
recording and replays, and much, much more.

New features, especially new endpoints for the `nv-server` REST API, should be
accompanied by tests for common success and failure cases. Additionally, fixes
for bugs should usually be accompanied by new regression tests.

In general, adding tests is always a good activity to pursue to make Night
Vision better.

## Version Control (Git / Jujutsu)

There are many ways to use [Git](https://git-scm.com/), the popular Version
Control System (VCS), so it's worth explaining how _we_ use it on Night Vision.

Additionally, some developers on Night Vision may choose to use
[Jujutsu](https://www.jj-vcs.dev/latest/), an alternative Git-compatible VCS,
so we document how to do that as well.

### Short-Lived Dev Branches

When working on changes to Night Vision, keep short-lived, feature-focused
branches for your work. Long lived branches are harder to merge in the future,
as they're more likely to drift from `main` and need substantial surgery to
rebase into a merge-able state.

On Jujutsu, the same is true and for the same reasons. Keep short-lived dev
bookmarks.

### Integrate Dev Branches by Rebasing

When making an MR, be sure it is mergeable (GitLab will tell you if it is not).
If it's not, then rebase it off the latest from `main`, resolving conflicts.

In Git, this means running
`git checkout <FEATURE_BRANCH> && git rebase -i main` to enter an interactive
rebase of your feature branch against `main`, resolving any conflicts that
Git flags during the process before resolving the interactive rebase.

In Jujutsu, it means running `jj rebase -b <FEATURE_BOOKMARK> -d main`, and
then resolving any conflicted commits.

### Don't Merge From `main` to a Dev Branch

This is a common mistake when trying to resolve conflicts between a dev branch
and `main`. Don't do it. It makes for messy history compared to alternatives.

### Make Commits Logically Coherent

Commits matter. Give them clear, explanatory titles that follow the
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) spec,
and bodies that explain the reasoning behind a change, flag future issues that
will need addressing or other information that's important to reviewers to see.

Before submitting an MR, make sure the branch's commits represent logically
coherent chunks of work, each with a clear and appropriate commit message.

To help prepare commits before an PR in Git, use the new `git history` command,
added in Git 2.54, as needed. `git history reword` lets you modify commit
messages for commits, while `git history split` lets you easily split a
commit into two commits.

To do the same in `jj`, use `jj edit -r <CHANGE_ID>` to edit a commit message,
and use `jj split -r <CHANGE_ID>` to split a commit into two.

### Consider Git Trailers

Git trailers are a useful pattern for embedding extra information into your
commits, such as who helped you with the contributions or who has reviewed
the changes in advance of submitting the MR.

There's no formal list of Git trailers, but there are many common ones which
are [documented by the Git project][trailers], such as `Co-authored-by` or
`Helped-by` which can be useful to share appropriate credit.

For AI contributions, as documented in our [AI Policy](./ai-policy.md),
__DO NOT__ include a `Co-authored-by` trailer for any AI model you use.
Instead, describe your use of the AI tool in your Merge Request message.

[Rust Analyzer]: https://rust-analyzer.github.io/
[trailers]: https://git-scm.com/docs/SubmittingPatches#commit-trailers
