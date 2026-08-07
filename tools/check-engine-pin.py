#!/usr/bin/env python3
"""Check that a binding's recorded engine revision describes a real carve-rs.

Every binding pins this engine, and until now only one of them measured its pin
at all - as a warning that could never fail a job (markup-carve/carve-rs#771).
The pins come in two shapes and, across the repositories, four spellings:

  - a Cargo git dependency, whose revision lives in `Cargo.toml` and again in
    `Cargo.lock`. The crate publishes as `carve-lang`, NOT `carve` (the name
    was taken on crates.io), so a reader grepping a manifest for "carve" finds
    the binding's own package and concludes there is no pin;
  - a bare 40-hex revision in a text file beside a prebuilt artifact, the way
    carve-go records `internal/wasm/REV` next to the wasm it describes.

One reader, parameterized by path and form, so the next binding inherits the
guard rather than reinventing it.

    python3 tools/check-engine-pin.py --engine <carve-rs checkout> \\
        --form cargo --manifest Cargo.toml --lock Cargo.lock
    python3 tools/check-engine-pin.py --engine <carve-rs checkout> \\
        --form rev-file --file internal/wasm/REV

WHAT IT ASSERTS, AND WHY IT IS NOT A DISTANCE CHECK
---------------------------------------------------
The obvious gate is "fail when the pin is behind main". That one is useless
here: carve-rs merges continuously, so it would be red from the moment any PR
opens, unclearable by the action it recommends, and the predictable end state is
someone raising the tolerance until it means nothing.

The inverse failure matters just as much. A gate whose only assertion is about
the distance stops asserting anything the moment the distance is zero - and a
healthy pin is the state this is trying to reach. A gate that stops working once
its subject is healthy is not a gate (markup-carve/carve#755).

So the load-bearing assertions are the ones that HOLD AND CAN FAIL AT ZERO
DRIFT, with the pin sitting exactly on the engine's current tip:

  pin_present        the pin file exists, is readable, and actually names the
                     engine. "No engine dependency found" is a FAILURE, never a
                     pass - a reader that quietly finds nothing is the defect
                     this replaces.
  pin_well_formed    exactly one revision, 40 lowercase hex characters. An
                     abbreviated or upper-case revision resolves locally and
                     then does not match the lockfile, so it is refused here.
  lock_agrees        (cargo form) the lockfile's `source = "git+...?rev=..."`
                     names the same revision as the manifest, and the package it
                     names is `carve-lang`. Read from the LOCK's own source
                     line, not from the manifest twice - reading one file twice
                     is what makes two files "agree" without either checking the
                     other.
  revision_exists    it is a real commit in carve-rs.
  revision_on_branch it is an ancestor of the engine's default branch, so the
                     artifact did not come from an unmerged or rewritten branch.
  artifact_digest    (optional) the committed artifact hashes to the digest
                     recorded beside the revision, so the revision describes the
                     binary rather than merely sitting next to it.

The lag is reported, never asserted on commit count. `--max-age-days N` gates it
on AGE instead, which is something the actor controls: a pin older than N days is
cleared by bumping it, whereas a pin behind by zero commits is unreachable while
the engine is merging. The commit count is printed as a number in the summary.

Exit codes: 0 all assertions hold, 1 an assertion failed, 2 usage/setup error.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - 3.10 and older
    print(
        "check-engine-pin: needs Python 3.11+ for tomllib. Parsing Cargo.toml "
        "and Cargo.lock with a regular expression is how a pin gets misread, "
        "so there is no fallback on purpose.",
        file=sys.stderr,
    )
    raise SystemExit(2)

ENGINE_REPO_RE = re.compile(r"carve-rs(\.git)?/?$")
FULL_REV_RE = re.compile(r"^[0-9a-f]{40}$")
ENGINE_PACKAGE = "carve-lang"

# `git+<url>?rev=<rev>#<resolved>` - Cargo writes the resolved commit after the
# fragment, and it is the one that was actually fetched.
LOCK_SOURCE_RE = re.compile(r"^git\+(?P<url>[^?#]+)(\?rev=(?P<rev>[^#]+))?(#(?P<resolved>.+))?$")


class Failure(Exception):
    """An assertion did not hold. Carries the check name for the report."""

    def __init__(self, check: str, message: str) -> None:
        super().__init__(message)
        self.check = check
        self.message = message


# ---------------------------------------------------------------------------
# reading a pin
# ---------------------------------------------------------------------------


def _load_toml(path: Path, check: str) -> dict:
    if not path.is_file():
        raise Failure(check, f"{path} does not exist")
    try:
        with path.open("rb") as fh:
            return tomllib.load(fh)
    except OSError as exc:
        raise Failure(check, f"{path} is not readable: {exc}") from exc
    except tomllib.TOMLDecodeError as exc:
        raise Failure(check, f"{path} is not valid TOML: {exc}") from exc


def _is_engine_url(url: str) -> bool:
    return bool(ENGINE_REPO_RE.search(url.rstrip("/")))


def manifest_rev(manifest: Path) -> tuple[str, str]:
    """The engine revision named by a Cargo manifest, plus the dependency key.

    Located by its git URL, not by its key: the three Cargo bindings spell the
    key `carve_rs`, `carve_rs` and `carve`, and the package it renames to is
    `carve-lang`. Searching for a name finds nothing in at least one of them.
    """
    data = _load_toml(manifest, "pin_present")
    found: list[tuple[str, dict]] = []
    for table in ("dependencies", "dev-dependencies", "build-dependencies"):
        for key, spec in (data.get(table) or {}).items():
            if isinstance(spec, dict) and _is_engine_url(str(spec.get("git", ""))):
                found.append((key, spec))
    if not found:
        raise Failure(
            "pin_present",
            f"{manifest} declares no git dependency on carve-rs. If the engine "
            f"moved to a published version, this guard has to be told; a reader "
            f"that finds nothing must not report success.",
        )
    if len(found) > 1:
        keys = ", ".join(sorted(k for k, _ in found))
        raise Failure("pin_present", f"{manifest} declares carve-rs more than once: {keys}")
    key, spec = found[0]
    rev = spec.get("rev")
    if not rev:
        raise Failure(
            "pin_present",
            f"{manifest} depends on carve-rs at `{key}` with no `rev`, so every "
            f"build resolves whatever has landed since and the package can carry "
            f"an engine no CI run here has ever built",
        )
    package = spec.get("package", key)
    if package != ENGINE_PACKAGE:
        raise Failure(
            "pin_present",
            f"{manifest} renames the engine to `{package}`; it publishes as "
            f"`{ENGINE_PACKAGE}` (the name `carve` is taken on crates.io)",
        )
    return str(rev), key


def lock_rev(lock: Path) -> str:
    """The engine revision the LOCKFILE resolved, from its own `source` line."""
    data = _load_toml(lock, "lock_agrees")
    matches = []
    for package in data.get("package") or []:
        source = str(package.get("source", ""))
        m = LOCK_SOURCE_RE.match(source)
        if m and _is_engine_url(m.group("url")):
            matches.append((package.get("name"), m))
    if not matches:
        raise Failure(
            "lock_agrees",
            f"{lock} has no `source = \"git+...carve-rs...\"` entry, so nothing "
            f"in it corroborates the manifest",
        )
    if len(matches) > 1:
        raise Failure("lock_agrees", f"{lock} resolves carve-rs more than once")
    name, m = matches[0]
    if name != ENGINE_PACKAGE:
        raise Failure(
            "lock_agrees",
            f"{lock} names the engine package `{name}`; it is `{ENGINE_PACKAGE}`",
        )
    rev = m.group("resolved") or m.group("rev")
    if not rev:
        raise Failure("lock_agrees", f"{lock} pins carve-rs to a branch, not a revision")
    return rev


def rev_file_rev(path: Path) -> str:
    if not path.is_file():
        raise Failure(
            "pin_present",
            f"{path} is missing; it is what records which carve-rs the committed "
            f"artifact was built from",
        )
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise Failure("pin_present", f"{path} is not readable: {exc}") from exc
    lines = [ln.strip() for ln in raw.splitlines() if ln.strip()]
    if len(lines) != 1:
        raise Failure(
            "pin_present",
            f"{path} should hold exactly one revision; it holds {len(lines)} non-blank line(s)",
        )
    return lines[0]


# ---------------------------------------------------------------------------
# assertions against the engine checkout
# ---------------------------------------------------------------------------


def git(engine: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(engine), *args],
        capture_output=True,
        text=True,
        check=False,
    )


def resolve_branch(engine: Path, branch: str) -> str:
    """The ref to compare against, preferring the remote-tracking one.

    A CI checkout is usually detached at the merge commit, so a bare `main` may
    not exist locally even though `origin/main` does.
    """
    for candidate in (f"origin/{branch}", branch):
        if git(engine, "rev-parse", "--verify", "--quiet", f"{candidate}^{{commit}}").returncode == 0:
            return candidate
    raise Failure(
        "revision_on_branch",
        f"neither origin/{branch} nor {branch} exists in {engine}; check it out "
        f"with fetch-depth: 0 so the history is there to compare against",
    )


def check_well_formed(rev: str) -> None:
    if not FULL_REV_RE.match(rev):
        raise Failure(
            "pin_well_formed",
            f"`{rev}` is not a 40-character lowercase hex revision. An "
            f"abbreviation resolves locally and then fails to match the "
            f"lockfile, which is the drift this guard is for.",
        )


def check_exists(engine: Path, rev: str) -> None:
    if git(engine, "cat-file", "-e", f"{rev}^{{commit}}").returncode != 0:
        raise Failure("revision_exists", f"carve-rs has no commit {rev}")


def check_on_branch(engine: Path, rev: str, ref: str) -> None:
    if git(engine, "merge-base", "--is-ancestor", rev, ref).returncode != 0:
        raise Failure(
            "revision_on_branch",
            f"carve-rs {rev} is not an ancestor of {ref}, so the pinned engine "
            f"came from an unmerged or rewritten branch",
        )


def check_artifact(artifact: Path, digest_file: Path) -> None:
    if not artifact.is_file():
        raise Failure("artifact_digest", f"{artifact} does not exist")
    if not digest_file.is_file():
        raise Failure("artifact_digest", f"{digest_file} does not exist")
    want = digest_file.read_text(encoding="utf-8").split()
    if not want:
        raise Failure("artifact_digest", f"{digest_file} is empty")
    got = hashlib.sha256(artifact.read_bytes()).hexdigest()
    if got != want[0].lower():
        raise Failure(
            "artifact_digest",
            f"{artifact} hashes to {got}, but {digest_file} records {want[0]}; "
            f"the recorded revision describes a different build than the one "
            f"committed",
        )


# ---------------------------------------------------------------------------
# reporting
# ---------------------------------------------------------------------------


def annotate(kind: str, message: str, github: bool) -> None:
    if github:
        print(f"::{kind}::{message}")
    else:
        print(f"{kind}: {message}")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(
        description="Check a binding's carve-rs pin.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--engine", required=True, type=Path, help="path to a carve-rs checkout")
    p.add_argument("--branch", default="main", help="engine branch the pin must be on")
    p.add_argument("--form", required=True, choices=("cargo", "rev-file"))
    p.add_argument("--manifest", type=Path, help="cargo form: path to Cargo.toml")
    p.add_argument("--lock", type=Path, help="cargo form: path to Cargo.lock")
    p.add_argument("--file", type=Path, help="rev-file form: path to the revision file")
    p.add_argument("--artifact", type=Path, help="committed artifact the revision describes")
    p.add_argument("--artifact-digest", type=Path, help="file holding the artifact's sha256")
    p.add_argument(
        "--max-age-days",
        type=int,
        default=0,
        help="fail when the pinned commit is older than this many days (0 = report only)",
    )
    p.add_argument("--github", action="store_true", help="emit GitHub Actions annotations")
    args = p.parse_args(argv)

    if args.form == "cargo" and not (args.manifest and args.lock):
        p.error("--form cargo needs --manifest and --lock")
    if args.form == "rev-file" and not args.file:
        p.error("--form rev-file needs --file")
    if bool(args.artifact) != bool(args.artifact_digest):
        p.error("--artifact and --artifact-digest are only meaningful together")
    if not (args.engine / ".git").exists():
        print(f"check-engine-pin: {args.engine} is not a git checkout", file=sys.stderr)
        return 2

    failures: list[Failure] = []
    rev = None
    try:
        if args.form == "cargo":
            rev, key = manifest_rev(args.manifest)
            check_well_formed(rev)
            locked = lock_rev(args.lock)
            if locked != rev:
                raise Failure(
                    "lock_agrees",
                    f"{args.manifest} pins `{key}` at {rev}, {args.lock} resolved "
                    f"{locked}. One of them was updated without the other.",
                )
        else:
            rev = rev_file_rev(args.file)
            check_well_formed(rev)
        check_exists(args.engine, rev)
        ref = resolve_branch(args.engine, args.branch)
        check_on_branch(args.engine, rev, ref)
        if args.artifact:
            check_artifact(args.artifact, args.artifact_digest)
    except Failure as failure:
        failures.append(failure)

    # The lag report. Reached only when the revision is known to exist, because
    # there is nothing to measure against otherwise. It is a NUMBER in the
    # summary; only AGE can fail the job.
    if rev and not failures:
        ref = resolve_branch(args.engine, args.branch)
        behind = git(args.engine, "rev-list", "--count", f"{rev}..{ref}").stdout.strip() or "?"
        subject = git(args.engine, "log", "-1", "--format=%s", rev).stdout.strip()
        age_days = git(args.engine, "log", "-1", "--format=%ct", rev).stdout.strip()
        now = git(args.engine, "log", "-1", "--format=%ct", ref).stdout.strip()
        days = None
        if age_days.isdigit() and now.isdigit():
            days = (int(now) - int(age_days)) / 86400.0
        print(f"pinned engine: carve-rs {rev} ({subject})")
        print(f"{args.branch} is {behind} commit(s) ahead of it")
        if days is not None:
            print(f"the pin is {days:.1f} day(s) older than the tip of {args.branch}")
            if args.max_age_days > 0 and days > args.max_age_days:
                failures.append(
                    Failure(
                        "pin_age",
                        f"the pin is {days:.1f} days old, over the {args.max_age_days}-day "
                        f"limit. Bump it; the commit count is not the subject here "
                        f"because carve-rs merges continuously and zero is unreachable.",
                    )
                )

    for failure in failures:
        annotate("error", f"{failure.check}: {failure.message}", args.github)
    if failures:
        return 1
    print("check-engine-pin: every assertion holds.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
