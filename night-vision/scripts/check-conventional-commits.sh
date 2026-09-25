#!/bin/sh

# Check that non-merge commits in the current CI commit range use
# Conventional Commits subject lines.

set -eu

zero_sha=0000000000000000000000000000000000000000
pattern='^[a-z][a-z0-9-]*(\([A-Za-z0-9._/-]+\))?!?: .+'

error() {
    printf '%s\n' "$1" >&2
}

commit_range() {
    if [ "${CI_PIPELINE_SOURCE:-}" = merge_request_event ]; then
        base=${CI_MERGE_REQUEST_DIFF_BASE_SHA:-}
        if [ -z "$base" ] || [ "$base" = "$zero_sha" ]; then
            error 'error: CI_MERGE_REQUEST_DIFF_BASE_SHA is not set'
            exit 1
        fi

        printf '%s..%s\n' "$base" "${CI_COMMIT_SHA:?CI_COMMIT_SHA is not set}"
        return
    fi

    before=${CI_COMMIT_BEFORE_SHA:-}
    if [ -n "$before" ] && [ "$before" != "$zero_sha" ]; then
        printf '%s..%s\n' "$before" "${CI_COMMIT_SHA:?CI_COMMIT_SHA is not set}"
        return
    fi

    printf '%s\n' "${CI_COMMIT_SHA:?CI_COMMIT_SHA is not set}"
}

range=$(commit_range)

if printf '%s\n' "$range" | grep -F '..' >/dev/null; then
    commits=$(git rev-list --reverse --no-merges "$range")
else
    if git show --no-patch --format=%P "$range" | grep ' ' >/dev/null; then
        commits=
    else
        commits=$range
    fi
fi

if [ -z "$commits" ]; then
    printf '%s\n' 'No non-merge commits to check.'
    exit 0
fi

failed=false

for commit in $commits; do
    subject=$(git show --no-patch --format=%s "$commit")
    if ! printf '%s\n' "$subject" | grep -Eq "$pattern"; then
        if [ "$failed" = false ]; then
            error 'Commit messages must use Conventional Commits format:'
            error '  type(scope)!: subject'
            error ''
            error 'Invalid commit subjects:'
        fi

        short_commit=$(git rev-parse --short "$commit")
        error "  $short_commit $subject"
        failed=true
    fi
done

if [ "$failed" = true ]; then
    exit 1
fi

printf '%s\n' 'All checked commit messages use Conventional Commits format.'
