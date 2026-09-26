#!/usr/bin/env python3
"""Generate the inline-seam matrix the engines sweep for writer defects.

Emits JSONL rows of `{"id": ..., "src": ...}`. Every id starts with its family
name. `--check` asserts every family is registered and populated and every named
shape has rows, so an edit cannot drop one without going red
(markup-carve/carve-rs#1644, #1972).

WHAT `--check` DOES NOT SEE. It reads this generator and nothing else. No job in
this repository or in the spec repo runs the emitted JSONL through an engine, so
nothing automatic compares a writer's output against these sources: a defect the
matrix would expose stays exposed only for whoever sweeps it by hand. `--check`
guards the INPUT, not a result.

    python3 tools/seam-matrix.py matrix.jsonl
    python3 tools/seam-matrix.py --check
"""

from __future__ import annotations

import argparse
import inspect
import json
import sys

# Every marker that pairs inside braces.
BRACED = ["/", "*", "_", "~", "=", "^", "-", "+"]
# The subset that also pairs bare. `^`, `-` and `+` pair only inside braces, so a
# bare row built from them measures the parser rather than the writer.
BARE = ["/", "*", "_", "~", "="]

INNERS = ["x", "~x", "x~", "~~x", "x~~", "*x", "x*", "**x", "x**", "x!"]
INNERS2 = ["y", "~y", "y~"]

# A bare leading `~` is a strike marker in Carve, so a context opening with one
# measures the parser; `~~` is text, and is the line-start case.
CTXS = [
    ("", ""),
    ("a ", ""),
    ("", "b"),
    ("a ", "b"),
    ("a", "b"),
    ("a*", ""),
    ("", "*b"),
    ("a~", ""),
    ("", "~b"),
    ("a~~", ""),
    ("", "~~b"),
    ("~~", ""),
]
# The short context set for the families that multiply out fastest.
CTXS_SHORT = [("", ""), ("a ", "b"), ("~~", "")]


def braced():
    for m1 in BRACED:
        for i1 in INNERS:
            for pre, post in CTXS:
                yield f"braced|{m1}|{i1}|none||{pre}|{post}", f"{pre}{{{m1}{i1}{m1}}}{post}"
                for m2 in BRACED:
                    for i2 in INNERS2:
                        yield (
                            f"braced|{m1}|{i1}|{m2}|{i2}|{pre}|{post}",
                            f"{pre}{{{m1}{i1}{m1}}}{{{m2}{i2}{m2}}}{post}",
                        )


def chain():
    """Three spans in a row, because a two-node rule does not obviously compose."""
    for m1 in BRACED:
        for m2 in BRACED:
            for m3 in BRACED:
                for pre, post in (("", ""), ("a ", "b")):
                    yield (
                        f"chain|{m1}{m2}{m3}|{pre}|{post}",
                        f"{pre}{{{m1}x{m1}}}{{{m2}y{m2}}}{{{m3}z{m3}}}{post}",
                    )


def bare():
    for m1 in BARE:
        for i1 in INNERS:
            for pre, post in CTXS:
                yield f"bare|{m1}|{i1}|none||{pre}|{post}", f"{pre}{m1}{i1}{m1}{post}"
                for m2 in BARE:
                    for i2 in INNERS2:
                        yield (
                            f"bare|{m1}|{i1}|{m2}|{i2}|{pre}|{post}",
                            f"{pre}{m1}{i1}{m1}{m2}{i2}{m2}{post}",
                        )


def nest():
    """One bare marker inside another.

    With `m1 == m2` the source parses as literal text (`//x//`), so those rows
    measure escaping; same-strength NESTING comes from `wrap` and `enclose`.
    """
    for m1 in BARE:
        for m2 in BARE:
            for i1 in INNERS:
                for pre, post in CTXS_SHORT:
                    yield (
                        f"nest|{m1}{m2}|{i1}|{pre}|{post}",
                        f"{pre}{m1}{m2}{i1}{m2}{m1}{post}",
                    )


