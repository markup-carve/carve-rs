#!/usr/bin/env python3
"""Run the Echo binding against every shared Carve HTML fixture."""

from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile


binding = Path(__file__).resolve().parent
repository = binding.parent.parent
corpus = repository / "tests" / "spec" / "tests" / "corpus"
executable = binding / "examples" / "carve-echo"
library = binding / "native" / "target" / "release"
metadata = subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--no-deps"],
    cwd=repository,
    capture_output=True,
    check=True,
    text=True,
)
rust_executable = Path(json.loads(metadata.stdout)["target_directory"]) / "release" / "carve"

environment = os.environ.copy()
environment["LD_LIBRARY_PATH"] = os.pathsep.join(
    filter(None, [str(library), environment.get("LD_LIBRARY_PATH")])
)

files = sorted(corpus.glob("*.crv"))
if not files:
    sys.exit(f"no Carve fixtures found under {corpus}")

failures = []
for source in files:
    rendered = subprocess.run(
        [executable, source],
        capture_output=True,
        env=environment,
        check=False,
    )
    native = subprocess.run(
        [rust_executable, source],
        capture_output=True,
        check=False,
    )
    if (
        rendered.returncode
        or native.returncode
        or rendered.stdout.rstrip() != native.stdout.rstrip()
        or rendered.stderr
    ):
        failures.append(
            f"{source.name}: Echo stderr={rendered.stderr.decode(errors='replace')!r}"
        )

if failures:
    print(f"{len(failures)} of {len(files)} fixtures failed", file=sys.stderr)
    print("\n".join(failures[:20]), file=sys.stderr)
    sys.exit(1)

with tempfile.NamedTemporaryFile() as invalid:
    invalid.write(b"\xff")
    invalid.flush()
    rejected = subprocess.run(
        [executable, invalid.name],
        capture_output=True,
        env=environment,
        check=False,
    )
    if rejected.returncode == 0 or b"status 2" not in rejected.stderr:
        sys.exit("Echo binding did not reject invalid UTF-8 with status 2")

print(f"Echo binding matched native Rust for {len(files)} Carve fixtures")
