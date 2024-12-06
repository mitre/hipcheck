---
title: Hipcheck's Values
weight: 2
slug: 0002
extra:
  rfd: 2
  primary_author: Andrew Lilley Brinker
  primary_author_link: https://github.com/alilleybrinker
  status: Accepted
  pr: 70
---


# Hipcheck's Values

Often, the _values_ associated with an open source software project are left
implicit rather than explicit. They form based on the beliefs and expectations
of the original contributors, and are reflected in the practices and
priorities of the project, but are not written down. The problem with this is
that values are central to successful collaboration, which requires shared
goals. Any open source software project relies on having a variety of
interested contributors who are rowing in the same direction. When values
are implicit, novel situations may arise which reveal deep divergence in
values between those contributors, leading to crises of governance and
project ownership that hurt the project overall.

To help avoid this fate, we (the current maintainers of Hipcheck) would like
to enumerate our values publicly, as a commitment to what we care about,
and to help others who are interested in contributing assess whether they
want the same things we want out of the project. Hipcheck as a project is
welcome to anyone, but we do believe that the greatest success will come
from contributors who share all or most of the values we'll enumerate here.

We'll be describing the values of both Hipcheck _the product_ and Hipcheck
_the project_. The product is the actual Hipcheck Command Line Interface
program we're building and which is shipped to end users. The project is
the collection of people and practices that are used to produce the product.

It's also important to note that _values_ may sometimes come into tension
with one another, and have to be traded off. There are no hard and fast
rules for how we will do this on the Hipcheck project, and they will be
done on a case-by-case basis.

Finally, we should give credit to the Oxide Computer Company's [list of
values][oxide_values], and their frank and public discussions about that
list, which have inspired our enumeration of values here.

## Product Values

- __Configurable__: We want Hipcheck to be flexible to meet the needs of
  people using it. Different organizations have different policies,
  different threat models, and different levels of risk tolerance. Hipcheck
  should be configurable to support that variety of needs.
- __Fast__: We want Hipcheck to be fast. In order for a tool to be used often,
  it needs to not get in the way of action for the people using it. That means
  Hipcheck needs to provide answers to users quickly.
- __Actionable__: Hipcheck's analyses are only as useful as the actions they
  enable our users to take. In addition to providing an overall recommendation
  about whether to furthur investigate a specific package, Hipcheck should
  also provide more detailed insights into specific areas of concern.

## Project Values

- __Candor__: Being forthright, even when it’s difficult. Respecting those
  who speak candidly, even if we disagree.
- __Courage__: Being hold, willing to do things that are unconventional,
  difficult, scary, or unproven. Not foolhardy or contrarian; decisions
  come from well-informed conviction.
- __Curiosity__: Being lifetime learners, unafraid of learning something new,
  no matter how intimidating or strange.
- __Diversity__: Believing the best results come from combining different
  perspectives and uniting them with shared values and mission.
- __Empathy__: Believing that to bring value to others, we must see the world
  through others’ eyes. Letting empathy guide our engineering and interactions
  with colleagues and users.
- __Humor__: Knowing that while the work is serious, we can’t take ourselves
  too seriously. Keeping things light even when problems are hard.
- __Resilience__: Persisting even when problems are hard, pushing through
  despite challenges or setbacks.
- __Responsibility__: Doing things larger than ourselves; not seeking to merely
  fulfill obligations, but to find new ways to help.
- __Rigor__: Believing systems must be correct above all else; being disciplined
  and through and insisting on getting to the roots of issues.
- __Teamwork__: Being intensely team-oriented; drawing strength and inspiration
  from the people we’re lucky to work with.
- __Urgency__: Being focused in how we approach our tasks, knowing we have
  finite resources and limited time; moving deliberately rather than hastily.

[oxide_values]: https://oxide.computer/principles#values