def wrap():
    """A bare marker wrapping a braced inline: `/{*x*}/`, and `/{/x/}/` nests."""
    for m1 in BARE:
        for m2 in BRACED:
            for i1 in INNERS:
                for pre, post in CTXS_SHORT:
                    yield (
                        f"wrap|{m1}|{m2}|{i1}|{pre}|{post}",
                        f"{pre}{m1}{{{m2}{i1}{m2}}}{m1}{post}",
                    )


def enclose():
    """A braced inline wrapping a bare marker: `{*/x/*}`, and `{//x//}` nests."""
    for m1 in BRACED:
        for m2 in BARE:
            for i1 in INNERS:
                for pre, post in CTXS_SHORT:
                    yield (
                        f"enclose|{m1}|{m2}|{i1}|{pre}|{post}",
                        f"{pre}{{{m1}{m2}{i1}{m2}{m1}}}{post}",
                    )


FAMILIES = {
    "braced": braced,
    "chain": chain,
    "bare": bare,
    "nest": nest,
    "wrap": wrap,
    "enclose": enclose,
}

# Generator functions that are plumbing rather than a family. Spelled out so
# adding one is a visible decision: anything else this module yields from has to
# be registered above or the check refuses it.
NOT_A_FAMILY = frozenset({"rows"})

# Shapes the matrix was blind to, each with a row that must exist. A populated
# family is not enough: `nest` had rows while no row parsed as same-strength
# nesting. The nesting spellings are ones the parser reads as nested.
REQUIRED_SHAPES = [
    ("a marker nested inside itself, bare around braced", "/{/x/}/"),
    ("a marker nested inside itself, braced around bare", "{//x//}"),
    ("a bare marker wrapping a braced inline", "/{*x*}/"),
    ("a bare marker at an intraword position", "a/x/b"),
    ("a doubled bare marker, which parses as text and measures escaping", "**x**"),
]


def rows(only=None):
    for name, family in FAMILIES.items():
        if only and name != only:
            continue
        yield from family()


def unregistered_generators() -> list:
    """Generator functions in this module that `FAMILIES` does not name.

    `--check` walks `FAMILIES`, so it could only ever ask about the families
    already registered: a new generator nobody wired in was never generated and
    never checked, and the check stayed green (markup-carve/carve-rs#1972). The
    enumeration is the module's own namespace, which is the one thing an author
    adding a family cannot forget to update.
    """
    registered = {family for family in FAMILIES.values()}
    return sorted(
        name
        for name, value in sorted(globals().items())
        if inspect.isgeneratorfunction(value)
        and getattr(value, "__module__", None) == __name__
        and name not in NOT_A_FAMILY
        and value not in registered
    )


def check() -> int:
    produced = {}
    sources = set()
    for rid, src in rows():
        produced[rid.split("|", 1)[0]] = produced.get(rid.split("|", 1)[0], 0) + 1
        sources.add(src)

    failures = []
    for name in unregistered_generators():
        failures.append(
            f"`{name}` is a generator function this module defines and `FAMILIES` "
            f"does not name, so nothing generates or checks it"
        )
    for name in FAMILIES:
        count = produced.get(name, 0)
        if count == 0:
            failures.append(f"family `{name}` generates no rows")
        else:
            print(f"family {name}: {count} rows")
    for reason, src in REQUIRED_SHAPES:
        if src in sources:
            print(f"shape present: {reason} ({src})")
        else:
            failures.append(f"no row spells {reason}: `{src}` is in no family")

    for failure in failures:
        print(f"error: {failure}", file=sys.stderr)
    if failures:
        return 1
    print(f"seam-matrix: {len(sources)} distinct sources across {len(FAMILIES)} families.")
    return 0


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("out", nargs="?", help="path to write the JSONL matrix to")
    p.add_argument("--only", choices=sorted(FAMILIES), help="emit one family")
    p.add_argument("--check", action="store_true", help="assert every family and shape is covered")
    args = p.parse_args(argv)

    if args.check:
        return check()
    if not args.out:
        p.error("give an output path, or --check")
    with open(args.out, "w", encoding="utf-8") as f:
        n = 0
        for rid, src in rows(args.only):
            f.write(json.dumps({"id": rid, "src": src}, ensure_ascii=False) + "\n")
            n += 1
    print(n)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
