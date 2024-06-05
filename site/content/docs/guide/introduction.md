---
title: Introduction
---

# Introduction

Hipcheck is a tool for automatically assessing risks associated with software
repositories. It exists to make it possible for maintainers to identify their
riskiest dependencies and do their own reviews.

Hipcheck works by collecting metrics from Git logs, the APIs of common Git hosts
like GitHub, and language-specific tools like NPM and using them to detect risky
development practices and possible supply chain attacks.

## Why Use Hipcheck?

It's common to hear you need to audit your dependencies, less common to do it,
even less common to do it consistently. For projects that have said "we'll
do it when we have time," Hipcheck makes the problem easier to manage by
helping you target your review. Instead of reviewing 100 dependencies,
direct and transitive, you might review 5 that Hipcheck flags for further
investigation. When new versions come out, you run Hipcheck again and let it
tell you if anything has changed in the risk profile.

## How Does Hipcheck Compare to Alternatives?

Hipcheck fills a unique role in this space. The other major categories of tools
in this area are vulnerable version detectors, static code analyzers, and practices
analyzers.

### Vulnerable Version Detectors

These are things like GitHub's Dependabot or Snyk Open Source. These are
extremely useful tools for identifying vulnerable versions of your
dependencies, and you should use them or a similar alternative. What they
don't do is detect risks in a project's development practices, or possible
supply chain attacks like malicious contributions or typosquatted dependencies.

### Static Code Analyzers

Often you'll see projects try running static code analyzers like Fortify
Static Code Analyzer, Checkmarx SAST, or SonarQube against open source
they're considering incorporating. Static code analyzers are great tools
for identifying code weaknesses that may be true vulnerabilities; but
static code analysis techniques produce false positives, especially on code
not written with them in the process throughout development.

Applying static code analysis to open source dependencies _can_ find real
risks, but it requires a lot of work to filter through results, and often
requires building expertise in the internals of a library to assess
findings.

### Practices Analyzers

There is one other similar tool in this space, Scorecard, by the Open Source
Security Foundation. Scorecard tackles the same problem, and is a worthwhile tool
to try. There are definite differences to highlight between Scorecard and Hipcheck:

#### Configuration

Hipcheck is more configurable. You can override any thresholds and weights to
change when individual analyses will flag a repository, and how failing analyses
will contribute to the overall risk score.

#### Attack Detection

Hipcheck includes analyses to detect possible attacks like malicious
contributions, using statistical analysis of commit-level data to do the job.
