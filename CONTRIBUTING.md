# Contributing

We're happy to accept contributions!

For bug fixes and minor changes to the implementation, feel free to open an
issue in the issue tracker explaining what you'd like to fix. If a project
maintainer hasn't responded after a few days, feel free to `@`-mention one of
us. We'll try to respond in a reasonable time frame.

If you have changes you'd like to propose, feel free to open a Pull Request!
That said, especially for larger changes, we recommend talking to us through
the issue tracker or Discussions page before spending too much time on
implementation. Hipcheck is under active development and changes we're working
on may conflict with or invalidate work you do without coordination.

You can see our [full roadmap] for more information on our current priorities.

We do expect changes to pass Continuous Integration testing prior to merge.
You can try out changes locally with `cargo xtask ci`, which will run a
battery of tests similar to our actual CI configuration. However, this command
is not _guaranteed_ to be identical to our CI tests, and it's the official CI
tests which have to pass for a merge.

[full roadmap]: https://github.com/orgs/mitre/projects/29
