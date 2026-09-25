//! `CARVE-P9-073`: a `::: footnotes` marker places the endnotes section only at
//! document top level (carve-rs#1894).
//!
//! Inside a block-level container the marker does not place. It renders §12's
//! `<div class="{kind}">` floor where it is written, and the section is appended
//! where an unmarked document puts it. Before this, carve-rs wrote its marker
//! from the directive renderer at every depth, so `role="doc-endnotes"` was
//! announced nested inside a quotation and the document's own notes read as
//! part of what was quoted.
//!
//! The container list is the clause's own: a block quote, a list item, a div or
//! directive body, a table cell, a definition description, a footnote
//! definition.

use carve::{to_html, to_html_with_options, Options};

/// The paragraph every case below starts from, and the section every case ends
/// with. Taken from corpus row
/// `497-a-footnotes-placement-marker-inside-a-container-does-not-place`.
const INTRO: &str =
    "<p>Intro<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>.</p>\n";
const SECTION: &str = "<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>only note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";

fn document(body: &str) -> String {
    format!("Intro[^a].\n\n{body}\n[^a]: only note\n")
}

#[test]
fn a_marker_inside_a_block_quote_renders_the_floor_and_the_section_lands_last() {
    // The corpus row, byte for byte. Its `.crv` and `.html` are the fixture
    // this engine is measured against, so the whole document is pinned rather
    // than a substring of it.
    assert_eq!(
        to_html(&document("> ::: footnotes\n> :::\n\n")),
        format!("{INTRO}<blockquote>\n  <div class=\"footnotes\">\n\n  </div>\n</blockquote>\n{SECTION}")
    );
}

#[test]
fn a_marker_inside_a_list_item_renders_the_floor_and_the_section_lands_last() {
    assert_eq!(
        to_html(&document("- ::: footnotes\n  :::\n\n")),
        format!(
            "{INTRO}<ul>\n  <li>\n    <div class=\"footnotes\">\n\n    </div>\n  </li>\n</ul>\n{SECTION}"
        )
    );
}

#[test]
fn a_marker_inside_a_definition_description_renders_the_floor() {
    assert_eq!(
        to_html(&document(":: term\n: ::: footnotes\n  :::\n\n")),
        format!(
            "{INTRO}<dl>\n  <dt>term</dt>\n  <dd>\n    <div class=\"footnotes\">\n\n    </div>\n  </dd>\n</dl>\n{SECTION}"
        )
    );
}

#[test]
fn a_marker_inside_an_admonition_body_renders_the_floor() {
    assert_eq!(
        to_html(&document(":::: note\n::: footnotes\n:::\n::::\n\n")),
        format!(
            "{INTRO}<aside class=\"admonition note\" aria-label=\"Note\">\n  <div class=\"footnotes\">\n\n  </div>\n</aside>\n{SECTION}"
        )
    );
}

#[test]
fn a_marker_inside_another_directives_body_renders_the_floor() {
    // A directive body is a container too, and the outer directive here is
    // itself a placement marker - so neither of the two places.
    assert_eq!(
        to_html(&document(":::: toc\n::: footnotes\n:::\n::::\n\n")),
        format!(
            "{INTRO}<div class=\"toc\">\n  <div class=\"footnotes\">\n\n  </div>\n</div>\n{SECTION}"
        )
    );
}

#[test]
fn a_marker_inside_a_footnote_definition_leaves_exactly_one_section() {
    // The clause's own nuance: the floor renders inside the very section the
    // marker failed to move, so the section still appears exactly once. This
    // engine already declined to place here, through a separate guard; the
    // assertion is that the outcome did not change.
    let html = to_html("X[^a].\n\n[^a]: ::: footnotes\n    :::\n");
    assert_eq!(html.matches("doc-endnotes").count(), 1, "{html:?}");
    assert!(html.contains("<div class=\"footnotes\">"), "{html:?}");
    let section = html
        .split_once("<section role=\"doc-endnotes\"")
        .expect("an endnotes section")
        .1;
    assert!(
        section.contains("<div class=\"footnotes\">"),
        "the floor left the section it could not move: {html:?}"
    );
}

#[test]
fn a_table_cell_holds_no_container_so_the_marker_stays_text() {
    // Core table cells are inline-only, so the clause's table-cell position is
    // unreachable from source here: the marker never becomes a directive at
    // all. Pinned so a later cell-block feature is noticed rather than assumed.
    let html = to_html(&document("| ::: footnotes |\n| :::           |\n\n"));
    assert!(html.contains("<td>::: footnotes</td>"), "{html:?}");
    assert!(!html.contains("<div class=\"footnotes\">"), "{html:?}");
    assert!(html.ends_with(SECTION), "{html:?}");
}

// ---- controls: a top-level marker is unchanged ----

#[test]
fn a_top_level_marker_still_places() {
    // The control the fix has to leave alone. The section stands where the
    // marker was written, ahead of the heading that follows it.
    let html = to_html("X[^a].\n\n::: footnotes\n:::\n\n## After\n\n[^a]: a\n");
    let notes = html.find("doc-endnotes").expect("endnotes");
    let after = html.find(">After<").expect("the trailing heading");
    assert!(notes < after, "the section did not place: {html:?}");
    assert_eq!(html.matches("doc-endnotes").count(), 1, "{html:?}");
    assert!(!html.contains("<div class=\"footnotes\">"), "{html:?}");
}

#[test]
fn a_section_wrapper_is_not_a_container() {
    // Heading `<section>` nesting is a rendering artifact, not AST
    // containment, so a marker written under a heading is still at document
    // top level. Deriving the test from the indent `level` would have failed
    // this one, and `sections` is on by default - so every real document with
    // a heading would have stopped placing.
    for options in [Options::new(), Options::new().with_sections(true)] {
        let html = to_html_with_options(
            "# H\n\nX[^a].\n\n::: footnotes\n:::\n\n## After\n\n[^a]: a\n",
            &options,
        );
        assert!(
            html.find("doc-endnotes").expect("endnotes")
                < html.find(">After<").expect("the trailing heading"),
            "the section did not place under a heading: {html:?}"
        );
        assert!(!html.contains("<div class=\"footnotes\">"), "{html:?}");
    }
}

#[test]
fn an_unmarked_document_is_where_a_contained_marker_puts_the_section() {
    // What "appended where an unmarked document puts it" means, stated as the
    // comparison: strike the marker from the source and the section does not
    // move. Without this, a fix that dropped the section entirely would pass
    // every assertion above that only counts `doc-endnotes` once.
    let unmarked = to_html(&document(""));
    for body in [
        "> ::: footnotes\n> :::\n\n",
        "- ::: footnotes\n  :::\n\n",
        ":::: note\n::: footnotes\n:::\n::::\n\n",
    ] {
        let contained = to_html(&document(body));
        assert!(contained.ends_with(SECTION), "{body:?}: {contained:?}");
        assert!(unmarked.ends_with(SECTION), "{unmarked:?}");
        assert_eq!(contained.matches("doc-endnotes").count(), 1, "{body:?}");
    }
}
