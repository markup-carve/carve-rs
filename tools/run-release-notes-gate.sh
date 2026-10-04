#!/usr/bin/env bash
# The pre-publish release-notes gate, shared by release.yml and rehearse-release-notes.yml.
# Usage: tools/run-release-notes-gate.sh <tag>  (reads GH_TOKEN and GITHUB_REPOSITORY)
set -eo pipefail

tag="$1"
release="$(gh api "repos/$GITHUB_REPOSITORY/releases?per_page=100" --paginate \
  | jq -cs --arg tag "$tag" '[.[][] | select(.tag_name == $tag)] | first // empty')"
if [ -z "$release" ]; then
  echo "::error::No release for $tag. Write its notes first."
  exit 1
fi
printf '%s' "$release" | python tools/check-release-notes.py \
  --tag "$tag" --repo "$GITHUB_REPOSITORY"
