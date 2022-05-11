
# What Hipcheck Can Analyze

Hipcheck can analyze any Git repository, local or remote. However,
some details are worth mentioning.

Currently, review analysis is limited to GitHub repositories and
their local clones. Hipcheck knows how to talk to the GitHub API to
ask for relevant data, but doesn't yet know any other APIs.

GitHub will always make a local clone of any remote repository,
inside a hidden `.hipcheck` directory containing clones and, in
the future, cached data to speed up Hipcheck's operation.
