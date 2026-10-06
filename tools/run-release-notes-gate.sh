#!/usr/bin/env bash
# The pre-publish release-notes gate, shared by release.yml and rehearse-release-notes.yml.
# Usage: tools/run-release-notes-gate.sh <tag>  (reads GH_TOKEN and GITHUB_REPOSITORY)
set -eo pipefail

tag="$1"

# RESOLVE THE INTERPRETER, DO NOT NAME ONE. Hard-coding `python` made this
# wrapper CI-only: the name exists there because actions/setup-python shims it,
# and not on a machine carrying only `python3`, where the gate died with exit
# 127 and a message that reads like a broken checker (carve-rs#2344). Pinning
# `python3` instead would privilege the other environment just as arbitrarily,
# so both names are probed and `PYTHON` overrides. Resolving before the release
# lookup keeps the environment failure separate from the gate's own verdict.
python_bin="${PYTHON:-}"
if [ -z "$python_bin" ]; then
  for candidate in python3 python; do
    if command -v "$candidate" >/dev/null 2>&1; then
      python_bin="$candidate"
      break
    fi
  done
fi
if [ -z "$python_bin" ]; then
  echo "::error::No Python interpreter found. Looked for python3 and python on PATH; set PYTHON to name one."
  exit 1
fi

release="$(gh api "repos/$GITHUB_REPOSITORY/releases?per_page=100" --paginate \
  | jq -cs --arg tag "$tag" '[.[][] | select(.tag_name == $tag)] | first // empty')"
if [ -z "$release" ]; then
  echo "::error::No release for $tag. Write its notes first."
  exit 1
fi
printf '%s' "$release" | "$python_bin" tools/check-release-notes.py \
  --tag "$tag" --repo "$GITHUB_REPOSITORY"
