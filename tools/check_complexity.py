#!/usr/bin/env python3
"""Reject Rust functions with cyclomatic complexity of 100 or more."""

import sys
from pathlib import Path

import lizard


def main() -> int:
    roots = [Path(arg) for arg in sys.argv[1:]] or [Path("src")]
    files = sorted(path for root in roots for path in (root.rglob("*.rs") if root.is_dir() else [root]))
    violations = [
        (function.cyclomatic_complexity, path, function.start_line, function.name)
        for path in files
        for function in lizard.analyze_file(str(path)).function_list
        if function.cyclomatic_complexity >= 100
    ]
    for score, path, line, name in sorted(violations, reverse=True):
        print(f"{path}:{line}: {name}: cyclomatic complexity {score} (maximum 99)")
    print(f"Scanned {len(files)} Rust files; {len(violations)} functions at 100 or above.")
    return bool(violations)


if __name__ == "__main__":
    raise SystemExit(main())
