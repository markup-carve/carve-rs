//! The one authoring hazard around `::: >` that no other diagnostic reaches.
//!
//! Written on the wrong side of the marker a fence is left unclosed, and the
//! unclosed-container diagnostic reports it. Written at the quote's own column
//! nothing is malformed at all: the fence is a block opener, so it ends the
//! quote above and starts a SIBLING one, and the two adjacent blockquotes read
//! exactly like the nesting the author was reaching for
//! (markup-carve/carve#1718). Rule id and message mirror carve-js `lint.ts`,
//! the parity reference.

fn reports(source: &str) -> Vec<carve::LintWarning> {
    carve::lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "quote-fence-ends-the-quote-above")
        .collect()
}

#[test]
fn the_opener_is_reported_and_the_render_shows_why() {
    let source = "> a\n::: >\nb\n:::\n";
    assert_eq!(
        carve::to_html(source).trim(),
        "<blockquote><p>a</p></blockquote>\n<blockquote><p>b</p></blockquote>"
    );

    let warnings = reports(source);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].line, 2);
    assert_eq!(warnings[0].column, 1);
    assert!(
        warnings[0].message.contains("opens a sibling one"),
        "{:?}",
        warnings[0].message
    );
    assert!(
        warnings[0].message.contains("\"> ::: >\""),
        "{:?}",
        warnings[0].message
    );
}

#[test]
fn it_is_reported_wherever_the_content_column_sits() {
    // A container body and a list item: the same mistake at two columns, which
    // is why the pass reads siblings rather than lines.
    assert_eq!(
        reports(":::: note\n> a\n::: >\nb\n:::\n::::\n").len(),
        1,
        "inside a container"
    );
    assert_eq!(
        reports("- > a\n  ::: >\n  b\n  :::\n").len(),
        1,
        "inside a list item"
    );
}

#[test]
fn the_nested_spelling_which_needs_the_marker_is_not_reported() {
    let source = "> ::: >\n> b\n> :::\n";
    assert_eq!(
        carve::to_html(source).trim(),
        "<blockquote>\n  <blockquote><p>b</p></blockquote>\n</blockquote>"
    );
    assert!(reports(source).is_empty());
}

#[test]
fn a_blank_line_makes_two_quotes_deliberate() {
    assert!(reports("> a\n\n::: >\nb\n:::\n").is_empty());
}

#[test]
fn a_fenced_quote_below_a_fenced_quote_is_not_reported() {
    // Both spellings are one node, but only the prefixed one leaves the author
    // no visible cue: after a closing fence the sibling is where it looks.
    assert!(reports("::: >\na\n:::\n::: >\nb\n:::\n").is_empty());
}

#[test]
fn a_lone_quote_in_either_spelling_is_not_reported() {
    assert!(reports("> a\n> b\n").is_empty());
    assert!(reports("::: >\nb\n:::\n").is_empty());
}
