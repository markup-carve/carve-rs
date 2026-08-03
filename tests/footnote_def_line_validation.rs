//! A footnote-definition line that is not one must stay on the page.
//!
//! `parse_link_def_line` carries a long comment about validating the WHOLE
//! reference-label production, because `[]: u` was being consumed as a
//! definition and rendering nothing - so the line DISAPPEARED (carve-rs#451),
//! and a first fix that rejected only the empty case left `[]]: u` and
//! `[a]b]: u` still vanishing (carve-rs#456).
//!
//! `parse_footnote_def_line` sits directly above it and got neither fix. Both
//! defects are reproducible there:
//!
//!   [^a]:        label, separator, NO content   -> deleted
//!   [^a]b]: x    a `]` inside the label         -> deleted
//!
//! carve-js and carve-php keep both as paragraphs.
//!
//! An empty label WITH content (`[^]: x`) is consumed by all three engines and
//! is deliberately left alone here - that is a three-way agreement, not a
//! divergence.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_definition_with_no_content_is_a_paragraph() {
    assert_eq!(squash(&to_html("[^a]: \n")), "<p>[^a]:</p>");
    assert_eq!(squash(&to_html("[^]: \n")), "<p>[^]:</p>");
}

#[test]
fn a_label_containing_a_bracket_is_a_paragraph() {
    assert_eq!(squash(&to_html("[^a]b]: x\n")), "<p>[^a]b]: x</p>");
    assert_eq!(squash(&to_html("[^]]: x\n")), "<p>[^]]: x</p>");
}

#[test]
fn nothing_is_silently_dropped() {
    // The sharp end: these rendered NOTHING AT ALL, so the line vanished from
    // the document rather than being misparsed into something visible.
    for src in ["[^a]: \n", "[^]: \n", "[^a]b]: x\n", "[^]]: x\n"] {
        assert!(
            !to_html(src).trim().is_empty(),
            "{src:?} rendered nothing at all"
        );
    }
}

#[test]
fn a_real_footnote_definition_still_works() {
    let html = to_html("See [^a].\n\n[^a]: note\n");

    assert!(html.contains("doc-noteref"), "{html}");
    assert!(html.contains("note"), "{html}");
}

#[test]
fn an_empty_label_with_content_is_unchanged() {
    // All three engines consume this; not this fix's business to change.
    assert_eq!(to_html("[^]: x\n").trim(), "");
}
