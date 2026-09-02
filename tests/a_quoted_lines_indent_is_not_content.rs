//! A QUOTED LINE'S OWN INDENT IS NOT CONTENT (markup-carve/carve-rs#1511).
//!
//! `> x` over `>  plain text` is one paragraph reading `x` / `plain text`. The
//! second line is written past the quote's content column, its indentation is
//! not significant there, and the executable spec (`tests/spec` at carve
//! `86569bd`) drops it.
//!
//! THE DEFECT WAS A PATH DIVERGENCE, not only a conformance one. `carve::to_html`
//! kept the residual column and the CLI did not, because the CLI turns position
//! tracking on for every render and the borrowed layout facade behind `to_html`
//! is skipped whenever any option is set. So the same document parsed two ways
//! depending on a flag that is only supposed to add spans - the shape
//! carve-rs#908 cost once already.
//!
//! EVERY TEST HERE RUNS THROUGH `carve::to_html`. A CLI-driven test cannot see
//! this bug at all, and neither can one that passes any `Options`.
//!
//! MEASURED, NOT ASSUMED. The carve-rs#1509 sweep, re-derived: 306 prefix/column
//! pairs over the l(ist)/q(uote) container prefixes to depth four, the line
//! written at every column from just past the last quote marker out to 14,
//! sixteen line kinds each - 4896 documents - rendered through `carve::to_html`
//! and through the executable spec at the pinned corpus. Before: the paragraph,
//! table and definition-list kinds each disagreed on the same 12 pairs, prefix
//! `q` at every column from 3 to 14. After: 0 for all three, every other kind
//! unmoved, and 36 of the 4896 documents changed answer - exactly the 36 that
//! were wrong.

use carve::{to_html, to_html_with_options, Options};

/// Rendering the same source with an option set takes the authoritative
/// pipeline instead of the layout facade. The two must agree on every document;
/// where they do not, `carve::to_html` is answering a question the CLI answers
/// differently.
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

#[test]
fn the_reported_document_renders_what_the_spec_renders() {
    assert_eq!(
        both_paths("> x\n>  plain text\n").trim(),
        "<blockquote><p>x\nplain text</p></blockquote>",
    );
}

#[test]
fn the_ticket_s_table_rows_lose_their_column_too() {
    // The same defect, in the spelling that shows it is more than cosmetic:
    // the leading space is what a reader has to strip to see the row at all.
    // Trimmed, each line is a table row, and a row is not a shape the facade
    // answers - so this document leaves it and the authoritative pipeline
    // renders the spec's paragraph.
    assert_eq!(
        both_paths("> x\n>  | A |\n>  | - |\n>  | b |\n").trim(),
        "<blockquote><p>x\n| A |\n| - |\n| b |</p></blockquote>",
    );
}

#[test]
fn a_definition_list_written_past_the_column_folds_the_same_way() {
    assert_eq!(
        both_paths("> x\n>  t\n>  : d\n").trim(),
        "<blockquote><p>x\nt\n: d</p></blockquote>",
    );
}

#[test]
fn every_column_past_the_marker_answers_as_the_marker_s_own_column_does() {
    // The twelve pairs the sweep found for each kind: prefix `q`, columns 3
    // through 14. Column 2 is the quote's own content column and was always
    // right; every column past it must render identically to it.
    let at_column_two = both_paths("> x\n> plain text\n");
    assert_eq!(
        at_column_two.trim(),
        "<blockquote><p>x\nplain text</p></blockquote>",
    );
    for column in 3..=14usize {
        let src = format!("> x\n>{}plain text\n", " ".repeat(column - 1));
        let html = both_paths(&src);
        assert_eq!(html, at_column_two, "column {column}: {src:?}");
    }
}

#[test]
fn the_quote_s_first_line_may_carry_the_column_too() {
    // Nothing here is about a CONTINUATION line. A quote whose every line is
    // written past the marker holds the same paragraph as one written at it.
    for src in [">  a\n>  b\n", ">   a\n>   b\n"] {
        assert_eq!(
            both_paths(src).trim(),
            "<blockquote><p>a\nb</p></blockquote>",
            "{src:?}"
        );
    }
}

#[test]
fn a_line_at_the_content_column_is_untouched() {
    // THE CONTROLS. A fix that trimmed more than the residual column would
    // take these with it, and each is the executable spec's own output.
    for (src, expected) in [
        ("> a\n> b\n", "<blockquote><p>a\nb</p></blockquote>"),
        ("> a\n", "<blockquote><p>a</p></blockquote>"),
        ("> | A |\n> | - |\n> | b |\n", "<td>b</td>"),
        ("> # H\n", "<h1 id=\"H\">H</h1>"),
    ] {
        let html = both_paths(src);
        assert!(html.contains(expected), "{src:?}: {html}");
    }
}

#[test]
fn an_escaped_leading_space_is_still_content() {
    // A NON-BREAKING SPACE IS NOT INDENTATION. Only ASCII whitespace is the
    // column the container took; a character that renders as a space is text
    // and survives the trim, as it does in the authoritative pipeline.
    let html = both_paths("> x\n> \u{a0}plain text\n");
    assert!(html.contains("&nbsp;plain text"), "{html}");
}
