# Security Policy

The following is the security policy for the `hipcheck` binary crate found in this workspace.

## Reporting a Vulnerability

You can report a vulnerability using the "Report a Vulnerability" button under
the "Security" tab of the repository. If we find that a vulnerability is legitimate,
we will create a [RustSec](https://rustsec.org/) advisory.

Please give us 90 days to respond to a vulnerability disclosure. In general, we
will try to produce fixes and respond publicly to disclosures faster than that.

We ask that you _not_ create advisories yourself; instead please submit
vulnerability reports to us first so we can plan a response. If we accept the
legitimacy of a vulnerability, please wait for us to respond publicly to the
vulnerability before publicly disclosing the vulnerability yourself.

Our response will include publication of new versions, yanking of old versions,
and public disclosure of the vulnerability and its fixes in the RustSec database.

We consider soundness violations (violations of safe Rust's memory, thread, or
type safety guarantees) to be at least informational vulnerabilities and
will treat them as such.

RustSec advisories are automatically imported into the GitHub Security Advisory
system and the OSV database, so you do not need to create duplicate reports for
those systems.