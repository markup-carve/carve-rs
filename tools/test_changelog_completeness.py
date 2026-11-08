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

    def test_a_merge_commit_pull_request_counts_and_its_branch_commits_do_not(self):
        with tempfile.TemporaryDirectory(prefix="carve-changelog-test-") as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()

            git("init", "-q", "-b", "main")
            git("config", "user.email", "test@example.invalid")
            git("config", "user.name", "Test")
            (root / "Cargo.toml").write_text('[package]\nversion = "0.1.7"\n')
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\n## [0.1.7]\n\nReleased.\n")
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("pub fn a() {}\n")
            git("add", ".")
            git("commit", "-qm", "Release")
            git("tag", "0.1.7")
            git("switch", "-qc", "fix/branch")
            (root / "src/lib.rs").write_text("pub fn b() {}\n")
            git("commit", "-qam", "Work on the branch")
            git("switch", "-q", "main")
            git("merge", "-q", "--no-ff", "fix/branch", "-m", "Merge pull request #100 from test/fix/branch")

            def run():
                output = io.StringIO()
                with patch("sys.argv", ["check", "--root", str(root), "--repo", "test/repo"]), patch.object(changelog, "closing_issues", return_value={}), contextlib.redirect_stdout(output):
                    status = changelog.main()
                return status, output.getvalue()

            status, output = run()
            self.assertEqual(3, status, output)
            self.assertIn("#100 is not cited in the Unreleased section", output)
            self.assertNotIn("Work on the branch", output, "a merged branch's own commit is not a separate change")
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\nFix (#100).\n\n## [0.1.7]\n\nReleased.\n")
            git("commit", "-qam", "Document fix")
            self.assertEqual(0, run()[0])

    def test_reverted_source_commit_does_not_require_a_pull_request(self):
        with tempfile.TemporaryDirectory(prefix="carve-changelog-test-") as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()

            git("init", "-q")
            git("config", "user.email", "test@example.invalid")
            git("config", "user.name", "Test")
            (root / "Cargo.toml").write_text('[package]\nversion = "0.1.7"\n')
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\n## [0.1.7]\n\nReleased.\n")
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("pub fn original() {}\n")
            git("add", ".")
            git("commit", "-qm", "Release")
            git("tag", "0.1.7")
            (root / "src/lib.rs").write_text("pub fn abandoned() {}\n")
            git("commit", "-qam", "Abandoned fix (#99)")
            git("revert", "--no-edit", "HEAD")
            revert_sha = git("rev-parse", "HEAD")
            (root / "src/extra.rs").write_text("pub fn final_fix() {}\n")
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\nFinal fix (#100).\n\n## [0.1.7]\n\nReleased.\n")
            git("add", ".")
            git("commit", "-qm", "Final fix (#100)")

            with patch("sys.argv", ["check", "--root", str(root), "--repo", "test/repo"]), patch.object(changelog, "closing_issues", return_value={}) as closes:
                self.assertEqual(0, changelog.main())
            self.assertEqual(["100"], closes.call_args.args[2])

            git("revert", "--no-edit", revert_sha)
            output = io.StringIO()
            with patch("sys.argv", ["check", "--root", str(root), "--repo", "test/repo"]), patch.object(changelog, "closing_issues", return_value={}), contextlib.redirect_stdout(output):
                self.assertEqual(3, changelog.main())
            self.assertIn("#99 is not cited", output.getvalue())

    def test_revert_of_prior_release_change_still_requires_a_citation(self):
        with tempfile.TemporaryDirectory(prefix="carve-changelog-test-") as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()

            git("init", "-q")
            git("config", "user.email", "test@example.invalid")
            git("config", "user.name", "Test")
            (root / "Cargo.toml").write_text('[package]\nversion = "0.1.7"\n')
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n\n## [0.1.7]\n\nReleased.\n")
            git("add", ".")
            git("commit", "-qm", "Start release")
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("pub fn old_behavior() {}\n")
            git("add", ".")
            git("commit", "-qm", "Old behavior (#98)")
            old_sha = git("rev-parse", "HEAD")
            git("tag", "0.1.7")
            git("revert", "--no-commit", old_sha)
            git("commit", "-qm", "Remove old behavior (#101)", "-m", f"This reverts commit {old_sha}.")

            output = io.StringIO()
            with patch("sys.argv", ["check", "--root", str(root), "--repo", "test/repo"]), patch.object(changelog, "closing_issues", return_value={}), contextlib.redirect_stdout(output):
                self.assertEqual(3, changelog.main())
            self.assertIn("#101 is not cited", output.getvalue())


if __name__ == "__main__":
    unittest.main()
