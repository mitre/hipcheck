---
title: Why Hipcheck?
weight: 2
---

# Why Hipcheck?

To understand how Hipcheck works, it's first useful to understand why Hipcheck
was created.

## How Hipcheck Began

Hipcheck started as an internal project at MITRE. MITRE is a not-for-profit
corporation in the United States that operates Federally Funded Research and
Development Centers (FFRDCs). In practice, this means that MITRE serves as
a trusted advisor to many parts of the US federal government, providing
analysis, making recommendations, engaging in research, and building
prototypes in support of government missions.

Hipcheck began in 2019 when one of MITRE's sponsors asked for an assessment
of some open source software they were interested in using. MITRE built
Hipcheck as an attempt to answer that question, because they viewed existing
techniques for evaluating open source software in a manual or
automation-supported way to be inadequate.

Work on Hipcheck continued within MITRE from them, and Hipcheck was released
as an open source project under the Apache 2.0 license in January of 2023.

## The Problems with Existing Techniques

Before Hipcheck was created, the common mechanisms seen for managing supply
chain risk associated with open source software included:

- Manual review of a project, including their practices, code quality,
  history of vulnerabilities and vulnerability response, level of activity,
  assurance practices like code review or testing, and more.
- Analyzing the software with a static code analysis tool, depending on the
  language used and the types of tools available. This included both open
  source "linters" (generally, less sophisticated static analyzers often
  focused on code quality) and commercial analyzers.
- Analyzing the software with dynamic analysis tools, if possible. Again, the
  possibilities here are influenced by the language, toolchains required,
  difficulty of establishing a build, and difficulty of setting the project
  up for dynamic analysis.

In practice, each of these approaches had substantial challenges.

### The Problems with Manual Review

Manual review, while the most informative, was also the most time consuming.
In order to assess a project's history, reviewers may need to manually go
through extensive lists of prior contributions. Understanding code review
practices may only be based on a brief survey of contributions, and the same
would often be true with assessing testing practices. Human errors were
common, especially when reviewing code in an unfamiliar programming language
or in languages where idioms may very significantly from codebase to codebase.

### The Problems with Static Code Analysis

Static code analysis, while automated, had the challenge of often producing
large numbers of false positive results, especially on codebases which did
not themselves have a regular practice of running static code analyzers.
Static analyses are inherently _conservative_, they flag code patterns which
the analyzer can't prove not to be problematic. Often, this would result in
large numbers of benign findings which then needed to be manually reviewed.
Even after an initial review, full conclusions on the validity of specific
findings might need to involve consultation with the original maintainers
of the project being analyzed, or rely on a judgment call in the absence of
that more expert consultation.

### The Problems with Dynamic Code Analysis

Dynamic code analysis is valuable for avoiding false positives, but has
several difficulties of its own. First, "wiring up" a project to be
analyzed by a dynamic analysis tool like a fuzzer or a symbolic execution
system may be tedious and difficult, especially for an unfamiliar open source
project you are assessing. Dynamic analysis is also inherently probabilistic;
for example in fuzzing you can increase the confidence in the assurance of
the code by running the fuzzer for longer, but never eliminate the possibility
that something severe would have been found if you'd waited even one more
second before stopping the analysis.

## An Alternative Approach

While all of these are useful assurance techniques, they were not necessarily
the right techniques to use in the context of this specific question: __should
I feel comfortable using this open source software?__

Hipcheck was developed to test out an alternative approach. Instead of
analyzing the _code_ in a project like a static code analyzer would, Hipcheck
analyzes project _metadata_, like the commit history and platform API data
for packages, pull requests, and more, to make inferences about the _practices_
a project follows to produce its software.

In the time since that initial effort, Hipcheck has continued to grow and
improve, gaining more analyses and becoming a production-ready tool for
analyzing software packages for supply chain risk. Throughout that time,
it's number one goal has been and continues to be to empower producers and
users of open source software to understand the risks associated with a
project before using it, in a way that is sensible, maintains low false
positives, and adapts to the needs of the user.

We still believe that the other techniques described above are useful,
and in general we highly recommend them in other contexts! Manual review
is a great thing to do after letting Hipcheck filter your list of all
dependencies to specifically the ones that look the most concerning.
Static code analysis is a great thing to do for your own code, and
Hipcheck itself is written in [Rust](https://rust-lang.org), a language
with pretty strong static analysis built into it. Dynamic code analysis
is a wonderful set of techniques for finding real bugs and vulnerabilities,
as shown by the track record of groups like [Fish in a Barrel](https://fishinabarrel.github.io),
a security research team who run fuzzers against open source code written
in C and C++.
