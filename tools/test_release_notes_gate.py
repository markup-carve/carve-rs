"""The release-notes gate wrapper must run wherever Python is called python3.

`tools/run-release-notes-gate.sh` hard-coded `python`, a name that exists in CI
only because actions/setup-python shims it. On a machine carrying `python3`
alone the gate died with exit 127, so the one check a human wants before
publishing a draft could not be run outside CI (carve-rs#2344).

These tests stub PATH rather than relying on the host, so they hold on a runner
where both names resolve. Nothing here reaches the network: the interpreter is
resolved before the release lookup.
"""

import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

GATE = Path(__file__).with_name("run-release-notes-gate.sh")
ROOT = GATE.parent.parent
# Absolute, because PATH below holds the stubs alone: an interpreter reachable
# through the real PATH would decide the test instead of the stub.
BASH = shutil.which("bash") or "/bin/bash"


def _stub(directory: Path, name: str, body: str) -> None:
    path = directory / name
    path.write_text("#!/bin/sh\n" + body, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


def _run(path_dir: Path, **env):
    environment = {
        "PATH": str(path_dir),
        "GITHUB_REPOSITORY": "markup-carve/carve-rs",
        "GH_TOKEN": "stub",
        **env,
    }
    return subprocess.run(
        [BASH, str(GATE), "0.0.0-test"],
        cwd=ROOT,
        env=environment,
        capture_output=True,
        text=True,
    )


class ReleaseNotesGateInterpreterTests(unittest.TestCase):
    def test_the_gate_runs_where_only_python3_exists(self):
        with tempfile.TemporaryDirectory(prefix="carve-gate-test-") as directory:
            stubs = Path(directory)
            _stub(stubs, "python3", 'echo "ran under python3"\nexit 0\n')
            _stub(stubs, "gh", 'echo "[]"\n')
            _stub(stubs, "jq", 'echo "{\\"tag_name\\": \\"0.0.0-test\\"}"\n')

            result = _run(stubs)

            self.assertEqual(0, result.returncode, result.stderr)
            self.assertIn("ran under python3", result.stdout)

    def test_python_alone_is_still_enough(self):
        with tempfile.TemporaryDirectory(prefix="carve-gate-test-") as directory:
            stubs = Path(directory)
            _stub(stubs, "python", 'echo "ran under python"\nexit 0\n')
            _stub(stubs, "gh", 'echo "[]"\n')
            _stub(stubs, "jq", 'echo "{\\"tag_name\\": \\"0.0.0-test\\"}"\n')

            result = _run(stubs)

            self.assertEqual(0, result.returncode, result.stderr)
            self.assertIn("ran under python", result.stdout)

    def test_neither_name_present_fails_and_says_what_was_missing(self):
        with tempfile.TemporaryDirectory(prefix="carve-gate-test-") as directory:
            stubs = Path(directory)
            _stub(stubs, "gh", 'echo "[]"\n')
            _stub(stubs, "jq", 'echo ""\n')

            result = _run(stubs)

            self.assertEqual(1, result.returncode)
            self.assertIn("No Python interpreter found", result.stdout + result.stderr)
            self.assertNotEqual(127, result.returncode)

    def test_the_wrapper_names_no_interpreter_in_the_pipeline(self):
        text = GATE.read_text(encoding="utf-8")
        self.assertNotIn("| python ", text)
        self.assertNotIn("| python3 ", text)
        self.assertIn('"$python_bin" tools/check-release-notes.py', text)


if __name__ == "__main__":
    unittest.main()
