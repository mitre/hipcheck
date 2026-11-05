# Issue Tracker Standards

Night Vision uses GitHub issues to describe, prioritize, and track project
work. Issues should make work easier to pick up, review, and close. Prefer
clear, specific issues over broad placeholders.

[[_TOC_]]

## Goals

Issue tracker organization should help the team:

- Understand the problem before choosing an implementation.
- Find work by area, type, status, and priority.
- Keep Merge Requests focused and reviewable.
- Link implementation work back to concrete acceptance criteria.
- Preserve useful context without turning issues into design documents.

## When to Create an Issue

Create an issue when the work needs tracking, discussion, prioritization, or a
follow-up MR. Small changes may skip an issue when they are obvious, tightly
scoped, and do not need separate planning.

Good issue candidates include:

- Bugs or regressions.
- New user-visible behavior.
- Backend API, database, or package-analysis changes.
- Frontend workflow or UI changes.
- Developer tooling, CI, or local setup improvements.
- Documentation work that affects team workflow or project understanding.
- Research tasks where the outcome should guide later implementation.

Avoid creating issues that are only reminders with no clear outcome. If the
desired result is not known yet, create a research issue and make the expected
decision or written finding explicit.

## Issue Size

Most implementation issues should be small enough to close with one focused
Merge Request. Split issues when they contain unrelated areas, unclear ordering,
or multiple outcomes that could be reviewed independently.

Use larger tracking issues only when they coordinate several concrete child
issues. A tracking issue should list the child work and describe the overall
outcome; it should not be the only place where implementation details live.

## Required Fields

Every issue should have:

- A short, specific title.
- A description using the standard template, unless the issue is intentionally
  minimal.
- At least one `Type::` label.
- At least one `Area::` label.
- One status label.
- A priority label after triage.

Issues that are missing required labels or enough context to start work should
use `Status::Needs-Triage`.

## Titles

Use direct titles that describe the outcome or problem. Prefer:

- `Add package version assessment endpoint`
- `Document local Docker secret staging`
- `Fix invite retry after email delivery failure`

Avoid vague titles:

- `Backend work`
- `Clean up`
- `Bug`
- `Frontend stuff`

Conventional Commit style is acceptable for issue titles when it reads well,
but it is not required.

## Labels

Use labels to make the tracker searchable and to support lightweight triage.
Avoid label sets that require maintainers to debate fine distinctions before
work can begin.

### Type Labels

Each issue should have one primary type label:

- `Type::Bug`: broken, incorrect, or regressed behavior.
- `Type::Feature`: new product or user-visible behavior.
- `Type::Chore`: maintenance, cleanup, dependency, or infrastructure work.
- `Type::Docs`: documentation-only work.
- `Type::Research`: investigation where the main output is a decision,
  recommendation, or written finding.

### Area Labels

Use one or more area labels to show where the work belongs:

- `Area::Backend`
- `Area::Frontend`
- `Area::Database`
- `Area::Dev-Tools`
- `Area::Docs`
- `Area::CI`
- `Area::Security`

Add new area labels only when the existing set cannot describe recurring work.

### Status Labels

Use exactly one status label in normal triage:

- `Status::Needs-Triage`: the issue needs more detail, labels, priority, or
  scope decisions.
- `Status::Ready`: the issue is clear enough for someone to start work.
- `Status::Blocked`: work cannot continue until a named dependency is resolved.

When marking an issue blocked, describe the blocker in the issue description or
a comment.

### Priority Labels

Use priority labels to describe ordering pressure:

- `Priority::High`: important to current milestones, users, security,
  reliability, or team velocity.
- `Priority::Medium`: useful work with no special urgency.
- `Priority::Low`: nice-to-have, cleanup, or speculative work.

Priority is not severity. A severe bug may be low priority if it affects
unsupported behavior; a small tooling issue may be high priority if it blocks
the team.

### Helper Labels

Use helper labels sparingly:

- `good-first-issue`: suitable for a new contributor with clear acceptance
  criteria and limited project context.
- `help-wanted`: useful work that maintainers are comfortable delegating.

Do not use helper labels as a substitute for a complete description.

## Milestones

Use milestones for coherent workstreams or releasable slices. Avoid creating
milestones for vague themes or single issues unless the milestone represents a
real delivery target.

Suggested early project milestones:

- `MVP Backend Foundations`
- `MVP Frontend Foundations`
- `Package Analysis Pipeline`
- `Developer Experience`
- `Hardening / Reliability`

Milestones should contain issues that move toward a shared outcome. If a
milestone becomes a miscellaneous backlog, split or rename it.

## Triage Process

During triage:

1. Confirm the issue has a clear problem and desired outcome.
2. Apply one `Type::` label.
3. Apply one or more `Area::` labels.
4. Set `Status::Needs-Triage`, `Status::Ready`, or `Status::Blocked`.
5. Set a priority label.
6. Assign a milestone when the issue belongs to a current workstream.
7. Split, merge, or close issues that are too broad, duplicate, or obsolete.

Issues should move to `Status::Ready` only when a contributor can understand
what done means without asking for the basic scope.

## Issue Template

Use this template for normal implementation, bug, documentation, and tooling
issues. Delete sections that truly do not apply, but keep acceptance criteria
whenever possible.

```md
## Problem

What is wrong, missing, confusing, or worth improving?

## Desired Outcome

What should be true when this issue is complete?

## Scope

What is included?

What is explicitly out of scope?

## Implementation Notes

Relevant files, docs, APIs, constraints, prior decisions, or suspected approach.

## Acceptance Criteria

- [ ] Concrete, checkable result.
- [ ] Tests are added or updated when behavior changes.
- [ ] Documentation is updated when behavior, setup, architecture, or workflow
      changes.
- [ ] The closing PR links this issue.
```

## Research Issue Template

Use this template when the issue is for investigation rather than direct
implementation.

```md
## Question

What decision, unknown, or tradeoff needs investigation?

## Context

What prompted the research? Link relevant issues, MRs, docs, or external
references.

## Constraints

What project, security, operational, or compatibility constraints matter?

## Expected Output

What should this issue produce? For example: recommendation, design note,
prototype, RFD, implementation issue list, or no-go decision.

## Acceptance Criteria

- [ ] Findings are summarized in the issue, a linked doc, or a linked RFD.
- [ ] Recommended follow-up issues are created or linked.
- [ ] The issue records any decision made and who reviewed it.
```

## Closing Issues

Close an issue when the desired outcome and acceptance criteria are met, or
when the team decides the work should not be done. Link the closing PR when
implementation work is involved.

When closing without implementation, leave a short comment explaining why. Good
reasons include duplicate work, changed product direction, obsolete context, or
a decision captured elsewhere.
