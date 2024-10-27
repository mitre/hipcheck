---
title: "Quickstart: Your First Analysis"
weight: 3
---

# Quickstart: Your First Analysis

With Hipcheck installed, let's use it to analyze something! To do that, we'll
use the `hc check` subcommand. This command takes a "target" (like a package
from a popular open source package repository, or a Git source repository like
on GitHub or GitLab) and analyzes it to understand its _practices_ — how the
people making the software do their work — and possible _attacks_.

Let's start by running Hipcheck against the Hipcheck repository!

```sh
$ hc check https://github.com/mitre/hipcheck
```

If you do this, you should see output somewhat like the following:

```
  Analyzing https://github.com/mitre/hipcheck
       Done loading configuration and data files
5.738s Done resolving git repository source
1.249s Done analyzing and scoring results

 Analyzed https://github.com/mitre/hipcheck (e662147)
          using Hipcheck 3.3.2
          on Tue July 2, 2024 at 3:11pm

  Passing
        + has been updated recently
          updated 0 weeks ago, required in the last 71 weeks

        + no concerning contributors
          0 found, 0 permitted

        + no concerning binary files
          0 found, 0 permitted

        + few unusually large commits
          1.41% of commits are unusually large, 2.00% permitted

        + no unusual-looking commits
          0.00% of commits are unusual-looking, 0.00% permitted

  Failing
        - commits too often applied by the author
          28.00% of commits merged by author, 20.00% permitted

  Errored
        ? fuzz analysis error: failed to get response from fuzz check
          unable to query fuzzing info
          unable to search fuzzing information; please check the HC_GITHUB_TOKEN system environment variable
          unable to query fuzzing info

        ? review analysis error: failed to get pull request reviews
          failed to get pull request reviews from the GitHub API, please check the HC_GITHUB_TOKEN system environment variable
          https://api.github.com/graphql: status code 401

        ? typo analysis error: failed to get dependencies
          can't identify a known language in the repository

Recommendation
     PASS risk rated as 0.17, acceptable below or equal to 0.50
```

## Breaking Down Hipcheck's Output

Let's break that output up into parts so we can understand it better.

### Progress Report

First, there's the _progress reporting_:

```
  Analyzing https://github.com/mitre/hipcheck
       Done loading configuration and data files
5.738s Done resolving git repository source
1.249s Done analyzing and scoring results
```

This says:

- What "target" Hipcheck is analyzing (in our case, the Hipcheck source
  repository on GitHub)
- That Hipcheck successfully loaded its configuration and data files
- That it successfuly found the Git repository (and how long that took)
- That it analyzed and scored the results

Already, we've learned that Hipcheck both _analyzes_ and _scores_ a target.
This means Hipcheck runs a variety of individual analyses, and uses them to
produce an overall score. We'll understand better later on.

