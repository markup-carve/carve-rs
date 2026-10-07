import importlib.util
from pathlib import Path
import unittest
import json
import os
import subprocess
import sys
import tempfile

spec = importlib.util.spec_from_file_location("release_notes", Path(__file__).with_name("check-release-notes.py"))
notes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notes)

REPO = "markup-carve/carve-wasm"
CHANGELOG = """# Changelog

## [Unreleased]

### Fixes
- A future change (#999).

## [0.1.5] - 2026-09-29

### Breaking

- Refuse unsafe destinations under a new loss code (#158, #159; markup-carve/carve#2679).

### Fixes

- Move the engine to 0.1.7 (#143).
- Keep a reference label's raw identity on import (#151).

### Improvements

- Faster rendering. See the [measurements](reports/parser.md) (#160).

## [0.1.4] - 2026-09-01

### Fixes

- An earlier release (#100).

[0.1.5]: https://github.com/markup-carve/carve-wasm/compare/v0.1.4...v0.1.5
"""
LINK = "Details in the [CHANGELOG](https://github.com/markup-carve/carve-wasm/blob/v0.1.5/CHANGELOG.md)."
FOOTER = "**Full Changelog**: https://github.com/markup-carve/carve-wasm/compare/v0.1.4...v0.1.5"
CONDENSED = f"""### Breaking

- Unsafe destinations are refused (#158).

### Fixes

- Import and engine fixes (#143, #151).

{LINK}
"""


