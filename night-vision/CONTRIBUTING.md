# Contributing to Night Vision

Thanks for contributing to Night Vision. This project is in early development,
so expect some parts of the workflow to evolve. When in doubt, prefer the
guidance in the project docs and ask in the team channel before making a large
process change.

## Start Here

Before opening your first Merge Request (MR), read the onboarding material in
the [README](README.md#getting-started):

- [Dev Practices](docs/project/dev-practices.md), for team expectations around
  documentation, debugging, testing, commits, and version control.
- [AI Policy](docs/project/ai-policy.md), if you use AI tools while working on
  the project.
- [Backend docs](docs/backend/), if you are touching the Rust API, database,
  package analysis, or backend development tools.
- [Documentation index](docs/introduction.md), for the rest of the project
  documentation.

## Development Environment

Follow the [getting started instructions in the project README](README.md#getting-started)
to set up your development environment.

## Working on Changes

Keep changes focused and easy to review:

- Work on a short-lived branch or Jujutsu bookmark.
- Rebase on the latest `main` before opening or updating an MR.
- Do not merge `main` into your development branch.
- Use clear commit messages that follow
  [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
- Keep commits logically coherent. Split or reword commits before review when
  that makes the change easier to understand.
- Add tests for new behavior and regression tests for bug fixes.
- Update docs when behavior, setup, architecture, or developer workflow changes.

The detailed version-control guidance lives in
[docs/project/dev-practices.md](docs/project/dev-practices.md#version-control-git--jujutsu).

## Local Checks

Run the checks that match the part of the project you changed before opening an
MR. Run them from inside an activated Flox environment.

For backend changes:

```sh
cd backend
cargo xtask ci
```

Confirm `cargo xtask ci` completes cleanly before submitting an MR.

For frontend changes:

```sh
cd frontend
pnpm install
pnpm run check
pnpm run build
```

If you cannot run a relevant check, say so in the PR description and explain
why.

## Opening a Merge Request

Night Vision is hosted at
<https://github.com/mitre/hipcheck/night-vision>.

To open an MR:

1. Push your branch to GitHub:

   ```sh
   git push -u origin <branch-name>
   ```

2. Open the repository in GitHub:
   <https://github.com/mitre/hipcheck/night-vision>.
3. Use GitHub "Create merge request" flow for your pushed branch.
4. Set the target branch to `main`.
5. Give the PR a short, specific title. Conventional Commit style is preferred
   when it fits, for example `feat: add package subscription endpoint`.
6. In the description, include:
   - What changed.
   - Why the change is needed.
   - How you tested it, including commands run.
   - Any known limitations, follow-up work, or reviewer context.
   - How AI tools were used, if applicable, following
     [docs/project/ai-policy.md](docs/project/ai-policy.md).
7. Link any related GitHub issues.
8. Mark the PR as a draft if it is not ready for full review.
9. Request review from the appropriate project maintainers or teammates.

Before requesting review, make sure the PR is mergeable. If GitHub reports that
the branch is behind or has conflicts, rebase on the latest `main`, resolve the
conflicts locally, rerun relevant checks, and push the updated branch.

## Review Expectations

Review is part of the development process. Expect reviewers to ask about
behavior, tests, error handling, documentation, and maintainability.

When responding to review:

- Prefer follow-up commits while review is active.
- Resolve each thread only after addressing it or agreeing with the reviewer
  that no change is needed.
- Keep the PR description current if the scope or testing changes.
- Rebase and clean up commits before merge if maintainers request it.

For AI-assisted changes, you remain responsible for the submitted work. Review
the generated output carefully, remove any tool transcripts or logs, check for
secrets, and document AI usage in the PR as required by the
[AI Policy](docs/project/ai-policy.md).
