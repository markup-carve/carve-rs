//! Smart typography is a presentation choice: right for a person reading the
//! output, usually wrong for a machine reading it. Source mode reproduces what
//! the author typed so a search for the source spelling finds it.

use carve::{Options, SmartTypographyMode};

fn source_mode(input: &str) -> String {
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };

    carve::to_markdown_with_options(input, &options)
        .trim()
        .to_string()
}

const CASES: &[(&str, &str)] = &[
    ("a...b", "a…b"),
    ("a--b", "a–b"),
    ("a---b", "a—b"),
    ("a -> b", "a → b"),
    ("a <= b", "a ≤ b"),
    ("(c) 2026", "© 2026"),
    ("say \"hi\"", "say “hi”"),
];

#[test]
fn source_mode_emits_what_the_author_typed() {
    for (input, _) in CASES {
        assert_eq!(source_mode(input), *input, "input: {input}");
    }
}

#[test]
fn glyph_mode_remains_the_default() {
    for (input, glyphs) in CASES {
        assert_eq!(carve::to_markdown(input).trim(), *glyphs, "input: {input}");
    }
}

#[test]
fn escaping_is_left_alone() {
    // Escaping is a separate concern with its own rationale.
    assert_eq!(source_mode("a & b"), "a &amp; b");
}

#[test]
fn code_spans_are_left_alone() {
    assert_eq!(source_mode("`a...b`"), "`a...b`");
}

#[test]
fn markdown_structure_still_renders() {
    let markdown = source_mode("# Title\n\nA *strong* claim... here.\n");

    assert!(markdown.contains("# Title"), "{markdown}");
    assert!(markdown.contains("**strong**"), "{markdown}");
    assert!(markdown.contains("claim... here"), "{markdown}");
}

#[test]
fn every_target_defaults_to_the_glyph() {
    // Not "other targets ignore the mode": HTML honours it too
    // (tests/html_smart_typography_mode.rs). What is pinned here is the
    // DEFAULT, which is the glyph everywhere.
    assert_eq!(carve::to_html("a...b").trim(), "<p>a…b</p>");
    assert_eq!(carve::to_plain_text("a...b").trim(), "a…b");
}
