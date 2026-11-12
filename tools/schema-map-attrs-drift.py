#!/usr/bin/env python3
"""Compare the vendored schema map's node attributes with carve-grammars.

`tools/check-schema-map.py` gates the DECISIONS (names, kinds, accepts) on
every pull request and leaves `attrs` alone, so attribute renames upstream
went unseen (markup-carve/carve-rs#4292: `alignment` versus `textAlign`).
This reports every type both maps name whose `attrs` differ, skipping the
types `_provenance.divergences` already declares.

    python3 tools/schema-map-attrs-drift.py <vendored map> <upstream map>

Exit 0 identical, 1 drift (printed), 2 usage error.
"""

from __future__ import annotations

import json
import sys


def attrs(path: str) -> dict:
    with open(path, encoding="utf-8") as handle:
        root = json.load(handle)
    declared = (root.get("_provenance") or {}).get("divergences") or {}
    out = {"__declared__": {f"types.{name}" for name in declared}}
    for section in ("types", "markCarrierNodes", "preservationNodes"):
        for name, entry in (root.get(section) or {}).items():
            if isinstance(entry, dict):
                out[f"{section}.{name}"] = entry.get("attrs")
    return out


def declared(table: dict) -> set:
    return table.pop("__declared__", set())


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        local, upstream = attrs(argv[0]), attrs(argv[1])
    except (OSError, json.JSONDecodeError) as exc:
        print(f"schema-map-attrs-drift: {exc}", file=sys.stderr)
        return 2
    skip = declared(local) | declared(upstream)
    drift = []
    for key in sorted((set(local) & set(upstream)) - skip):
        if local[key] != upstream[key]:
            mine, theirs = local[key] or {}, upstream[key] or {}
            names = sorted(set(mine) ^ set(theirs))
            changed = sorted(k for k in set(mine) & set(theirs) if mine[k] != theirs[k])
            drift.append(f"{key}: only on one side {names}, described differently {changed}")
    for line in drift:
        print(line)
    return 1 if drift else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
