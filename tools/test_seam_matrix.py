#!/usr/bin/env python3
"""The seam matrix's own coverage gate, proven to fire.

    python3 -m unittest discover -s tools -p 'test_*.py'

`--check` walked `FAMILIES`, so it could only ever ask about the families
already registered: a generator function nobody wired in was never generated and
never checked, and the check stayed green (markup-carve/carve-rs#1972). The test
below adds exactly that - a real family, unregistered - and requires the
refusal, with the registered spelling as the control.
"""

from __future__ import annotations

import importlib.util
import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent

spec = importlib.util.spec_from_file_location("seam_matrix", HERE / "seam-matrix.py")
seam_matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(seam_matrix)

# DEFINED INSIDE THE MODULE UNDER TEST, the way an edit to the file would define
# it. The registration check reads `__module__` so an imported generator is not
# mistaken for a family, and a probe written in THIS file would carry this file's
# name and be skipped rather than refused.
PROBE = "\n".join(
    [
        "def intraword():",
        '    """A family this module does not register: a bare marker in a word."""',
        "    for marker in BARE:",
        '        yield f"intraword|{marker}", f"a{marker}x{marker}b"',
    ]
)


def add_probe():
    exec(PROBE, seam_matrix.__dict__)
    assert seam_matrix.intraword.__module__ == seam_matrix.__name__

    return seam_matrix.intraword


def run_check():
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        code = seam_matrix.check()

    return code, out.getvalue() + err.getvalue()


class UnregisteredFamily(unittest.TestCase):
    def tearDown(self):
        seam_matrix.__dict__.pop("intraword", None)
        seam_matrix.FAMILIES.pop("intraword", None)

    def test_the_module_as_committed_passes(self):
        code, output = run_check()
        self.assertEqual(code, 0, output)
        self.assertIn("distinct sources across", output)

    def test_a_generator_nobody_registered_is_refused(self):
        add_probe()
        code, output = run_check()
        self.assertEqual(code, 1, output)
        self.assertIn("`intraword` is a generator function", output)

    def test_the_same_generator_registered_passes(self):
        """The control: the refusal came from the registration, not the family."""
        seam_matrix.FAMILIES["intraword"] = add_probe()
        code, output = run_check()
        self.assertEqual(code, 0, output)
        self.assertIn("family intraword:", output)

    def test_plumbing_is_named_rather_than_guessed(self):
        """`rows` yields and is not a family, so it is listed, not pattern-matched."""
        self.assertIn("rows", seam_matrix.NOT_A_FAMILY)
        for name in seam_matrix.NOT_A_FAMILY:
            self.assertIn(name, seam_matrix.__dict__, f"{name} is not in this module")

    def test_an_emptied_family_still_fires(self):
        """The gate that already worked, kept beside the new one."""
        original = seam_matrix.FAMILIES["chain"]
        seam_matrix.FAMILIES["chain"] = lambda: iter(())
        try:
            code, output = run_check()
        finally:
            seam_matrix.FAMILIES["chain"] = original
        self.assertEqual(code, 1, output)
        self.assertIn("family `chain` generates no rows", output)


if __name__ == "__main__":
    unittest.main()
