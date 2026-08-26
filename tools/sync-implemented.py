#!/usr/bin/env python3
"""Add corpus categories to IMPLEMENTED - but only the ones that already pass.

`tests/corpus.rs` guards the allowlist from both directions: a corpus category
missing from IMPLEMENTED fails the build, and an IMPLEMENTED entry with no
corpus pair fails it too. Both guards are right, and neither is maintained by
anything that bumps the spec submodule - so a spec bump that adds a category
lands red on a list nobody told it about (carve#729).

Adding names blindly would break the other half of what the list means: an entry
asserts THIS ENGINE renders the category byte-exact, so an unverified entry turns
a real divergence into a green run. This renders every pair of every new category
and adds only the categories where all pairs match their committed `.html`.

    python3 tools/sync-implemented.py [--check]

Exit codes: 0 when the list is complete, 1 when a category remains unimplemented
(named, with the first diff), 2 on a usage/setup error. `--check` reports without
writing, for CI.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "spec" / "tests" / "corpus"
TESTS = ROOT / "tests" / "corpus.rs"


def strip_leading_number(slug: str) -> str:
    return re.sub(r"^\d+-", "", slug)


def base_category(slug: str, stems: set[str]) -> str:
    """`12-foo-bar-3` -> `foo-bar`, the same reduction tests/corpus.rs makes.

    A CATEGORY MAY END IN A NUMBER OF ITS OWN, so the variant suffix is dropped
    only when what remains names a pair that exists - the same rule
    `base_category` in tests/corpus.rs applies, and for the same reason.
    Stripping the `-0` off `an-empty-description-body-claims-no-line-below-column-0`
    invents a category no pair carries, which this tool then asks IMPLEMENTED to
    name and `all_implemented_pairs_exist` refuses.
    """
    slug = strip_leading_number(slug)
    head, sep, tail = slug.rpartition("-")
    if sep and tail.isdigit() and head in stems:
        return head
    return slug


DECLARATION = "const IMPLEMENTED: &[&str] = &["
GAPS_DECLARATION = "const KNOWN_GAPS: &[&str] = &["


def read_allowlist(source: str) -> tuple[list[str], int, int]:
    """Entry names plus the span of the slice body.

    Anchored on the whole declaration, not on the next `[` after it: the type
    `&[&str]` carries a bracket of its own, and starting there rewrites the
    declaration into something that does not compile.
    """
    start = source.index(DECLARATION)
    body_start = start + len(DECLARATION)
    end = source.index("];", body_start)
    return re.findall(r'"([^"]+)"', source[body_start:end]), body_start, end


def render(binary: Path, path: Path) -> str:
    out = subprocess.run(
        [str(binary), str(path)], capture_output=True, text=True, check=False
    )
    if out.returncode != 0:
        return f"<<engine exited {out.returncode}>>\n{out.stderr.strip()}"
    return out.stdout.strip()


def main() -> int:
    check_only = "--check" in sys.argv[1:]
    if not CORPUS.is_dir():
        print(f"no corpus at {CORPUS} - run: git submodule update --init", file=sys.stderr)
        return 2

    source = TESTS.read_text()
    allowed, body_start, body_end = read_allowlist(source)
    gap_start = source.index(GAPS_DECLARATION) + len(GAPS_DECLARATION)
    gap_end = source.index("];", gap_start)
    known_gaps = re.findall(r'"([^"]+)"', source[gap_start:gap_end])

    pairs: dict[str, list[str]] = {}
    stems = {
        strip_leading_number(crv.stem)
        for crv in CORPUS.glob("*.crv")
        if crv.with_suffix(".html").exists()
    }
    for crv in sorted(CORPUS.glob("*.crv")):
        if not crv.with_suffix(".html").exists():
            continue
        pairs.setdefault(base_category(crv.stem, stems), []).append(crv.stem)

    missing = [c for c in pairs if c not in allowed and c not in known_gaps]
    if not missing:
        print(f"sync-implemented: all {len(pairs)} corpus categories are in IMPLEMENTED.")
        return 0

    binary = next(
        (p for p in (ROOT / "target/release/carve", ROOT / "target/debug/carve") if p.exists()),
        None,
    )
    if binary is None:
        print("no built binary - run: cargo build --release", file=sys.stderr)
        return 2

    passing, failing = [], []
    for category in sorted(missing):
        bad = None
        for slug in pairs[category]:
            expected = (CORPUS / f"{slug}.html").read_text().strip()
            actual = render(binary, CORPUS / f"{slug}.crv")
            if actual != expected:
                bad = (slug, expected, actual)
                break
        (failing if bad else passing).append((category, bad))

    for category, _ in passing:
        print(f"  renders byte-exact, adding: {category}")
    for category, bad in failing:
        slug, expected, actual = bad
        print(f"  NOT implemented, left out:  {category}  (first mismatch: {slug})")
        print(f"      expected: {expected.splitlines()[0][:90] if expected else '(empty)'}")
        print(f"      actual:   {actual.splitlines()[0][:90] if actual else '(empty)'}")

    if passing and not check_only:
        # APPENDED, not merged-and-sorted. The list is in no particular order and
        # rewriting it sorted turns a two-line bump into a 400-line diff nobody
        # can review - and the review is the point: each new entry is a claim
        # that this engine renders the category byte-exact.
        indent = "    "
        added = "".join(f'{indent}"{c}",\n' for c, _ in passing)
        TESTS.write_text(source[:body_end] + added + source[body_end:])
        print(f"sync-implemented: added {len(passing)} category(ies) to IMPLEMENTED.")

    if failing:
        print(
            f"\nsync-implemented: {len(failing)} category(ies) still need work in the engine. "
            "An IMPLEMENTED entry asserts byte-exact rendering, so these are not added.",
            file=sys.stderr,
        )
        return 1

    if passing and check_only:
        # `--check` asks whether the list is COMPLETE, and it is not: these
        # categories pass and are still missing. Exiting 0 here would let CI
        # report success on the state this tool exists to fix.
        print(
            f"\nsync-implemented: {len(passing)} category(ies) are missing from IMPLEMENTED. "
            "Run without --check to add them.",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