One thing to note which may not be clear in the final output is that
Hipcheck clones (or pulls the latest if it's already cloned) a local copy
of the source repository being analyzed. Hipcheck needs to analyze metadata
associated with the project's Git history, and it's much faster to clone the
repository up-front and then analyze that history locally than it would be to
try to use the GitHub API or some other mechanism to analyze it remotely.

### The "Analyzed" Block

Next, we have the "Analyzed" block:

```
Analyzed https://github.com/mitre/hipcheck (e662147)
         using Hipcheck 3.3.2
         on Tue July 2, 2024 at 3:11pm
```

This again says what target Hipcheck analyzed, this time with some extra
information in parentheses. This is the `HEAD` commit hash from Git, which
tells us what the exact "most recent" commit was for the target source
repository at the time of the analysis.

Hipcheck also says what version was used for the analysis, and when the
analysis took place. This is important because Hipcheck results may not
be able to be compared across different versions of the tool, as analyses
may improve over time.

Then, we have three major blocks of results: "Passing," "Failing," and
"Errored."

### The "Passing" Block

The "Passing" block looks like this:

```
Passing
      + has been updated recently
        updated 0 weeks ago, required in the last 71 weeks

      + no concerning contributors
        0 found, 0 permitted

      + no concerning binary files
        0 found, 0 permitted

      + few unusually large commits
        1.41% of commits are unusually large, 2.00% permitted

      + no unusual-looking commits
        0.00% of commits are unusual-looking, 0.00% permitted
```

This shows the results of an analysis which "passed," which means they did not
find anything concerning to report to you! For each analysis, they give some
information about what was analyzed, and what the conclusion was. In this
case, we see 5 analyses in the "Passing" block:

- __Activity__: is the project actively maintained?
- __Affiliation__: are there are contributors affiliated with a known
  organization of concern?
- __Binary__: are there any binary files checked into the source repository?
- __Churn__: does the project have unusually large commits in its history?
- __Entropy__: are there commits which are textually unusual in the project's
  history?

These names aren't shown in the normal Hipcheck output, but they are the names
used in the Hipcheck configuration files, and it's how the project generally
refers to them.

Each of these analyses calculate a value and compare it to a configurable
threshold, and both the value and the threshold are reported in the output.

### The "Failing" Block

The "Failing" block looks like this:

```
Failing
      - commits too often applied by the author
        28.00% of commits merged by author, 20.00% permitted
```

This shows that 28% of the commits to the Hipcheck repository are merged by
the person that wrote them, which is an indicator that the project may not
be practicing consistent code review. We know that code review is a generally
valuable practice for increasing assurance in software, so not doing it, or
not doing it well, is something we care about for supply chain risk!

Sometimes, analyses will also report additional information we calls
"concerns," which give more specific information about what an analysis found.
Concern reporting is useful if, after running Hipcheck, you decide to manually
review the software as well. If reported concerns include specific commits to
review, for example, you now have a place to start! The "Identity" analysis
being done here doesn't report additional concerns though.

### The "Errored" Block

The "Errored" block looks like this:

```
Errored
      ? fuzz analysis error: failed to get response from fuzz check
        unable to query fuzzing info
        unable to search fuzzing information; please check the HC_GITHUB_TOKEN system environment variable
        unable to query fuzzing info

      ? review analysis error: failed to get pull request reviews
        failed to get pull request reviews from the GitHub API, please check the HC_GITHUB_TOKEN system environment variable
        https://api.github.com/graphql: status code 401

      ? typo analysis error: failed to get dependencies
        can't identify a known language in the repository
```

This block collects analyses that did not run to completion, and tries to
report information so you can understand _why_ they didn't finish, and try to
correct any problems if they're correctable.

Analyses may error out because of issues like missing tokens or insufficient
token permissions, or they might error out because the target source
repository can't be analyzed in a specific way.

In this output, we see examples of both! The "Fuzz" and "Review" analyses have
both failed with an error message about needing a GitHub API token. That's
because both of these analyses use the GitHub API as a data source.

{% info(title="Hipcheck and GitHub") %}
It's worth noting here that Hipcheck can analyze _any_ Git repository, not
just ones hosted on GitHub. If a repository is _not_ on GitHub, then analyses
which require the GitHub API will be skipped. You also don't have to give a
GitHub URL specifically to use the analyses which need the GitHub API. If you
give a path to a local repository with a remote for the default branch which
is on GitHub, Hipcheck will detect that and use the GitHub API. If you give
a package from a package repository like NPM, PyPI, or Maven and that package
has a GitHub repository associated with it, Hipcheck will detect that as well.
{% end %}

If you want these analyses to run, you need to set the `HC_GITHUB_TOKEN`
environment variable to a token you've gotten from GitHub. For analyzing
public repositories, that token only needs to have permission to read public
repos. If you want to analyze a _private_ repository, the token will need
permission to access that repository. You can read more about [managing your
GitHub API tokens](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)
in the GitHub documentation.

The other analysis which errored-out is the "Typo" analysis, which tries
to analyze the dependencies of the target for possible typosquatting.
Typosquatting is a type of software supply chain attack where attackers
create a malicious package with a name which is very similar to the name of
an existing legitimate package, in the hope that some unsuspecting users will
type the name wrong and use the malicious package on accident. Currently,
Hipcheck only supports typosquatting analysis for JavaScript packages using
the NPM package host. We'd like to grow that support in the future. In this
case, Hipcheck is a Rust project, not a JavaScript project, so typosquatting
analysis can't complete.

### The Recommendation

Finally, we have Hipcheck's "Recommendation." Hipcheck will only ever
recommend one of two possibilities:

- __Pass__: Use the software, as it's considered low risk.
- __Investigate__: Manually investigate the software further, as it has some
  concerning analysis results.

Hipcheck never "fails" a piece of software; we believe a true rejection can
only be done by a human reviewing the software in question.

The recommendation Hipcheck makes is based on calculating a "risk score" from
the individual analysis results, and comparing it to the user's configured
"risk tolerance." Both the score and tolerance are always values between 0
and 1.

The weighting applied to each analysis in producing the risk score is
configurable, and you can also turn off analyses entirely if you aren't
interested in their results. You can also change how specific analyses work,
and configure your risk tolerance to your liking as well. In general, Hipcheck
is designed to adapt to your own policies and risk considerations, not the
other way around. More about this is covered in the Hipcheck
[Complete Guide](/docs/guide).

In this case, Hipcheck has given itself a `PASS` (would be concerning if it
didn't)! This means that, by Hipcheck's estimation, you should feel comfortable
using it without further manual investigation.

## Conclusion

With that, we've completed the Quickstart guide to Hipcheck! If you'd like
to understand more about Hipcheck's underlying concepts, how it works under the
hood, how to configure it, how to interpret the results, and more, we recommend
reading the [Complete Guide](/docs/guide).

If you have questions about using Hipcheck, feel free to ask them on our
[Discussions](https://github.com/mitre/hipcheck/discussions) forum.
