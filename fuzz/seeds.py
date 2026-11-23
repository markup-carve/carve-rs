"""Writes the seed corpora of the fuzz targets to fuzz/corpus/<target>.

Seeds: every input of the conformance corpus (examples, clause cases,
extension cases) and the .crv files under tests/. The targets that read a
selector byte first (render, ext, safety, export, import) get the input behind fixed
selectors; decode gets the trees of spec-trees/. Usage: python3 fuzz/seeds.py <repo root> [--smoke] [extra Markdown file...]
"""

import hashlib
import json
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
out = root / "fuzz" / "corpus"
deferred_fmt = (root / "fuzz/fixtures/timeout-d1cfc854").read_bytes()


def put(target, data):
    if isinstance(data, str):
        data = data.encode("utf-8")
    if len(data) > 60000:
        return
    d = out / target
    d.mkdir(parents=True, exist_ok=True)
    (d / hashlib.sha1(data).hexdigest()).write_bytes(data)


def load(name):
    return json.loads((root / "spec-conformance" / name).read_text(encoding="utf-8"))


carve, html = [], []
for e in load("examples.json")["examples"] + load("clause-cases.json")["cases"]:
    if isinstance(e.get("input"), str):
        carve.append(e["input"])
    if isinstance(e.get("html"), str):
        html.append(e["html"])
    for f in (e.get("files") or {}).values():
        if isinstance(f, dict) and isinstance(f.get("text"), str):
            carve.append(f["text"])


def walk(v):
    if isinstance(v, dict):
        for k, x in v.items():
            if k in ("input", "source") and isinstance(x, str):
                carve.append(x)
            else:
                walk(x)
    elif isinstance(v, list):
        for x in v:
            walk(x)


walk(load("extensions.json"))
for p in sorted(root.glob("tests/**/*.crv")):
    carve.append(p.read_text(encoding="utf-8", errors="replace"))
markdown = [Path(p).read_text(encoding="utf-8") for p in sys.argv[2:] if p != "--smoke"]

carve = list(dict.fromkeys(carve))
for i, s in enumerate(carve):
    for t in ("parse", "fmt", "lint"):
        put(t, s)
    for sel in (0x00, (0x21, 0x46, 0x93, 0x5C)[i % 4]):
        put("render", bytes([sel]) + s.encode())
        put("ext", bytes([sel]) + s.encode())
        put("safety", bytes([sel]) + s.encode())
        put("export", bytes([sel]) + s.encode())
    put("import", bytes([0x01 + i % 3]) + s.encode())
# The carve-rs#4538 reproducer: its fmt call must stay inside the smoke budget.
put("fmt", deferred_fmt)
for s in html:
    put("import", b"\x00" + s.encode())
for s in markdown:
    put("import", b"\x01" + s.encode())
    put("import", b"\x02" + s.encode())
trees = 0
for f in sorted((root / "spec-trees").glob("*.json")):
    for t in json.loads(f.read_text(encoding="utf-8")).get("trees", []):
        if isinstance(t.get("tree"), dict):
            put("decode", json.dumps(t["tree"], ensure_ascii=False, separators=(",", ":")))
            trees += 1
print(f"{trees} trees, {len(carve)} Carve, {len(html)} HTML, {len(markdown)} Markdown seeds")
