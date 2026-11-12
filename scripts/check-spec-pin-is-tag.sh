#!/usr/bin/env bash
#
# ASSERT THE PINNED SPEC COMMIT IS A PUBLISHED SPEC RELEASE.
#
# The spec is tagged first, on its own evidence; an engine then pins that tag
# and releases. So an engine release must implement a released spec, not a
# commit between two releases. Release-time only: between releases the pin
# moves freely, and check-spec-pin-ancestry.sh guards it in CI.
#
# Usage: check-spec-pin-is-tag.sh [submodule-path]
# Env:   SPEC_REMOTE (default: the submodule url from .gitmodules)
#
# Exit 0  a 0.x.y tag on the spec remote resolves to the pinned commit.
# Exit 1  no such tag - move the pin to a spec release tag before releasing.
# Exit 2  the check could not run (no submodule gitlink, remote unreachable).

set -euo pipefail

submodule_path="${1:-tests/spec}"

cd "$(git rev-parse --show-toplevel)"

pinned=$(git ls-tree HEAD "$submodule_path" | awk '$2 == "commit" { print $3 }')
if [ -z "$pinned" ]; then
  echo "CANNOT CHECK: no submodule gitlink recorded at '$submodule_path'." >&2
  exit 2
fi

remote="${SPEC_REMOTE:-}"
if [ -z "$remote" ]; then
  remote=$(git config -f .gitmodules --get "submodule.$submodule_path.url" || true)
fi
if [ -z "$remote" ]; then
  echo "CANNOT CHECK: no url for submodule '$submodule_path' in .gitmodules; set SPEC_REMOTE." >&2
  exit 2
fi

if ! refs=$(git ls-remote --tags "$remote"); then
  echo "CANNOT CHECK: could not list the tags of $remote." >&2
  exit 2
fi

# An annotated tag lists its object and its peeled commit (`^{}`); a
# lightweight tag lists the commit. Either names the pinned commit.
tags=$(printf '%s\n' "$refs" \
  | awk -v sha="$pinned" '$1 == sha { print $2 }' \
  | sed -e 's#^refs/tags/##' -e 's#\^{}$##' \
  | grep -E '^0\.[0-9]+\.[0-9]+$' | sort -u || true)

if [ -n "$tags" ]; then
  echo "ok: spec pin ${pinned:0:12} at '$submodule_path' is spec release $(echo "$tags" | paste -sd, -)."
  exit 0
fi

echo "::error::spec pin ${pinned:0:12} at '$submodule_path' is not a published spec release tag (0.x.y) on $remote." >&2
echo "Move the pin to the spec release this engine implements, then release." >&2
exit 1
