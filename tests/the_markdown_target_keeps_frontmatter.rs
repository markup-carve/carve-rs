//! PART 11 §10r (markup-carve/carve#2501): the Markdown target emits
//! frontmatter first, with the format token wherever the format is not `yaml`,
//! and the content verbatim. HTML, plain text and the terminal keep omitting it.
//!
//! The importer already preserved frontmatter, so dropping it here made
//! `migrate_markdown` in and `to_markdown` out lossy in the one direction where
//! both ends spell the construct natively.

use carve::{to_ansi, to_html, to_markdown, to_plain_text};

#[test]
fn frontmatter_is_emitted_first_with_a_bare_opener_for_yaml() {
    assert_eq!(
        to_markdown("---\ntitle: Hi\n---\n\n# H\n\ntext\n"),
        "---\ntitle: Hi\n---\n\n# H\n\ntext\n"
    );
}

#[test]
fn a_format_that_is_not_yaml_keeps_its_token() {
    assert_eq!(
        to_markdown("---toml\nt = 1\n---\n\nx\n"),
        "---toml\nt = 1\n---\n\nx\n"
    );
}

#[test]
fn a_spaced_yaml_opener_is_spelled_bare() {
    assert_eq!(
        to_markdown("--- yaml\na: 1\n---\n\nx\n"),
        "---\na: 1\n---\n\nx\n"
    );
}

#[test]
fn a_document_that_is_only_frontmatter_needs_no_blank_line_after_it() {
    assert_eq!(to_markdown("---\na: 1\n---\n"), "---\na: 1\n---\n");
}

#[test]
fn the_content_is_verbatim_blank_lines_and_metacharacters_included() {
    let source = "---\nlist:\n\n\n  - \"*not emphasis*\"\n---\n\nx\n";
    assert_eq!(to_markdown(source), source);
}

#[test]
fn the_emitted_block_is_a_fixed_point_which_is_what_the_round_trip_buys() {
    let once = to_markdown("---toml\nt = 1\n---\n\nx\n");
    assert_eq!(to_markdown(&once), once);
}

#[test]
fn a_trojan_source_control_goes_like_every_other_byte() {
    assert_eq!(
        to_markdown("---\na: \u{202e}b\n---\n\nx\n"),
        "---\na: b\n---\n\nx\n"
    );
}

#[test]
fn the_other_three_targets_still_omit_it() {
    let source = "---\ntitle: Hi\n---\n\ntext\n";
    assert!(!to_html(source).contains("title"));
    assert_eq!(to_plain_text(source), "text\n");
    assert_eq!(to_ansi(source), "text\n");
}
