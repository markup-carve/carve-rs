#!/usr/bin/env python3
"""Every pull request that moved shipped source in a release is cited in the
release's changelog section.

WHY THIS EXISTS
---------------
A `chore: release X.Y.Z` commit writes the `## [X.Y.Z]` section and then
development simply carries on over it. Nobody reopens the section, and weeks
later the tag ships notes describing only the handful of changes that existed
on cut day. Measured across three engines in one night: this repository's 0.1.7
section documented 3 of 24 merges, carve-php 0.1.10 documented 1 of 22, and
carve-js 0.1.8 documented 1 of 17. `[Unreleased]` was empty in every case, so
the work was recorded nowhere.

The guard already in the release workflow asks whether a section EXISTS and
carries notes. That passes happily on a section covering 3 of 24 changes.
Existence is not completeness, so this asks the other question.

WHY THE FILTER IS SHIPPED SOURCE, NOT THE COMMIT PREFIX
-------------------------------------------------------
A `fix:` prefix is a convention people drift from, and a `chore:`-prefixed
merge can still move the parser. Whether the diff touched SHIPPED is a fact
about the commit rather than a claim in its subject, so that is what decides.

WHY IT ASKS GITHUB WHAT A PULL REQUEST CLOSES
----------------------------------------------
The entries across these repositories cite the ISSUE a fix answers as often as
the pull request that carried it. A gate that demanded the pull request number
would fail correctly documented releases, and a check that cries wolf is one
somebody deletes. So a pull request counts as cited under its own number or
under any issue it closes.

    python3 tools/check-changelog-completeness.py [version] [options]

      version         the release to check; defaults to Cargo.toml's version
      --at REV        read history and CHANGELOG.md as of this revision;
                      defaults to the tag when it exists, otherwise HEAD
      --previous TAG  measure from this tag instead of the highest version tag
                      below `version`
      --section HEAD  the heading to read; defaults to `version`
      --repo O/N      this repository's slug; defaults to GITHUB_REPOSITORY,
                      then the origin remote
      --root DIR      the checkout to read; defaults to the working directory

Needs `gh` authenticated (the release workflow passes GITHUB_TOKEN). It refuses
to run without it rather than degrading to an answer it cannot back.

Exit 0  every shipped-source pull request in range is cited or exempt.
Exit 1  at least one is not, and every one of them is named.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

# Shipped source for this crate. A change anywhere under it can reach a
# consumer, so it owes the reader a changelog line. The crate version is
# `CARGO_PKG_VERSION` rather than a constant in the tree, so the release cut
# touches no file here and nothing needs excluding.
SHIPPED = [re.compile(r"^src/")]
NOT_SHIPPED: list[re.Pattern[str]] = []

EXEMPT_FILE = ".changelog-exempt"

VERSION_TAG = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")
# A squash merge carries its pull request as a trailing `(#N)`, the only number
# on the commit that can lead anywhere.
PULL_REQUEST = re.compile(r"\(#(\d+)\)\s*$")
# A qualified reference to ANOTHER repository is not a citation of this one, so
# the owner is checked rather than grepping for a bare `#N`.
REFERENCE = re.compile(r"(?:([A-Za-z0-9._-]+/[A-Za-z0-9._-]+))?#(\d+)\b")
EXEMPTION = re.compile(r"^#?(\d+)\s*[:\s]\s*(\S.*)$")


def main() -> int:
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("version", nargs="?")
    ap.add_argument("--at")
    ap.add_argument("--previous")
    ap.add_argument("--section")
    ap.add_argument("--repo")
    ap.add_argument("--root", default=".")
    opts = ap.parse_args()

    root = Path(opts.root).resolve()

    def git(*args: str) -> str:
        return subprocess.run(
            ["git", *args], cwd=root, check=True, capture_output=True, text=True
        ).stdout.strip()

    version = (opts.version or cargo_version(root)).lstrip("v")
    section = opts.section or version
    at = opts.at or (version if rev_exists(root, version) else "HEAD")
    slug = opts.repo or os.environ.get("GITHUB_REPOSITORY") or origin_slug(git)
    owner, _, name = slug.partition("/")

    previous = opts.previous or highest_below(git, at, version)
    rng = f"{previous}..{at}" if previous else at

    # What merged, and what of it touched shipped source.
    shipping: dict[str, str] = {}
    unattributed: list[tuple[str, str]] = []
    for line in git("log", "--format=%H\x1f%s", rng).splitlines():
        if not line:
            continue
        sha, _, subject = line.partition("\x1f")
        if not touches_shipped(git, sha):
            continue
        m = PULL_REQUEST.search(subject)
        if not m:
            unattributed.append((sha[:9], subject))
            continue
        shipping.setdefault(m.group(1), PULL_REQUEST.sub("", subject).strip())

    closes = closing_issues(owner, name, list(shipping), shipping)
    if closes is None:
        return 1

    body = changelog_at(git, root, at)
    text = section_of(body, section)
    if text is None:
        print(f"::error::CHANGELOG.md has no '## [{section}]' section to check")
        return 1

    cited = set()
    for qualifier, number in REFERENCE.findall(text):
        if qualifier and qualifier.lower() != slug.lower():
            continue
        cited.add(number)

    exemptions, malformed = read_exemptions(root)

    missing: list[tuple[str, str]] = []
    skipped: list[tuple[str, str, str]] = []
    for number in sorted(shipping, key=int):
        title = shipping[number]
        if number in cited or any(i in cited for i in closes.get(number, [])):
            continue
        if number in exemptions:
            skipped.append((number, title, exemptions[number]))
            continue
        missing.append((number, title))

    for number, title, reason in skipped:
        print(f"exempt: #{number} {title}\n        ({EXEMPT_FILE}: {reason})")
    for sha, subject in unattributed:
        print(f"::notice::{sha} touched shipped source with no pull request to cite: {subject}")

    frm = f"since {previous}" if previous else "so far"
    if malformed or missing:
        for problem in malformed:
            print(f"::error::{problem}")
        for number, title in missing:
            also = closes.get(number) or []
            tail = f" (closes {', '.join('#' + i for i in also)})" if also else ""
            print(f"::error::#{number}{tail} is not cited in the {section} section: {title}")
        if missing:
            print(
                f"changelog-completeness: {len(missing)} of {len(shipping)} pull request(s) {frm} "
                f"moved shipped source and are cited nowhere in the {section} section. Write them "
                f"up, or exempt one with a reason in {EXEMPT_FILE}."
            )
        return 1

    extra = ""
    if skipped:
        extra += f", {len(skipped)} exempt"
    if unattributed:
        extra += f", {len(unattributed)} commit(s) carrying no pull request"
    print(
        f"changelog-completeness: the {section} section accounts for all "
        f"{len(shipping) - len(skipped)} shipped-source pull request(s) {frm}{extra}"
    )
    return 0


def cargo_version(root: Path) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    start = re.search(r"^\[package\]\s*$", text, re.M)
    for line in text[start.end():].splitlines() if start else []:
        if line.startswith("["):
            break
        m = re.match(r'^version\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    raise SystemExit("Cargo.toml declares no [package] version")


def rev_exists(root: Path, rev: str) -> bool:
    return subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", f"{rev}^{{commit}}"],
        cwd=root, capture_output=True,
    ).returncode == 0


def origin_slug(git) -> str:
    m = re.search(r"[:/]([^/:]+/[^/]+?)(?:\.git)?$", git("remote", "get-url", "origin"))
    if not m:
        raise SystemExit("cannot derive the repository slug; pass --repo owner/name")
    return m.group(1)


def highest_below(git, at: str, version: str) -> str | None:
    def key(tag: str) -> tuple[int, int, int]:
        return tuple(int(p) for p in VERSION_TAG.match(tag).groups())  # type: ignore[return-value]

    target = key(version) if VERSION_TAG.match(version) else None
    below = [
        t.strip() for t in git("tag", "--merged", at).splitlines()
        if VERSION_TAG.match(t.strip()) and (target is None or key(t.strip()) < target)
    ]
    return max(below, key=key) if below else None


def touches_shipped(git, sha: str) -> bool:
    files = git(
        "diff-tree", "--no-commit-id", "--name-only", "-r", "-m", "--first-parent", "--root", sha
    ).splitlines()
    return any(
        any(p.search(f) for p in SHIPPED) and not any(p.search(f) for p in NOT_SHIPPED)
        for f in files if f
    )


def closing_issues(owner: str, name: str, numbers: list[str], titles: dict[str, str]):
    """What each pull request closes, batched by alias so this is a few
    requests rather than one per pull request."""
    out: dict[str, list[str]] = {}
    for i in range(0, len(numbers), 50):
        chunk = numbers[i:i + 50]
        fields = "\n".join(
            f"p{n}: pullRequest(number:{n})"
            "{number title closingIssuesReferences(first:30){nodes{number}}}"
            for n in chunk
        )
        query = (
            "query($owner:String!,$name:String!){"
            f"repository(owner:$owner,name:$name){{{fields}}}}}"
        )
        done = subprocess.run(
            ["gh", "api", "graphql", "-F", f"owner={owner}", "-F", f"name={name}", "-F", "query=@-"],
            input=query, capture_output=True, text=True,
        )
        if done.returncode != 0:
            print("::error::could not ask GitHub what these pull requests close, "
                  "so completeness cannot be judged.")
            print(f"::error::{done.stderr.strip().splitlines()[0] if done.stderr.strip() else 'gh failed'}")
            return None
        repo = (json.loads(done.stdout).get("data") or {}).get("repository") or {}
        for node in repo.values():
            if not node:
                continue
            number = str(node["number"])
            out[number] = [str(n["number"]) for n in node["closingIssuesReferences"]["nodes"]]
            if node.get("title"):
                titles[number] = node["title"]
    return out


def changelog_at(git, root: Path, at: str) -> str:
    try:
        return git("show", f"{at}:CHANGELOG.md")
    except subprocess.CalledProcessError:
        return (root / "CHANGELOG.md").read_text(encoding="utf-8")


def section_of(body: str, section: str) -> str | None:
    lines = body.split("\n")
    head = re.compile(r"^## \[?" + re.escape(section) + r"\]?(\s|\]|$)")
    start = next((i for i, l in enumerate(lines) if head.match(l)), None)
    if start is None:
        return None
    rest = lines[start + 1:]
    end = next((i for i, l in enumerate(rest) if l.startswith("## ")), len(rest))
    return "\n".join(rest[:end])


def read_exemptions(root: Path) -> tuple[dict[str, str], list[str]]:
    """Deliberate exclusions stay VISIBLE. An exemption with no reason is
    refused, so the escape hatch cannot decay into a list of bare numbers
    nobody can audit."""
    path = root / EXEMPT_FILE
    if not path.exists():
        return {}, []
    out: dict[str, str] = {}
    malformed: list[str] = []
    for i, raw in enumerate(path.read_text(encoding="utf-8").split("\n"), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = EXEMPTION.match(line)
        if not m:
            malformed.append(f"{EXEMPT_FILE}:{i}: expected '<number>: <reason>', got '{line}'")
            continue
        out[m.group(1)] = m.group(2).strip()
    return out, malformed


if __name__ == "__main__":
    sys.exit(main())
