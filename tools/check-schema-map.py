#!/usr/bin/env python3
"""Check the vendored ProseMirror schema map against carve-grammars.

`resources/prosemirror-schema-map.json` is a copy of carve-grammars
`tiptap/schema-map.json` and records which commit it was copied from. Nothing
measured that commit, so the copy could fall behind in silence
(markup-carve/carve-rs#1640).

    python3 tools/check-schema-map.py --grammars <carve-grammars checkout>

The subject is the set of DECISIONS - which Carve type maps to which ProseMirror
name or is declared unmappable, and the nodes the two sections outside `types`
name, which `src/prosemirror` reads as well - not the commit distance. carve-grammars merges
continuously, so a gate on distance would be red from any open pull request and
clearable only by luck; the distance is reported as a number instead.

Every difference from upstream has to be named in `_provenance.divergences`,
which is how a deliberate one is told apart from drift. A declaration also says
WHY as one of a closed set of reason kinds, and two of those kinds are claims
about upstream this script tests (markup-carve/carve#2270): the difference
disappearing and the reason going false are separate ways for a declaration to
stop being true, and only the first used to be refused. Exit 0 all assertions
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
# The two sections that name nodes belonging to no Carve type. `src/prosemirror`
# reads both - the carrier by the entry whose `attrs` holds `markType`, the
# preserved pair by the entries that have an `attrs` at all - so a rename or a
# moved attribute upstream is a DECISION and not prose. Keyed with a prefix so
# the name cannot collide with a Carve type, matching carve-php's `carrier:`.
NAMED_NODE_SECTIONS = (("markCarrierNodes", "carrier"), ("preservationNodes", "preserved"))
# `group` says where the node may appear and `kind` whether it is a node at all;
# both change what a bridge may write. The ATTRIBUTE NAMES come with them
# because the loader picks the carrier by one of them.
NAMED_NODE_KEYS = ("kind", "group")
NODE_NAME_RE = re.compile(r"^carve[A-Z][A-Za-z0-9]*$")

# What a divergence may say about upstream, and which of those a checker can
# test. `prose` is the escape hatch: it carries no claim about upstream, so
# nothing beyond the difference check reaches it.
REASON_KINDS = ("upstream-has-no-node", "upstream-names-a-node", "prose")
NODE_KINDS = ("upstream-has-no-node", "upstream-names-a-node")


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
    for key, tag in NAMED_NODE_SECTIONS:
        for name, entry in (doc.get(key) or {}).items():
            # `about` is the section's own prose, not a node.
            if name == "about" or not isinstance(entry, dict):
                continue
            attrs = entry.get("attrs")
            out[f"{tag}:{name}"] = (
                (tag,)
                + tuple(json.dumps(entry.get(k)) for k in NAMED_NODE_KEYS)
                + (json.dumps(sorted(attrs) if isinstance(attrs, dict) else attrs),)
            )
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


def reason_shapes(divergences: dict) -> None:
    """Every declaration carries one of `REASON_KINDS`, with the fields it needs.

    An unconstrained prose field cannot be gated at all, which is how four
    declarations kept saying "carve-grammars names no node for it yet" through an
    upstream decision (markup-carve/carve#2270).
    """
    bad = []
    for ty, entry in sorted(divergences.items()):
        if not isinstance(entry, dict):
            bad.append(f"{ty}: is {type(entry).__name__}, not an object with a `kind`")
            continue
        kind = entry.get("kind")
        if kind not in REASON_KINDS:
            bad.append(f"{ty}: `kind` is {kind!r}, not one of {', '.join(REASON_KINDS)}")
            continue
        if not str(entry.get("why") or "").strip():
            bad.append(f"{ty}: says no `why`")
        if kind not in NODE_KINDS:
            continue
        node = entry.get("node")
        if not node:
            bad.append(f"{ty}: `{kind}` names no `node`, so its claim about upstream cannot be tested")
        elif not NODE_NAME_RE.match(str(node)):
            bad.append(f"{ty}: `{node}` is not a ProseMirror name upstream could publish")
    if bad:
        raise Failure("divergence_reasons_are_shaped", "; ".join(bad))


def upstream_node_names(doc: dict) -> set:
    """Every ProseMirror name upstream publishes, mapped type or not.

    `preservationNodes` and `markCarrierNodes` are named nodes that belong to no
    Carve type, so a declaration claiming one is absent has to see them too.
    """
    names = set()
    for entry in (doc.get("types") or {}).values():
        if not isinstance(entry, dict):
            continue
        pm = entry.get("pm")
        names.update([pm] if isinstance(pm, str) else [n for n in (pm or []) if isinstance(n, str)])
    for key in ("preservationNodes", "markCarrierNodes"):
        names.update(name for name in (doc.get(key) or {}) if name != "about")

    return names


def reasons_hold(divergences: dict, head: dict, branch: str) -> None:
    """Test each declaration's claim about upstream against upstream's own list.

    Two halves per kind, because one of them is typo-proof: the NAME the entry
    gives is resolved against the published names, and upstream's decision about
    the TYPE is read from the key the entry is filed under. A misspelled name
    would otherwise never resolve and the claim would hold forever.
    """
    published = upstream_node_names(head)
    decided = head.get("types") or {}
    false_now = []
    for ty, entry in sorted(divergences.items()):
        kind = entry["kind"]
        if kind not in NODE_KINDS:
            continue
        node = str(entry["node"])
        resolves = node in published
        if kind == "upstream-has-no-node":
            if resolves:
                false_now.append(f"{ty}: {branch} publishes `{node}`")
            elif ty in decided:
                false_now.append(f"{ty}: {branch} names a node for it, `{decided[ty].get('pm')}`")
        elif not resolves:
            false_now.append(f"{ty}: {branch} publishes no `{node}`")
        elif ty not in decided:
            false_now.append(f"{ty}: {branch} names no node for it at all")
    if false_now:
        raise Failure(
            "divergence_reasons_hold",
            f"{len(false_now)} declaration(s) state something about upstream that is no longer "
            f"true; correct the reason or drop the entry: " + "; ".join(false_now),
        )


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

        # Ahead of the difference checks: a reason goes false while the
        # difference persists, so the two cannot be reached through each other.
        reason_shapes(divergences)
        reasons_hold(divergences, head, args.branch)

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
        for ty, entry in sorted(local["_provenance"].get("divergences", {}).items()):
            node = f" ({entry['node']})" if entry.get("node") else ""
            print(f"declared divergence: {ty} [{entry['kind']}{node}] - {entry['why']}")

    for failure in failures:
        annotate("error", f"{failure.check}: {failure.message}", args.github)
    if failures:
        return 1
    print("check-schema-map: every assertion holds.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
