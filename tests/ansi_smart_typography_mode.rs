//! Smart typography is a presentation choice: right for a person reading the
//! output, usually wrong for a machine reading it. Source mode reproduces what
//! the author typed so a search for the source spelling finds it.
//!
//! The terminal target is one of the four presentation renderers the spec
//! names, and it ignored the switch entirely until carve#560 -
//! `render_ansi_with_options` took its options as `_options` and read only the
//! heading-id flag.

use carve::{Options, SmartTypographyMode};

/// Drop the styling runs so an assertion can talk about the text. The runs
/// themselves are asserted separately - the switch must not disturb them.
fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        for c in chars.by_ref() {
            if c == 'm' {
                break;
            }
        }
    }
    out
}

fn source_mode(input: &str) -> String {
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };

    strip_ansi(&carve::to_ansi_with_options(input, &options))
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
            strip_ansi(&carve::to_ansi(input)).trim(),
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
            carve::to_ansi_with_options(input, &options),
            carve::to_ansi(input),
            "input: {input}"
        );
    }
}

#[test]
fn the_styling_runs_are_untouched_by_the_switch() {
    // Source mode changes the punctuation and nothing else: the escape
    // sequences a terminal reads must come out identical in both modes.
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };
    let input = "# Head...\n\nA /slanted/ and `a--b` run... here.\n";

    let runs = |s: &str| -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = s;
        while let Some(i) = rest.find('\u{1b}') {
            rest = &rest[i..];
            let end = rest.find('m').expect("terminated escape") + 1;
            found.push(rest[..end].to_string());
            rest = &rest[end..];
        }
        found
    };

    let glyph = carve::to_ansi(input);
    let source = carve::to_ansi_with_options(input, &options);

    assert_eq!(runs(&glyph), runs(&source));
    assert!(glyph.contains("\u{1b}[1m"), "{glyph:?}");
    assert_ne!(glyph, source);
}

#[test]
fn the_heading_rule_follows_the_text_it_underlines() {
    // The rule is drawn to the width of the rendered heading, so source mode
    // has to widen it: "a...b" is five columns where "a…b" is three. Nothing
    // recomputes this per mode - it falls out of the rendered text - which is
    // exactly why a silent desync would go unnoticed without an assertion.
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };
    let rule_width = |s: &str| strip_ansi(s).chars().filter(|c| *c == '═').count();

    let glyph = carve::to_ansi("# a...b\n");
    let source = carve::to_ansi_with_options("# a...b\n", &options);

    assert_eq!(rule_width(&glyph), 3, "{glyph:?}");
    assert_eq!(rule_width(&source), 5, "{source:?}");
}

#[test]
fn escaping_is_left_alone() {
    // CONTROL: no arm of this change touches escapes, so this is a regression
    // guard rather than a check on the new behavior.
    assert_eq!(source_mode("a & b"), "a & b");
    assert_eq!(source_mode("\\\" and \\-\\-"), "\" and --");
}

#[test]
fn code_spans_are_left_alone() {
    // CONTROL, as above: code content never reached the smart-punctuation arm.
    assert_eq!(source_mode("`a...b`"), "a...b");
    assert_eq!(strip_ansi(&carve::to_ansi("`a...b`")).trim(), "a...b");
}

#[test]
fn the_mode_does_not_survive_into_the_next_render() {
    // See the twin test in `plain_smart_typography_mode.rs` for why this is a
    // CONTROL as the tree stands, and what it does catch.
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };

    for _ in 0..2 {
        assert_eq!(source_mode("a...b"), "a...b");
        assert_eq!(carve::to_markdown("a...b").trim(), "a…b");
        assert_eq!(strip_ansi(&carve::to_ansi("a...b")).trim(), "a…b");
        assert_eq!(
            carve::to_plain_text_with_options("a...b", &options).trim(),
            "a...b"
        );
        assert_eq!(source_mode("a...b"), "a...b");
    }
}
