# Hipcheck &#x2713;

[![License: Apache-2.0](https://img.shields.io/github/license/mitre/hipcheck)][license]
[![GitHub Release](https://img.shields.io/github/v/release/mitre/hipcheck)][release]

__Go from hundreds of dependencies you can't review, to just a few you can!__

Managing the security risk of third-party software at scale is difficult.
Normal projects can easily have hundreds of dependencies; far too many to
review by hand.

Hipcheck is designed to help you filter that list of dependencies down to just
a few that appear concerning, and to give you the information you need to make
a security decision quickly.

Hipcheck is a command line interface (CLI) tool for analyzing open source
software packages and source repositories to understand their software supply
chain risk. It analyzes a project's _software development practices_ and
detects _active supply chain attacks_ to give you both a long-term and
immediate picture of the risk from using a package.

## Very Quick Explanation

Hipcheck can analyze Git source repositories and open source packages from
popular package hosts.

```sh
# Analyze Express, a popular JavaScript package for web servers, with the
# URL of its Git repository.
hc check https://github.com/expressjs/express

# Analyze urllib3 version 2.2.2, a popular URL-handling package hosted on PyPI.
hc check -t pypi urllib3@2.2.2

# Analyze the package described by an SPDX Software Bill of Materials.
hc check example-sbom.spdx.json
```

## Installation

__If installing locally:__ run the install script from the
[latest release][release], then run __`hc setup`__.

__If running as a container:__ use a [Hipcheck image from
Docker Hub][docker].

## Values

Hipcheck's product values are to be:

* __Configurable:__ Hipcheck should be adaptable to the policies of its users.
* __Fast:__ Hipcheck should provide answers quickly.
* __Actionable:__ Hipcheck should empower users to make informed security
  decisions.

Read more about Hipcheck's product and project values in [RFD #2][rfd_2].

## License

Hipcheck's software is licensed under the Apache 2.0 license, which can be
found in the [`LICENSE`](LICENSE) file in this repository.

## Public Release

> [!NOTE]
> Approved for Public Release; Distribution Unlimited. Public Release Case
> Number 22-2145.
>
> Portions of this software were produced for the U.S. Government under
> Contract No. FA8702-19-C-0001 and W56KGU-18-D-0004, and is subject to the
> [Rights in Noncommercial Computer Software and Noncommercial Computer
> Software Documentation Clause DFARS 252.227-7014 (FEB 2014)][dfars].

[dfars]: https://www.acquisition.gov/dfars/252.227-7014-rights-other-commercial-computer-software-and-other-commercial-computer-software-documentation.
[docker]: https://hub.docker.com/r/mitre/hipcheck
[install]: https://mitre.github.io/hipcheck/install
[license]: https://github.com/mitre/hipcheck/blob/main/LICENSE
[release]: https://github.com/mitre/hipcheck/releases/latest
[rfd_2]: https://mitre.github.io/hipcheck/rfds/0002
[website]: https://mitre.github.io/hipcheck
