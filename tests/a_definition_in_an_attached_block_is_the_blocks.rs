//! A DEFINITION AT A `+`-ATTACHED BLOCK'S OWN COLUMN 0 IS THE BLOCK'S
//! (markup-carve/carve#1918 row 24, markup-carve/carve-rs#1532).
//!
//! `+` attaches ONE flush-left block, and §10 I5 spends the lazy fold first, so
//! a definition the block REGISTERS was never a boundary - the extent runs past
//! it to the content it introduces, however many definitions there are. This
//! engine ended the item on the first of them and published what followed as a
//! top-level paragraph.
//!
//! The definitions are consumed BEFORE the sibling-marker test, so that test
//! reads the line they introduce. Leaving them for the caller ends the list on
//! them instead - they are document-column definitions, which are I5
//! interrupters there.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at carve
//! `2f654da9`, spec main, which is also corpus section
//! `447-the-host-does-not-change-which-column-a-definition-reaches`.

use carve::{to_html, to_html_with_options, Options};

/// The #908 guard: the facade and the position-tracking path must agree.
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

fn assert_html(src: &str, expected: &str) {
    let normalize = |html: &str| {
        html.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(" <", "<")
    };
    assert_eq!(
        normalize(&both_paths(src)),
        normalize(expected),
        "on {src:?}"
    );
}

/// Corpus row 24. Two definitions of ONE label: both are the block's, the
/// extent reaches `more`, and PART 9R's LAST definition wins picks `/b`.
#[test]
fn the_reported_document_keeps_the_extent() {
    assert_html(
        "- a\n+\n[r]: /a\n[r]: /b\nmore\n\nSee [r][].\n",
        "<ul><li>a more</li></ul><p>See <a href=\"/b\">r</a>.</p>",
    );
}

/// ONE definition goes the same way - the count is not what decides it.
#[test]
fn one_definition_keeps_the_extent() {
    assert_html(
        "- a\n+\n[r]: /a\nmore\n\nSee [r][].\n",
        "<ul><li>a more</li></ul><p>See <a href=\"/a\">r</a>.</p>",
    );
}

/// A FOOTNOTE definition is a definition here too.
#[test]
fn a_footnote_definition_keeps_the_extent() {
    assert_html(
        "- a\n+\n[^n]: note\nmore\n\nSee [^n].\n",
        "<ul><li>a more</li></ul>\
         <p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>.</p>\
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\">\
         <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\
         </li></ol></section>",
    );
}

/// WHAT THE DEFINITIONS INTRODUCE IS STILL READ. A sibling marker below them is
/// the outer list's next item, not the attached block's content - the reason the
/// definitions are consumed before that test rather than after it.
#[test]
fn a_sibling_marker_below_them_is_still_a_sibling() {
    assert_html(
        "- a\n+\n[r]: /a\n- z\n\nSee [r][].\n",
        "<ul><li>a</li><li>z</li></ul><p>See <a href=\"/a\">r</a>.</p>",
    );
}

/// DEFINITIONS ALONE attach nothing, and the item ends with them.
#[test]
fn definitions_alone_attach_nothing() {
    assert_html(
        "- a\n+\n[r]: /a\n\nSee [r][].\n",
        "<ul><li>a</li></ul><p>See <a href=\"/a\">r</a>.</p>",
    );
}

/// A `+` BLOCK WITH NO DEFINITION is the control the change must not touch.
#[test]
fn a_plain_attached_block_is_unchanged() {
    assert_html("- a\n+\nmore\n", "<ul><li>a more</li></ul>");
}

/// AT A NONZERO COLUMN the definition is not the block's own column 0, and the
/// item ends above `more` as it did before.
#[test]
fn a_definition_at_a_nonzero_column_is_not_this_band() {
    assert_html(
        "- a\n+\n  [r]: /a\nmore\n\nSee [r][].\n",
        "<ul><li>a</li></ul><p>more</p><p>See <a href=\"/a\">r</a>.</p>",
    );
}

/// WITHOUT THE `+` the definition is an I5 interrupter at the document column
/// and ends the item - unchanged, and what says this band is the `+` block's.
#[test]
fn without_the_marker_the_definition_still_ends_the_item() {
    assert_html(
        "- a\n[r]: /a\nmore\n\nSee [r][].\n",
        "<ul><li>a</li></ul><p>more</p><p>See <a href=\"/a\">r</a>.</p>",
    );
}
