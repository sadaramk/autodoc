#!/usr/bin/env bash
# Run `nunki diff` for a pull request and keep exactly one comment on it.
#
#   usage: action-comment.sh <nunki-binary> <diff-args...>
#
# Four behaviours the obvious version of this gets wrong:
#
#   * It updates its own comment instead of appending another on every push, so
#     a ten-commit branch leaves one comment rather than ten.
#   * When nothing changed it says nothing, and removes a comment it left
#     earlier — a branch that added a route and then reverted it should not keep
#     claiming the route was added.
#   * It finds its comment by a hidden marker rather than by author, so it still
#     works when the workflow runs with a PAT or a GitHub App instead of
#     `github-actions[bot]`.
#   * It compares against the commit the pull request merges into, not `HEAD~1`,
#     which describes the last push rather than the branch.
#
# Reads NUNKI_PR, NUNKI_BASE_SHA, GITHUB_REPOSITORY and GH_TOKEN.
set -euo pipefail

MARKER='<!-- nunki:architecture-diff -->'

bin="${1:?usage: action-comment.sh <nunki-binary> <diff-args...>}"
shift

repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}"
pr="${NUNKI_PR:-}"
if [ -z "$pr" ]; then
  echo "nunki: no pull request number — comment: true only works on pull_request events" >&2
  exit 1
fi

if ! printf '%s ' "$@" | grep -q -- '--base'; then
  base="${NUNKI_BASE_SHA:-}"
  if [ -z "$base" ]; then
    echo "nunki: no --base given and no pull request base to fall back on" >&2
    exit 1
  fi
  set -- "$@" --base "$base"
fi

# `diff --exit-code` separates all three outcomes: 0 unchanged, 1 changed, 2 a
# real failure. Reading them apart is what lets an unchanged architecture stay
# silent without matching on our own prose.
asked_for_exit_code=false
if printf '%s ' "$@" | grep -q -- '--exit-code'; then
  asked_for_exit_code=true
else
  set -- "$@" --exit-code
fi

echo "nunki: diff $*"
changed=0
markdown=$("$bin" diff "$@") || changed=$?
if [ "$changed" -gt 1 ]; then
  printf '%s\n' "$markdown"
  exit "$changed"
fi

# `--paginate` matters: a busy pull request can push our comment past the first
# page, and not finding it is indistinguishable from never having posted it —
# which is how duplicates appear.
existing=$(
  gh api --paginate "repos/$repo/issues/$pr/comments" \
    --jq "map(select(.body | startswith(\"$MARKER\"))) | .[0].id // empty" |
    head -1
)

if [ "$changed" -eq 0 ]; then
  if [ -n "$existing" ]; then
    echo "nunki: architecture unchanged — removing the earlier comment"
    gh api --method DELETE "repos/$repo/issues/comments/$existing" --silent
  else
    echo "nunki: architecture unchanged — saying nothing"
  fi
  exit 0
fi

body="$MARKER
$markdown"

if [ -n "$existing" ]; then
  echo "nunki: updating comment $existing"
  gh api --method PATCH "repos/$repo/issues/comments/$existing" -f "body=$body" --silent
else
  echo "nunki: posting a new comment"
  gh api --method POST "repos/$repo/issues/$pr/comments" -f "body=$body" --silent
fi

# The comment is not a verdict. Only a caller who asked for `--exit-code` wanted
# an architecture change to fail the job.
if [ "$asked_for_exit_code" = true ]; then
  exit 1
fi
