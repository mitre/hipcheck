---
title: "Low-Maintenance Projects and Contributor Risk"
weight: 14
slug: 0014
extra:
  rfd: 14
  primary_author: Andrew Lilley Brinker
  primary_author_link: https://github.com/alilleybrinker
---

# Low-Maintenance Projects and Contributor Risk

There are two valuable questions about open source software projects which
Hipcheck is unable to answer effectively today: is the project
"low-maintenance," and "how diverse is the contributor-base to the project"?

In this RFD, we outline what these questions _mean_ precisely, and describe
planned improvements in the form of new analysis plugins, improvements to
existing analysis plugins, and data-collection plugin improvements to enable
answering these questions.

## What does "low-maintenance" mean?

Today, Hipcheck has an "activity" analysis, provided by the `mitre/activity`
plugin, which checks when the last commit was made to the target source
repository. This is _useful_, but can be an incomplete picture of the
activity-level of a project.

For example, an open source software (OSS) project may be "low-maintenance,"
where it is not entirely _unmaintained_, but where it does not receive regular,
active updates, and where the maintainer or set of maintainers make no
commitments as to the frequency of updates or their responsiveness to issues.

This low-maintenance status is important to identify because it may indicate
that a project presents a greater-than-normal security risk, because
maintainers may not be willing or able to produce timely fixes in response to
security disclosures. In such a context, it would fall on users of the OSS code
to take action in the presence of disclosed zero-day vulnerabilities. This may
not inherently be a deal-breaker for all OSS consumers, but it is generally
worth being aware of the risk.

## How can we assess contributor-base diversity?

Contributor-base diversity is another key metric in understanding the long-term
risks associated with the use of an OSS package. "Diversity" here can be
assessed across multiple layers: how many "active" contributors are there in
total, and how many distinct organizations are behind the work of those
contributors?

First, "active" here may be contextual, but is trying to assess the degree to
which a project is at risk of becoming unmaintained if a single maintainer or a
small number of maintainers reduce or cease their contributions. An open source
project with an individually-diverse set of contributors is more resilient to
changes in contribution level by any one contributor. This is sometimes
morbidly referred to as a "bus factor," the risk to a project from a
contributor being hit by a bus, and thus not being available to contribute.

The second question will not matter for all projects, but can be critical for
projects where a substantial portion of the contributors engage with the
project as part of their employment. This is the most common form of
employer-support for open source software, and while the investment of funds in
the form of developer labor is a boon for the production of OSS, it also
presents a risk if that corporation's investment changes, or if the OSS
project's relationship to corporate strategic needs changes. So called "single
vendor" projects may be subject to sudden cessation of contribution or to
relicensing which impacts users' ability to continue using the OSS.

This corporation-level of diversity is important to assess separately from
individual-level contributor diversity because a project may _appear_ to have a
diverse set of individual contributors, but may in fact be subject to
substantial latent risk of becoming unmaintained or being relicensed if all of
those contributors are concentrated in a single organization.

## What improvements are necessary to implement these analyses?

With those concepts now described, we turn to the question of how to measure
the necessary metrics on open source projects to attempt producing credible
automated answers to these questions.

Let's begin with the second question: assessing the number of organizations
involved in an open source project. For this, we can turn to the "elephant
factor" metric defined by the Community Health Analytics in Open Source (CHAOS)
project, part of the Linux Foundation. The elephant factor of a project is
intended to identify the number of organizations whose paid labor provides some
threshold of contribution to a project. For example, a project with
an elephant factor of 2 at a threshold of 50% would mean the project has two
organizations whose paid contributors' provide 50% of the total contribution to
the project. Projects with low elephant factors at high thresholds have a
substantial reliance on a small number of organizations, meaning their ability
to continue effectively is tied to the continued investment of those
organizations.

To determine elephant factor, we need to identify contributor affiliation with
an organization. Thankfully, Hipcheck has an existing analysis, provided by the
`mitre/affiliation` plugin, to do this, which can also be extended. Currently,
the affiliation analysis looks at contributors' email addresses, and matches
them against a configurable list of known email hosts. However, there are more
signals we can and should incorporate. For example, GitHub profiles can
indicate a user's corporate affiliation through an employer field, and
individuals can also indicate affiliation by being part of a GitHub
Organization for their employer. Both of these pieces of information can be
gleaned from the GitHub API.

So for our plan of calculating the elephant factor for OSS projects, our first
step will be to enhance the `mitre/github` plugin to enable collecting this
organization information from users' GitHub profiles, and then to enhance the
`mitre/affiliation` plugin to use that additional data from Github when making
affiliation determinations.

Then, we can introduce a new "top-level" plugin (meaning one which can be used
directly in a Hipcheck policy file), tentatively called `mitre/critical-orgs`,
which implements the elephant factor calculation based on contribution volumes
from project Git commit histories, paired with organization affiliation
information from the enhanced `mitre/affiliation` plugin.

Together, these will provide the Minimum Viable Product (MVP) for identifying
projects with high corporate contributor risk.

For the other piece of assessing high reliance on individual contributors, that
can be done more directly, looking at individual contribution volume and
identifying high-leverage contributors whose contribution percentage is above
some threshold. This could be done with a new `mitre/critical-contributors`
"top-level" plugin.

Finally, for identifying low-maintenance projects, we can start by introducing
a new analysis to identify when a project has a defined security policy. This
could leverage three data sources: the presence of a `SECURITY.md` in the root
of a project's file hierarchy; for projects on GitHub, whether the project has
set up GitHub security reporting; whether a project's `README.md` contains a
security reporting section. These could be bundled in a new
`mitre/security-policy` "top-level" plugin, which would be used to infer
low-maintenance risk from a lack of a clear mechanism to report security
issues.

Given the above breakdowns, we have the following concrete steps:

1) Enhance the `mitre/github` plugin to get additional data from the GitHub
   API, including organization data from user profiles and security reporting
   data for source repositories.
2) Enhance the `mitre/affiliation` plugin to incorporate additional affiliation
   data sources beyond email hosts.
3) Introduce the new `mitre/critical-orgs` plugin to calculate the elephant
   factor for a project.
4) Introduce the new `mitre/critical-contributors` plugin to identify critical
   contributors for a project.
5) Introduce the new `mitre/security-policy` plugin to identify projects which
   lack a defined security policy.
