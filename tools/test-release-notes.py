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

CHANGELOG = """# Changelog

## [Unreleased]

### Changed
- A future change (#999).

## [0.1.5] - 2026-09-29

### Changed

- Move the engine to 0.1.7 (#143).
- Refuse unsafe destinations (#158).
- See the [measurements](reports/parser.md).

## [0.1.4] - 2026-09-01

### Added

- An earlier release (#100).

[0.1.5]: https://github.com/markup-carve/carve-wasm/compare/v0.1.4...v0.1.5
"""
SECTION = notes.changelog_section(CHANGELOG, "v0.1.5")[0]
BODY = notes.release_links(SECTION, "markup-carve/carve-wasm", "v0.1.5")
FOOTER = "**Full Changelog**: https://github.com/markup-carve/carve-wasm/compare/v0.1.4...v0.1.5"


class ReleaseNotesTest(unittest.TestCase):
    def check(self, body, changelog=CHANGELOG, tag="v0.1.5", repo="markup-carve/carve-wasm"):
        notes.check_release(changelog, {"tag_name": tag, "body": body}, repo, tag)

    def test_exact_section_with_footer(self):
        self.check(BODY + "\n\n" + FOOTER)

    def test_crlf_trailing_spaces_and_footer_padding(self):
        self.check((BODY + "  \n" + FOOTER + "\n").replace("\n", "\r\n"))

    def test_relative_links_can_be_resolved_to_the_release_tag(self):
        absolute = SECTION.replace("reports/parser.md", "https://github.com/markup-carve/carve-wasm/blob/v0.1.5/reports/parser.md")
        self.check(absolute + "\n" + FOOTER)

    def test_relative_links_in_the_release_body_fail(self):
        with self.assertRaisesRegex(ValueError, "differ"):
            self.check(SECTION + "\n" + FOOTER)

    def test_reference_definitions_do_not_hide_following_notes(self):
        changelog = CHANGELOG.replace("- Refuse unsafe destinations (#158).", "[x]: https://example.test\n- Refuse unsafe destinations (#158).")
        self.check(BODY + "\n" + FOOTER, changelog=changelog)
        with self.assertRaisesRegex(ValueError, "differ"):
            self.check(BODY.replace("- Refuse unsafe destinations (#158).\n", "") + "\n" + FOOTER, changelog=changelog)

    def test_literal_link_syntax_is_unchanged_inside_code(self):
        for source in ["`[t](a.md)`", "``[t](a.md)``", "```carve\n[t](a.md)\n```", "~~~\n[t](a.md)\n~~~"]:
            self.assertEqual(notes.release_links(source, "markup-carve/carve-wasm", "v0.1.5"), source)

    def test_root_relative_links_resolve_at_the_tag(self):
        self.assertEqual(notes.release_links("[x](/docs/x.md)", "markup-carve/carve-wasm", "v0.1.5"),
                         "[x](https://github.com/markup-carve/carve-wasm/blob/v0.1.5/docs/x.md)")

    def test_footnote_definitions_are_retained(self):
        changelog = "## [0.1.0]\n- Initial release[^1].\n\n[^1]: A note.\n"
        self.check("- Initial release[^1].\n\n[^1]: A note.\n**Full Changelog**: https://github.com/markup-carve/carve-wasm/releases/tag/v0.1.0",
                   changelog=changelog, tag="v0.1.0")

    def test_piped_json_is_utf8_even_with_a_windows_locale(self):
        with tempfile.TemporaryDirectory() as directory:
            changelog = Path(directory, "CHANGELOG.md")
            changelog.write_text("## [0.1.0]\n\n- Section §4.2 … corrected.\n", encoding="utf-8")
            body = "- Section §4.2 … corrected.\n**Full Changelog**: https://github.com/markup-carve/carve-wasm/releases/tag/v0.1.0"
            result = subprocess.run([sys.executable, str(Path(__file__).with_name("check-release-notes.py")),
                                     "--tag", "v0.1.0", "--repo", "markup-carve/carve-wasm", "--changelog", str(changelog)],
                                    input=json.dumps({"tag_name": "v0.1.0", "body": body}, ensure_ascii=False).encode("utf-8"),
                                    capture_output=True, env={**os.environ, "PYTHONIOENCODING": "cp1252"})
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_present_but_wrong_notes_fail(self):
        with self.assertRaisesRegex(ValueError, "differ"):
            self.check("The engine does not move. Rendering is unchanged.\n" + FOOTER)

    def test_missing_entry_same_heading_fails(self):
        with self.assertRaisesRegex(ValueError, "differ"):
            self.check(BODY.replace("- Refuse unsafe destinations (#158).\n", "") + "\n" + FOOTER)

    def test_reworded_entry_with_same_pr_numbers_fails(self):
        with self.assertRaisesRegex(ValueError, "differ"):
            self.check(BODY.replace("Move the engine to 0.1.7", "Keep the engine unchanged") + "\n" + FOOTER)

    def test_changed_heading_fails(self):
        with self.assertRaisesRegex(ValueError, "differ"):
            self.check(BODY.replace("### Changed", "### Breaking") + "\n" + FOOTER)

    def test_wrong_repo_or_version_footer_fails(self):
        for footer in [FOOTER.replace("carve-wasm", "carve-py"), FOOTER.replace("...v0.1.5", "...v0.1.4")]:
            with self.assertRaisesRegex(ValueError, "footer"):
                self.check(BODY + "\n" + footer)

    def test_missing_release_or_empty_body_fails(self):
        for release in [None, {"tag_name": "v0.1.4", "body": SECTION}, {"tag_name": "v0.1.5", "body": " "}]:
            with self.assertRaises(ValueError):
                notes.check_release(CHANGELOG, release, "markup-carve/carve-wasm", "v0.1.5")

    def test_missing_or_duplicate_version_section_fails(self):
        for changelog in [CHANGELOG.replace("[0.1.5]", "[0.1.6]"), CHANGELOG + "\n## [0.1.5]\n- Duplicate.\n"]:
            with self.assertRaisesRegex(ValueError, "exactly one"):
                self.check(BODY + "\n" + FOOTER, changelog=changelog)

    def test_unprefixed_tag(self):
        self.check(BODY.replace("/v0.1.5/", "/0.1.5/") + "\n" + FOOTER.replace("v0.1", "0.1"), tag="0.1.5")

    def test_first_release_excludes_reference_definitions(self):
        first = "## [0.1.0]\n\n- Initial release.\n\n[0.1.0]: https://example.test/tag\n"
        self.check("- Initial release.\n**Full Changelog**: https://github.com/markup-carve/carve-wasm/releases/tag/v0.1.0", changelog=first, tag="v0.1.0")


if __name__ == "__main__":
    unittest.main()