class ReleaseNotesTest(unittest.TestCase):
    def check(self, body, changelog=CHANGELOG, tag="v0.1.5", repo=REPO):
        notes.check_release(changelog, {"tag_name": tag, "body": body}, repo, tag)

    def test_condensed_summary_passes(self):
        self.check(CONDENSED + "\n" + FOOTER)

    def test_entries_may_be_left_out(self):
        self.check("### Breaking\n\n- Unsafe destinations are refused (#159).\n\n" + LINK + "\n" + FOOTER)

    def test_crlf_trailing_spaces_and_footer_padding(self):
        self.check((CONDENSED + "  \n" + FOOTER + "\n").replace("\n", "\r\n"))

    def test_cross_repository_reference_counts_for_breaking(self):
        self.check("- Breaking: refused destinations (markup-carve/carve#2679).\n\n" + LINK + "\n" + FOOTER)

    def test_fully_qualified_same_repository_reference_matches(self):
        self.check("- Refused destinations (markup-carve/carve-wasm#158).\n\n" + LINK + "\n" + FOOTER)

    def test_reference_the_section_does_not_hold_fails(self):
        with self.assertRaisesRegex(ValueError, "does not: markup-carve/carve-wasm#999"):
            self.check(CONDENSED.replace("(#143, #151)", "(#143, #999)") + "\n" + FOOTER)

    def test_url_citations_count_as_references(self):
        with self.assertRaisesRegex(ValueError, "#999"):
            self.check(CONDENSED + "- See [PR](https://github.com/markup-carve/carve-wasm/pull/999).\n" + FOOTER)
        self.check("- Refused destinations ([PR](https://github.com/markup-carve/carve-wasm/pull/158)).\n\n" + LINK + "\n" + FOOTER)

    def test_reference_from_another_release_fails(self):
        with self.assertRaisesRegex(ValueError, "#100"):
            self.check(CONDENSED + "- Also (#100).\n" + FOOTER)

    def test_left_out_breaking_change_fails(self):
        with self.assertRaisesRegex(ValueError, "breaking change"):
            self.check("### Fixes\n\n- Import fixes (#151).\n\n" + LINK + "\n" + FOOTER)

    def test_breaking_entry_without_references_is_not_checked(self):
        changelog = CHANGELOG.replace("(#158, #159; markup-carve/carve#2679)", "")
        self.check("### Fixes\n\n- Import fixes (#151).\n\n" + LINK + "\n" + FOOTER, changelog=changelog)

    def test_placeholder_without_references_fails(self):
        changelog = CHANGELOG.replace("(#158, #159; markup-carve/carve#2679)", "")
        with self.assertRaisesRegex(ValueError, "cite none"):
            self.check("Bug fixes and improvements.\n\n" + LINK + "\n" + FOOTER, changelog=changelog)

    def test_changelog_link_is_not_constrained(self):
        """A changelog link is optional, so the gate does not police where it points."""
        for other in ["https://github.com/markup-carve/carve-py/blob/v0.1.5/CHANGELOG.md",
                      "https://github.com/markup-carve/carve-wasm/blob/v0.1.4/CHANGELOG.md",
                      "https://example.test/CHANGELOG.md"]:
            self.check(CONDENSED.replace("https://github.com/markup-carve/carve-wasm/blob/v0.1.5/CHANGELOG.md", other) + "\n" + FOOTER)

    def test_shared_reference_does_not_cover_two_breaking_changes(self):
        changelog = CHANGELOG.replace("- Refuse unsafe destinations under a new loss code (#158, #159; markup-carve/carve#2679).",
                                      "- Refuse unsafe destinations (#158, #170).\n- Rename the diagnostics field (#158, #171).")
        with self.assertRaisesRegex(ValueError, "Rename the diagnostics"):
            self.check("### Breaking\n\n- Unsafe destinations are refused (#158, #170).\n\n" + LINK + "\n" + FOOTER, changelog=changelog)
        self.check("### Breaking\n\n- Refused destinations and a renamed field (#170, #171).\n\n" + LINK + "\n" + FOOTER, changelog=changelog)

    def test_breaking_entry_with_only_shared_references_fails(self):
        changelog = CHANGELOG.replace("- Refuse unsafe destinations under a new loss code (#158, #159; markup-carve/carve#2679).",
                                      "- Refuse unsafe destinations (#158, #170).\n- Rename the diagnostics field (#158).")
        with self.assertRaisesRegex(ValueError, "reference of its own"):
            self.check("### Breaking\n\n- Refused destinations and a renamed field (#158, #170).\n\n" + LINK + "\n" + FOOTER, changelog=changelog)

    def test_notes_without_a_changelog_link_pass(self):
        self.check(CONDENSED.replace(LINK, "") + "\n" + FOOTER)

    def test_relative_link_fails(self):
        with self.assertRaisesRegex(ValueError, "relative link"):
            self.check(CONDENSED.replace(LINK, "Details in the [CHANGELOG](CHANGELOG.md).") + "\n" + FOOTER)

    def test_anchor_in_changelog_link_is_not_a_reference(self):
        anchored = LINK.replace("CHANGELOG.md)", "CHANGELOG.md#015---2026-09-29)")
        self.check(CONDENSED.replace(LINK, anchored) + "\n" + FOOTER)

    def test_literal_link_syntax_is_unchanged_inside_code(self):
        for source in ["`[t](a.md)`", "``[t](a.md)``", "```carve\n[t](a.md)\n```", "~~~\n[t](a.md)\n~~~"]:
            self.assertEqual(notes.release_links(source, REPO, "v0.1.5"), source)

    def test_root_relative_links_resolve_at_the_tag(self):
        self.assertEqual(notes.release_links("[x](/docs/x.md)", REPO, "v0.1.5"),
                         "[x](https://github.com/markup-carve/carve-wasm/blob/v0.1.5/docs/x.md)")

    def test_piped_json_is_utf8_even_with_a_windows_locale(self):
        with tempfile.TemporaryDirectory() as directory:
            changelog = Path(directory, "CHANGELOG.md")
            changelog.write_text("## [0.1.0]\n\n- Section §4.2 … corrected.\n", encoding="utf-8")
            body = ("- Section §4.2 … corrected.\n\nSee the [CHANGELOG](https://github.com/markup-carve/carve-wasm/blob/v0.1.0/CHANGELOG.md).\n"
                    "**Full Changelog**: https://github.com/markup-carve/carve-wasm/releases/tag/v0.1.0")
            result = subprocess.run([sys.executable, str(Path(__file__).with_name("check-release-notes.py")),
                                     "--tag", "v0.1.0", "--repo", REPO, "--changelog", str(changelog)],
                                    input=json.dumps({"tag_name": "v0.1.0", "body": body}, ensure_ascii=False).encode("utf-8"),
                                    capture_output=True, env={**os.environ, "PYTHONIOENCODING": "cp1252"})
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_wrong_repo_or_version_footer_fails(self):
        for footer in [FOOTER.replace("carve-wasm", "carve-py"), FOOTER.replace("...v0.1.5", "...v0.1.4")]:
            with self.assertRaisesRegex(ValueError, "footer"):
                self.check(CONDENSED + "\n" + footer)

    def test_footer_alone_fails(self):
        with self.assertRaisesRegex(ValueError, "no notes above the footer"):
            self.check(FOOTER)

    def test_missing_release_or_empty_body_fails(self):
        for release in [None, {"tag_name": "v0.1.4", "body": CONDENSED}, {"tag_name": "v0.1.5", "body": " "}]:
            with self.assertRaises(ValueError):
                notes.check_release(CHANGELOG, release, REPO, "v0.1.5")

    def test_missing_or_duplicate_version_section_fails(self):
        for changelog in [CHANGELOG.replace("[0.1.5]", "[0.1.6]"), CHANGELOG + "\n## [0.1.5]\n- Duplicate.\n"]:
            with self.assertRaisesRegex(ValueError, "exactly one"):
                self.check(CONDENSED + "\n" + FOOTER, changelog=changelog)

    def test_empty_section_fails(self):
        with self.assertRaisesRegex(ValueError, "has no notes"):
            self.check(CONDENSED + "\n" + FOOTER, changelog="## [0.1.5]\n\n## [0.1.4]\n- Earlier.\n")

    def test_unprefixed_tag(self):
        self.check(CONDENSED.replace("/v0.1.5/", "/0.1.5/") + "\n" + FOOTER.replace("v0.1", "0.1"), tag="0.1.5")

    def test_first_release_footer(self):
        first = "## [0.1.0]\n\n- Initial release (#1).\n\n[0.1.0]: https://example.test/tag\n"
        self.check("- Initial release (#1).\n\nSee the [CHANGELOG](https://github.com/markup-carve/carve-wasm/blob/v0.1.0/CHANGELOG.md).\n"
                   "**Full Changelog**: https://github.com/markup-carve/carve-wasm/releases/tag/v0.1.0", changelog=first, tag="v0.1.0")


if __name__ == "__main__":
    unittest.main()
