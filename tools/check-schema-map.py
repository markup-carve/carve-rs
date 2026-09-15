#!/usr/bin/env python3
"""Check the vendored ProseMirror schema map against carve-grammars.

`resources/prosemirror-schema-map.json` is a copy of carve-grammars
`tiptap/schema-map.json` and records which commit it was copied from. Nothing
measured that commit, so the copy could fall behind in silence
(markup-carve/carve-rs#1640).

    python3 tools/check-schema-map.py --grammars <carve-grammars checkout>

The subject is the set of DECISIONS - which Carve type maps to which ProseMirror
name, or is declared unmappable - not the commit distance. carve-grammars merges
continuously, so a gate on distance would be red from any open pull request and
clearable only by luck; the distance is reported as a number instead.

Every difference from upstream has to be named in `_provenance.divergences`,
which is how a deliberate one is told apart from drift. Exit 0 all assertions
hold, 1 an assertion failed, 2 usage error.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

FULL_REV_RE = re.compile(r"^[0-9a-f]{40}$")
# `notes` is local prose, and `attrs`/`aliasOf` are upstream keys this engine's
# loader never reads. A difference in one of them is not a decision.
DECISION_KEYS = ("kind", "pm", "accepts")


class Failure(Exception):
    def __init__(self, check: str, message: str) -> None:
        super().__init__(message)
        self.check = check
        self.message = message


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=False
    )


def decisions(doc: dict) -> dict:
    """Type name -> the mapping decision, with prose and unread keys dropped."""
    out = {}
    for ty, entry in (doc.get("types") or {}).items():
        if not isinstance(entry, dict):
            continue
        out[ty] = ("type",) + tuple(json.dumps(entry.get(k)) for k in DECISION_KEYS)
    for ty in doc.get("unmapped") or {}:
        out[ty] = ("unmapped",)
    return out


def provenance(local: dict) -> tuple[str, str, dict]:
    block = local.get("_provenance")
    if not isinstance(block, dict):
        raise Failure("provenance_present", "the map carries no `_provenance` block")
    commit = block.get("commit")
    source = block.get("source")
    if not commit:
        raise Failure("provenance_present", "`_provenance` names no `commit`")
    if not source:
        raise Failure("provenance_present", "`_provenance` names no `source`")
    if not FULL_REV_RE.match(str(commit)):
        raise Failure(
            "commit_well_formed",
            f"`{commit}` is not a 40-character lowercase hex revision; an "
            f"abbreviation resolves in one checkout and not in the next",
        )
    divergences = block.get("divergences") or {}
    if not isinstance(divergences, dict):
        raise Failure("provenance_present", "`_provenance.divergences` is not an object")
    return str(commit), str(source), divergences


def source_path(source: str) -> str:
    """The upstream path named by `_provenance.source`, so a rename shows up here."""
    paths = [token for token in str(source).split() if token.endswith(".json")]
    if len(paths) != 1:
        raise Failure(
            "source_readable",
            f"`_provenance.source` should name exactly one .json path; it reads "
            f"{source!r}",
        )
    return paths[0]


def upstream_at(grammars: Path, rev: str, path: str, check: str) -> dict:
    result = git(grammars, "show", f"{rev}:{path}")
    if result.returncode != 0:
        raise Failure(check, f"carve-grammars {rev} has no {path}: {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise Failure(check, f"carve-grammars {rev}:{path} is not valid JSON: {exc}") from exc


def resolve_branch(grammars: Path, branch: str) -> str:
    for candidate in (f"origin/{branch}", branch):
        if git(grammars, "rev-parse", "--verify", "--quiet", f"{candidate}^{{commit}}").returncode == 0:
            return candidate
    raise Failure(
        "pin_is_current",
        f"neither origin/{branch} nor {branch} exists in {grammars}; check it out "
        f"with fetch-depth: 0",
    )


def compare(ours: dict, theirs: dict, divergences: dict, check: str, what: str) -> set:
    """Every decision that differs has to be declared. Returns the names used."""
    undeclared, used = [], set()
    for ty in sorted(set(ours) | set(theirs)):
        if ours.get(ty) == theirs.get(ty):
            continue
        if ty in divergences:
            used.add(ty)
            continue
        undeclared.append(f"{ty} (here: {ours.get(ty, 'no decision')}, {what}: {theirs.get(ty, 'no decision')})")
    if undeclared:
        raise Failure(
            check,
            f"{len(undeclared)} decision(s) differ from {what} with nothing in "
            f"`_provenance.divergences` saying why: " + "; ".join(undeclared),
        )
    return used


def annotate(kind: str, message: str, github: bool) -> None:
    print(f"::{kind}::{message}" if github else f"{kind}: {message}")


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--grammars", required=True, type=Path, help="a carve-grammars checkout")
    p.add_argument("--branch", default="main", help="the branch the pin must be on")
    p.add_argument(
        "--map",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "resources" / "prosemirror-schema-map.json",
    )
    p.add_argument("--github", action="store_true", help="emit GitHub Actions annotations")
    args = p.parse_args(argv)

    if not (args.grammars / ".git").exists():
        print(f"check-schema-map: {args.grammars} is not a git checkout", file=sys.stderr)
        return 2
    try:
        local = json.loads(args.map.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"check-schema-map: {args.map} is not readable JSON: {exc}", file=sys.stderr)
        return 2

    failures: list[Failure] = []
    commit = None
    try:
        commit, source, divergences = provenance(local)
        path = source_path(source)

        if git(args.grammars, "cat-file", "-e", f"{commit}^{{commit}}").returncode != 0:
            raise Failure("commit_exists", f"carve-grammars has no commit {commit}")
        ref = resolve_branch(args.grammars, args.branch)
        if git(args.grammars, "merge-base", "--is-ancestor", commit, ref).returncode != 0:
            raise Failure(
                "commit_on_branch",
                f"carve-grammars {commit} is not an ancestor of {ref}, so this copy "
                f"came from an unmerged or rewritten branch",
            )

        touched = git(args.grammars, "log", "-1", "--format=%H", commit, "--", path).stdout.strip()
        if touched != commit:
            raise Failure(
                "commit_touched_source",
                f"carve-grammars {commit} does not change {path}; record the commit "
                f"the copy was taken from, which is {touched or 'none in its history'}",
            )

        pinned = upstream_at(args.grammars, commit, path, "source_readable")
        head = upstream_at(args.grammars, ref, path, "pin_is_current")

        used = compare(decisions(local), decisions(pinned), divergences, "decisions_match_pin", "the pin")
        used |= compare(decisions(pinned), decisions(head), divergences, "pin_is_current", f"{args.branch}")

        stale = sorted(set(divergences) - used)
        if stale:
            raise Failure(
                "divergences_are_live",
                f"`_provenance.divergences` names {', '.join(stale)}, which no longer "
                f"differ from upstream; drop the entry rather than let the list only grow",
            )
    except Failure as failure:
        failures.append(failure)

    if commit and not failures:
        ref = resolve_branch(args.grammars, args.branch)
        behind = git(args.grammars, "rev-list", "--count", f"{commit}..{ref}").stdout.strip() or "?"
        touching = git(
            args.grammars, "rev-list", "--count", f"{commit}..{ref}", "--", source_path(local["_provenance"]["source"])
        ).stdout.strip() or "?"
        print(f"recorded carve-grammars commit: {commit}")
        print(f"{args.branch} is {behind} commit(s) ahead of it, {touching} of them touching the map")
        for ty, why in sorted(local["_provenance"].get("divergences", {}).items()):
            print(f"declared divergence: {ty} - {why}")

    for failure in failures:
        annotate("error", f"{failure.check}: {failure.message}", args.github)
    if failures:
        return 1
    print("check-schema-map: every assertion holds.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
