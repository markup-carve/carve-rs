//! `Options::smart_typography` reaches the HTML target too.
//!
//! The option existed and the Markdown renderer honoured it; HTML read the
//! glyph unconditionally, so a host that set the option got a page that looked
//! configured and was not - the state carve#560 describes, and the state the
//! spec calls the only non-conformant one (a host may omit the switch, but not
//! accept it silently).

use carve::{Options, SmartTypographyMode};

fn source_mode(input: &str) -> String {
    let options = Options {
        smart_typography: SmartTypographyMode::Source,
        ..Options::default()
    };

    carve::to_html_with_options(input, &options)
        .trim()
        .to_string()
}

const CASES: &[(&str, &str, &str)] = &[
    ("a...b", "<p>a…b</p>", "<p>a...b</p>"),
    ("a--b", "<p>a–b</p>", "<p>a--b</p>"),
    ("a---b", "<p>a—b</p>", "<p>a---b</p>"),
    ("a -> b", "<p>a → b</p>", "<p>a -&gt; b</p>"),
    ("a <= b", "<p>a ≤ b</p>", "<p>a &lt;= b</p>"),
    ("(c) 2026", "<p>© 2026</p>", "<p>(c) 2026</p>"),
    ("say \"hi\"", "<p>say “hi”</p>", "<p>say \"hi\"</p>"),
];

#[test]
fn source_mode_emits_what_the_author_typed() {
    for (input, _, source) in CASES {
        assert_eq!(source_mode(input), *source, "input: {input}");
    }
}

#[test]
fn glyph_mode_remains_the_default() {
    for (input, glyphs, _) in CASES {
        assert_eq!(carve::to_html(input).trim(), *glyphs, "input: {input}");
    }
}

#[test]
fn the_escaping_rule_is_unchanged() {
    // Source mode changes WHICH text is emitted, never how it is escaped: the
    // arrow's `>` and the comparison's `<` still leave as entities above, and
    // an ampersand is untouched by the switch.
    assert_eq!(source_mode("a & b"), "<p>a &amp; b</p>");
}

#[test]
fn code_spans_are_left_alone() {
    assert_eq!(source_mode("`a...b`"), "<p><code>a...b</code></p>");
}

#[test]
fn heading_ids_do_not_depend_on_the_switch() {
    // The id pass slugs from the glyph text and normalizes it back to ASCII,
    // so a document's ids are the same in both modes. A switch that moved them
    // would break every link into the document.
    let source = "# Don't repeat yourself\n";
    let glyph = carve::to_html(source);
    let plain = source_mode(source);

    assert!(glyph.contains("id=\"Don-t-repeat-yourself\""), "{glyph}");
    assert!(plain.contains("id=\"Don-t-repeat-yourself\""), "{plain}");
}
