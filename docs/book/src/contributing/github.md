
# GitHub

Hipcheck relies on GitHub for providing information about pull request reviews
for review analysis. Currently, GitHub is the only remote Git host for which this
form of analysis is supported.

This analysis currently works via calls to version 3 of the GitHub API, which is
a REST API. Hipcheck is authenticated to the GitHub API using a token provided
by the user as part of their Hipcheck configuration.

Hipcheck only needs permissions for accessing public repository data, so those 
are the only permissions to assign to your generated token.
