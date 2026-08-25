#!/usr/bin/env bash
#
# ASSERT THE PINNED SPEC COMMIT IS REACHABLE FROM THE SPEC'S DEFAULT BRANCH.
#
# A submodule resolves by SHA, so a pin can point at a commit that is not on
# the spec's default branch and nothing notices: the tree still fetches while
# something in the remote keeps the object alive, CI checks it out, the corpus
# runs against it, and the job goes green. That is exactly what happened -
# all three engines sat on `963510b6`, the PRE-SQUASH branch head of a spec PR
# whose squashed form landed on main as a different commit. The content was
# equivalent, so no test could see it. Once the branch is deleted and the
# commit is garbage-collected, CI fails at CHECKOUT instead, with an error
# that points nowhere near the pin (markup-carve/carve#1740).
#
# This is the check that asks the question no other gate asks.
#
# Usage: check-spec-pin-ancestry.sh [submodule-path]
# Env:   SPEC_DEFAULT_BRANCH (default: main)
#
# Exit 0  the pin is an ancestor of the default branch.
# Exit 1  the pin is NOT an ancestor - a real finding, fix the pin.
# Exit 2  the check could not run (no network, no submodule, no history).
#         DELIBERATELY DISTINCT: a check that cannot reach the remote has not
#         detected a dangling pin, and must never be read as if it had.

set -euo pipefail

submodule_path="${1:-tests/spec}"
default_branch="${SPEC_DEFAULT_BRANCH:-main}"

cd "$(git rev-parse --show-toplevel)"

pinned=$(git ls-tree HEAD "$submodule_path" | awk '$2 == "commit" { print $3 }')
if [ -z "$pinned" ]; then
  echo "CANNOT CHECK: no submodule gitlink recorded at '$submodule_path'." >&2
  echo "Pass the submodule path as the first argument." >&2
  exit 2
fi

if [ ! -e "$submodule_path/.git" ]; then
  echo "CANNOT CHECK: submodule '$submodule_path' is not checked out." >&2
  echo "Run: git submodule update --init '$submodule_path'" >&2
  exit 2
fi

# UNSHALLOW BEFORE ASKING. `merge-base` answers from the commits it can see,
# and a CI checkout is shallow by default - actions/checkout clones submodules
# at depth 1 unless fetch-depth is 0. Against a depth-1 clone the ancestry
# question has no honest answer: every commit but one is missing, so a healthy
# pin and a dangling one look identical. Deepening first is what makes the
# green result mean something.
if [ "$(git -C "$submodule_path" rev-parse --is-shallow-repository)" = "true" ]; then
  if ! git -C "$submodule_path" fetch --unshallow --quiet origin 2>/dev/null; then
    echo "CANNOT CHECK: '$submodule_path' is a shallow clone and could not be deepened." >&2
    echo "This is a fetch failure, NOT a dangling pin." >&2
    exit 2
  fi
fi

if ! git -C "$submodule_path" fetch --quiet origin "$default_branch" 2>/dev/null; then
  echo "CANNOT CHECK: could not fetch '$default_branch' from the spec remote." >&2
  echo "This is a fetch failure, NOT a dangling pin." >&2
  exit 2
fi
tip=$(git -C "$submodule_path" rev-parse FETCH_HEAD)

# The pinned object must be present locally before ancestry means anything -
# otherwise `merge-base` errors on a bad revision and the message blames the
# wrong thing.
if ! git -C "$submodule_path" cat-file -e "${pinned}^{commit}" 2>/dev/null; then
  echo "PINNED SPEC COMMIT IS GONE." >&2
  echo "  pinned:         $pinned" >&2
  echo "  submodule:      $submodule_path" >&2
  echo "  $default_branch is at: $tip" >&2
  echo "The commit is not in the spec repo at all - it was garbage-collected." >&2
  echo "Move the pin to a commit on '$default_branch'." >&2
  exit 1
fi

if git -C "$submodule_path" merge-base --is-ancestor "$pinned" "$tip"; then
  echo "ok: pinned spec $pinned is an ancestor of $default_branch ($tip)"
  exit 0
fi

echo "PINNED SPEC COMMIT IS NOT ON '$default_branch'." >&2
echo "  pinned:         $pinned  ($(git -C "$submodule_path" log -1 --format=%s "$pinned"))" >&2
echo "  submodule:      $submodule_path" >&2
echo "  $default_branch is at: $tip  ($(git -C "$submodule_path" log -1 --format=%s "$tip"))" >&2
echo >&2
echo "The pin resolves - a submodule fetches by SHA - but the commit is not" >&2
echo "reachable from '$default_branch'. It is most likely the pre-squash head" >&2
echo "of a merged spec branch. When that branch is deleted and the commit is" >&2
echo "collected, this repo's CI will start failing at CHECKOUT instead." >&2
echo >&2
echo "Move the pin to a commit on '$default_branch' (markup-carve/carve#1740)." >&2
exit 1
