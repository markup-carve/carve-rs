//! markup-carve/carve-rs#2456: the Djot importer escaped two markers it must
//! not. A table caption `^ text` is the same marker Carve spells a caption
//! with, so `\^` dropped the caption and left a paragraph; and the `%%%%` this
//! importer GENERATES for an empty footnote body was escaped into literal text,
//! so the render carried a `%%%%` paragraph no source ever held.
//!
//! Carve has no empty footnote body - `[^b]:` alone re-parses as a paragraph -
//! so the comment placeholder is what makes the definition exist at all. It
//! therefore travels as a NUL token the text escaper cannot see.
//!
//! djot.js 0.3.2 folds an UNINDENTED caption continuation into the caption
//! (measured; its own `test/tables.test` snapshot pins it), although
//! `doc/syntax.md` says a continuation must be indented. Carve's parser folds
//! it too, so the importer carries the lines through either way.

fn imported(src: &str) -> String {
    carve::djot_to_carve(src)
}

fn html(src: &str) -> String {
    carve::to_html(&carve::djot_to_carve(src))
}

#[test]
fn a_caption_after_a_blank_line_keeps_its_marker() {
    assert_eq!(
        imported("| a | b |\n\n^ With a _caption_\nand another line.\n"),
        "| a | b |\n\n^ With a /caption/\nand another line.\n"
    );
}

#[test]
fn a_caption_directly_under_the_table_keeps_its_marker() {
    assert_eq!(imported("| a | b |\n^ cap\n"), "| a | b |\n^ cap\n");
}

#[test]
fn an_indented_continuation_keeps_its_indent() {
    assert_eq!(
        imported("| a | b |\n\n^ With a _caption_\n  and another line.\n"),
        "| a | b |\n\n^ With a /caption/\n  and another line.\n"
    );
}

#[test]
fn a_tab_after_the_marker_takes_the_space_carve_requires() {
    assert_eq!(imported("| a | b |\n\n^\tcap\n"), "| a | b |\n\n^ cap\n");
}

#[test]
fn a_caption_inside_a_quote_keeps_its_marker() {
    assert_eq!(
        imported("> | a | b |\n>\n> ^ cap\n"),
        "> | a | b |\n>\n> ^ cap\n"
    );
}

#[test]
fn an_empty_footnote_body_becomes_a_comment() {
    assert_eq!(
        imported("[^a]\n[^b]\n\n[^b]:\n"),
        "[^carve-djot-note-0]: %%%%\n\n[^carve-djot-note-0]\n[^b]\n\n[^b]: %%%%\n"
    );
}

#[test]
fn the_caption_reaches_the_rendered_table() {
    assert!(html("| a | b |\n\n^ With a _caption_\nand another line.\n")
        .contains("<caption>With a <em>caption</em>\nand another line.</caption>"));
}

#[test]
fn an_empty_footnote_body_renders_nothing_but_its_backlink() {
    let out = html("[^a]\n[^b]\n\n[^b]:\n");
    assert!(!out.contains("%%%%"), "{out}");
    assert!(out.contains(
        "<li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>"
    ), "{out}");
}

// CONTROLS. Each needs its escape, and a fix that stopped escaping would
// satisfy the cases above while breaking one of these.

#[test]
fn a_caret_paragraph_with_no_table_above_it_still_escapes() {
    assert_eq!(imported("para\n\n^ cap\n"), "para\n\n\\^ cap\n");
}

#[test]
fn only_the_first_block_after_a_table_may_be_its_caption() {
    assert_eq!(
        imported("| a | b |\n\n^ cap\n\n^ not a caption\n"),
        "| a | b |\n\n^ cap\n\n\\^ not a caption\n"
    );
}

#[test]
fn a_paragraph_after_a_table_without_a_caret_is_untouched() {
    assert_eq!(
        imported("| a | b |\n\nplain paragraph\n"),
        "| a | b |\n\nplain paragraph\n"
    );
}

#[test]
fn a_caret_with_no_space_after_it_is_not_a_caption_marker() {
    assert_eq!(imported("| a | b |\n\n^cap\n"), "| a | b |\n\n\\^cap\n");
}

#[test]
fn an_authored_percent_run_in_a_footnote_body_still_escapes() {
    assert_eq!(
        imported("[^a]: %%%%\n\ntext[^a]\n"),
        "[^a]: \\%%%%\n\ntext[^a]\n"
    );
    assert!(html("[^a]: %%%%\n\ntext[^a]\n").contains("%%%%"));
}

#[test]
fn a_non_empty_footnote_body_is_untouched() {
    assert_eq!(imported("t[^a]\n\n[^a]: body\n"), "t[^a]\n\n[^a]: body\n");
}

#[test]
fn a_caret_in_running_text_still_escapes() {
    assert_eq!(imported("a ^ b and ^x^ c\n"), "a \\^ b and {^x^} c\n");
}

// The Carve parser side of both markers, which this fix must not move.

#[test]
fn an_authored_carve_caption_still_opens_one() {
    assert!(carve::to_html("| a | b |\n\n^ cap\n").contains("<caption>cap</caption>"));
}

#[test]
fn an_authored_percent_run_is_still_a_comment() {
    assert_eq!(carve::to_html("a %%note here\nb\n"), "<p>a\nb</p>");
}
