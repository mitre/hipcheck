---
title: Analyses
weight: 3
---

# Analyses

As suggested in the section on data, _analyses_ in Hipcheck are
about computations performed on the data Hipcheck collects, with the purpose
of producing _measurements_ about that data to which policies can be applied.

In general, analyses can be grouped into two broad categories:

- __Practice__: Analyses which assess the software development practices a
  project follows.
- __Attack__: Analyses which try to detect active software supply chain
  attacks.

To understand these, it's useful to ask: what is software supply chain risk?
In general, we understand software supply chain risk to be the collection of
risks associated with adopting third-party software dependencies. This may
include:

- __Intellectual property risk__: The risk that a project may re-license in
  a manner that prohibits or raises the cost of its use, may introduce
  trademarks which limit its use, may introduce patents which limit its use,
  or may fall victim to any intellectual-property-related issues with its
  own contributors or dependencies (for example, contributors revoking the
  license of their own prior contributions, or an outside party asserting that
  contributions made violate that party's own intellectual property rights;
  see the [SCO-Linux disputes][sco] for an example of this kind of problem).
- __Vulnerability risk__: The risk that a project may introduce vulnerabilities
  into its users. In general, we expect software of any kind to have defects,
  and use _assurance_ techniques like code review, testing, code analysis,
  and more to identify and remove defects, and thereby reduce code weaknesses
  and vulnerabilities in shipped code.

{% info(title="Weaknesses and Vulnerabilities") %}
It's worthwhile to be precise about "weaknesses" and "vulnerabilities" in
software. Both are important, but the distinction matters. To explain, we will
borrow definitions from the Common Weakness Enumeration (CWE) and Common
Vulnerabilities and Exposures (CVE) programs. CWE is a program for enumerating
a taxonomy of known software and hardware weakness types. CVE is a program for
tracking known software vulnerabilities.

Definition of "weakness":

> A 'weakness' is a condition in a software, firmware, hardware, or service
> component that, under certain circumstances, could contribute to the
> introduction of vulnerabilities.
> — [Common Weakess Enumeration](https://cwe.mitre.org/about/index.html)

Definition of "vulnerability":

> An instance of one or more weaknesses in a Product that can be exploited,
> causing a negative impact to confidentiality, integrity, or availability;
> a set of conditions or behaviors that allows the violation of an explicit
> or implicit security policy.
> — [Common Vulnerabilities &amp; Exposures](https://www.cve.org/ResourcesSupport/Glossary?activeTerm=glossaryVulnerability)
{% end %}


- __Supply chain attack risk__: The risk that a project may become the victim
  of a supply chain attack. These attacks exist on spectrums of targeting and
  sophistication, from extremes like the generally unsophisticated and
  untargeted [typosquatting attack](https://arxiv.org/pdf/2005.09535), to the
  highly sophisticated and highly targeted
  ["xz-utils" backdoor](https://en.wikipedia.org/wiki/XZ_Utils_backdoor).

In general, Hipcheck is _not_ concerned with intellectual-property risks,
as there exist many tools today that effectively extract licensing information
for open source software, analyze those licenses for compatibility and
compliance requirements, and report back to users to ensure users avoid
violating the terms of licenses and meet their compliance obligations. We do
not believe there's significant value for Hipcheck to re-implement these
same analyses.

However, Hipcheck _does_ care about vulnerability risk, which is what the
"practice" analyses are concerned with, and about supply chain attack risk,
which is the concern of the "attack" analyses.

In general, we believe that _most_ open source software will not be the
victim of supply chain _attacks_, at least currently. This may change in the
future if open source software supply chain attacks continue to become
more common. To quote the paper ["Backstabber’s Knife Collection: A Review of
Open Source Software Supply Chain Attacks"](https://arxiv.org/pdf/2005.09535)
by Ohm, Plate, Sykosch, and Meier:

> From an attacker’s point of view, package repositories represent a reliable
> and scalable malware distribution channel.

However, in the current landscape, users of open source software dependencies
are rightfully more concerned with the risk that their dependencies will
include vulnerabilities which have to be managed and responded to in the
future. This is what "practice" analyses intend to assess.

[sco]: https://en.wikipedia.org/wiki/SCO%E2%80%93Linux_disputes
