import contextlib
import importlib.util
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("changelog", Path(__file__).with_name("check-changelog-completeness.py"))
changelog = importlib.util.module_from_spec(spec)
spec.loader.exec_module(changelog)


class ChangelogCompletenessTests(unittest.TestCase):
    def test_pending_headings_stop_at_the_first_merged_tag(self):
        body = "## [Unreleased]\n\n## [0.2.0] - soon\n\n## [0.1.7] - shipped\n"
        self.assertEqual(["Unreleased", "0.2.0"], changelog.pending_sections(body, ["v0.1.7"]))
        self.assertEqual([], changelog.pending_sections("## [0.1.7]\n", ["0.1.7"]))

    def test_default_checks_pending_merges_and_explicit_version_checks_the_tag(self):
        with tempfile.TemporaryDirectory(prefix="carve-changelog-test-") as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()

            git("init", "-q")
            git("config", "user.email", "test@example.invalid")
            git("config", "user.name", "Test")
            (root / "Cargo.toml").write_text('[package]\nversion = "0.1.7"\n')
            (root / "CHANGELOG.md").write_text("## [0.1.7]\n\nReleased.\n")
            git("add", ".")
            git("commit", "-qm", "Release")
            git("tag", "0.1.7")
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("pub fn fixed() {}\n")
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\n## [0.1.7]\n\nReleased.\n")
            git("add", ".")
            git("commit", "-qm", "Fix parser (#99)")

            def run(*args):
                output = io.StringIO()
                with patch("sys.argv", ["check", "--root", str(root), "--repo", "test/repo", *args]), patch.object(changelog, "closing_issues", return_value={}), contextlib.redirect_stdout(output):
                    status = changelog.main()
                return status, output.getvalue()

            status, output = run()
            self.assertEqual(3, status)
            self.assertIn("#99 is not cited in the Unreleased section", output)
            self.assertIn("since 0.1.7", output)
            self.assertEqual(0, run("0.1.7")[0])
            (root / ".changelog-exempt").write_text("not an exemption\n")
            self.assertEqual(1, run()[0], "a malformed exemption file is a broken check, not drift")
            (root / ".changelog-exempt").unlink()
            self.assertEqual(1, run("--section", "Nope")[0], "a missing section is a broken check, not drift")
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\nFix parser (#99).\n\n## [0.1.7]\n\nReleased.\n")
            git("add", ".")
            git("commit", "-qm", "Document fix")
            self.assertEqual(0, run()[0])

    def test_a_pull_request_merged_with_a_merge_commit_is_asked_for_an_entry(self):
        with tempfile.TemporaryDirectory(prefix="carve-changelog-test-") as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()

            git("init", "-q", "-b", "main")
            git("config", "user.email", "test@example.invalid")
            git("config", "user.name", "Test")
            (root / "Cargo.toml").write_text('[package]\nversion = "0.1.7"\n')
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\n## [0.1.7]\n\nReleased.\n")
            git("add", ".")
            git("commit", "-qm", "Release")
            git("tag", "0.1.7")
            git("checkout", "-qb", "wave")
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("pub fn fixed() {}\n")
            git("add", ".")
            git("commit", "-qm", "Fix the parser")
            git("checkout", "-q", "main")
            git("merge", "-q", "--no-ff", "wave", "-m", "Merge pull request #120 from o/wave\n\nFix a wave")

            output = io.StringIO()
            with patch("sys.argv", ["check", "--root", str(root), "--repo", "test/repo"]), patch.object(changelog, "closing_issues", return_value={}), contextlib.redirect_stdout(output):
                status = changelog.main()
            self.assertEqual(3, status)
            self.assertIn("#120 is not cited in the Unreleased section", output.getvalue())
            self.assertNotIn("Fix the parser", output.getvalue(), "the branch commit is not a pull request of its own")


if __name__ == "__main__":
    unittest.main()
