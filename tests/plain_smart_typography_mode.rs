//! Smart typography is a presentation choice: right for a person reading the
//! output, usually wrong for a machine reading it. Source mode reproduces what
//! the author typed so a search for the source spelling finds it.
//!
//! Plain text is one of the four presentation renderers the spec names, and it
//! ignored the switch entirely until carve#560 - `--smart-typography source`
//! was accepted on this target and did nothing.

use carve::{Options, SmartTypographyMode};

fn source_mode(input: &str) -> String {
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };

    carve::to_plain_text_with_options(input, &options)
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
        assert_eq!(
            carve::to_plain_text(input).trim(),
            *glyphs,
            "input: {input}"
        );
    }
}

#[test]
fn asking_for_glyph_explicitly_matches_the_default() {
    let options = Options {
        smart_typography: SmartTypographyMode::Glyph,
        ..Options::default()
    };

    for (input, _) in CASES {
        assert_eq!(
            carve::to_plain_text_with_options(input, &options),
            carve::to_plain_text(input),
            "input: {input}"
        );
    }
}

#[test]
fn escaping_is_left_alone() {
    // Escaping is a separate concern with its own rationale. CONTROL: no arm of
    // this change touches it, so this is a regression guard rather than a check
    // on the new behavior.
    assert_eq!(source_mode("a & b"), "a & b");
    assert_eq!(source_mode("\\\" and \\-\\-"), "\" and --");
}

#[test]
fn code_spans_are_left_alone() {
    // CONTROL, as above: code content never reached the smart-punctuation arm.
    assert_eq!(source_mode("`a...b`"), "a...b");
    assert_eq!(carve::to_plain_text("`a...b`").trim(), "a...b");
}

#[test]
fn structure_still_renders_around_the_switch() {
    let plain = source_mode("# Title...\n\n- one... item\n- two -> three\n");

    assert!(plain.contains("Title..."), "{plain}");
    assert!(plain.contains("one... item"), "{plain}");
    assert!(plain.contains("two -> three"), "{plain}");
}

#[test]
fn the_mode_does_not_survive_into_the_next_render() {
    // Each renderer carries its own thread-local cell, and no entry point
    // restores the previous value - so the guarantee that a Source render does
    // not colour the render after it rests on every entry point setting the
    // cell. Interleave the targets in both orders and check each answer.
    //
    // CONTROL as the tree stands: neither renderer re-enters another, so a
    // shared cell would not be observable here today. It fails the moment an
    // entry point stops setting the cell, which is the mutation that matters.
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };

    for _ in 0..2 {
        assert_eq!(
            carve::to_plain_text_with_options("a...b", &options).trim(),
            "a...b"
        );
        assert_eq!(carve::to_markdown("a...b").trim(), "a…b");
        assert_eq!(carve::to_plain_text("a...b").trim(), "a…b");
        assert_eq!(
            carve::to_markdown_with_options("a...b", &options).trim(),
            "a...b"
        );
        assert_eq!(carve::to_ansi("a...b").trim(), "a…b");
        assert_eq!(
            carve::to_plain_text_with_options("a...b", &options).trim(),
            "a...b"
        );
    }
}
