#!/usr/bin/env python3
"""Reject hand-written Rust functions of the 0.2 engine above cyclomatic complexity 15."""

import sys
from pathlib import Path

import lizard

LIMIT = 15
# Exceptions: "path:function": reason with the benchmark number that justifies it.
EXCEPTIONS: dict[str, str] = {}


def main() -> int:
    roots = [Path(arg) for arg in sys.argv[1:]] or [Path("src"), Path("build.rs")]
    files = sorted(path for root in roots for path in (root.rglob("*.rs") if root.is_dir() else [root]))
    violations = [
        (function.cyclomatic_complexity, path, function.start_line, function.name)
        for path in files
        for function in lizard.analyze_file(str(path)).function_list
        if function.cyclomatic_complexity > LIMIT and f"{path}:{function.name}" not in EXCEPTIONS
    ]
    for score, path, line, name in sorted(violations, reverse=True):
        print(f"{path}:{line}: {name}: cyclomatic complexity {score} (maximum {LIMIT})")
    print(f"Scanned {len(files)} Rust files; {len(violations)} functions above {LIMIT}.")
    return bool(violations)


if __name__ == "__main__":
    raise SystemExit(main())
