//! `^` WITH NOTHING AFTER IT IS NOT A CAPTION LINE.
//!
//! It re-parses as a paragraph holding a literal caret, so a block carrying an
//! empty caption came back saying something the tree never said. That is an
//! ADDITION rather than a loss, which is why it is fixed here rather than
//! declared in a report (markup-carve/carve-rs#1405, ruling
//! markup-carve/carve-js#1423).
//!
//! THE TICKET NAMED ONE SLOT AND THIS ENGINE HAD TWO. The ticket's premise is
//! that carve-rs already applied the rule to every FIGURE host and that only a
//! table's own caption - written as the last ROW of the table rather than as a
//! `^ ` line under a block - kept the caret. Measured on `e04bad8`, that
//! premise does not hold here: an empty figcaption never reaches the writer
//! from an HTML import, because the importer declines to build a figure for
//! one, but a `Figure` carrying an empty caption from an AST INGEST wrote the
//! bare `^` exactly as the table did. Both slots are closed, through one
//! predicate.
//!
//! THE ASSERTIONS ARE ON THE RE-RENDER. A test pinning emitted bytes would pass
//! a fix that swapped one wrong spelling for another; what is claimed is that
//! no caret reaches the rendered document and the block is otherwise untouched.

use carve::{
    from_json, html_to_carve, render_carve, to_carve, to_html, to_json, HtmlImportMode,
    HtmlImportOptions,
};

fn imported(html: &str, mode: HtmlImportMode) -> String {
    html_to_carve(
        html,
        &HtmlImportOptions {
            mode,
            ..Default::default()
        },
    )
    .expect("import")
    .value
}

/// The Carve a document carrying `caption` on its first block writes, reached
/// through the AST rather than through any importer: this is the second door
/// the ticket names, and a fix guarding only the HTML path leaves it open.
fn written_with_caption(source: &str, caption: &str) -> String {
    let json = to_json(&carve::parse(source));
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("ast json");
    value["children"][0]["caption"] = serde_json::from_str(caption).expect("caption json");
    let document = from_json(&value.to_string()).expect("decode");
    render_carve(&document).expect("write")
}

/// THE CANARY. The cheapest assertion in this file that cannot hold unless the
/// binary linked the source shipped beside it, so a stale-artifact run fails
/// here first and names itself instead of looking like a behavior bug.
#[test]
fn the_guard_is_present_in_the_binary_under_test() {
    assert_eq!(
        imported(
            "<table><caption></caption><tr><td>a</td></tr></table>",
            HtmlImportMode::Semantic
        ),
        "| a |\n",
        "a stale artifact: this binary's writer predates the empty-caption guard"
    );
}

/// THE TICKET'S OWN REPRO, asserted on what the document RENDERS.
#[test]
fn an_empty_table_caption_puts_no_caret_in_the_rendered_document() {
    for html in [
        "<table><caption></caption><tr><td>a</td></tr></table>",
        "<table><caption> </caption><tr><td>a</td></tr></table>",
        "<table><caption>\n\t </caption><tr><td>a</td></tr></table>",
    ] {
        for mode in [
            HtmlImportMode::Safe,
            HtmlImportMode::Semantic,
            HtmlImportMode::Roundtrip,
        ] {
            let written = imported(html, mode);
            let rendered = to_html(&written);
            assert!(
                !rendered.contains('^'),
                "a caret reached the rendered document for {html}: {rendered}"
            );
            assert!(
                !rendered.contains("<caption>"),
                "an empty caption became an element for {html}: {rendered}"
            );
            // AND THE TABLE IS OTHERWISE UNTOUCHED, which is the other half of
            // the claim: the guard removes a line, not a row.
            assert!(
                rendered.contains("<td>a</td>"),
                "the table lost its cell for {html}: {rendered}"
            );
        }
    }
}

/// THE SECOND DOOR. A tree carrying an empty caption reaches the same writer
/// without passing the importer at all.
#[test]
fn an_empty_caption_from_an_ast_ingest_writes_no_line() {
    for caption in ["[]", r#"[{"type":"text","value":" "}]"#] {
        let written = written_with_caption("| a |\n", caption);
        assert_eq!(written, "| a |\n", "caption {caption}");
        assert!(!to_html(&written).contains('^'));
    }
}

/// THE OTHER SLOT, which the ticket's premise said was already closed here and
/// was not: a `Figure` carrying an empty caption wrote the bare `^` too, and
/// the caret came back as literal text inside the target's own paragraph.
#[test]
fn an_empty_figure_caption_writes_no_line_either() {
    let written = written_with_caption("![A](a.png)\n^ Cap\n", "[]");
    assert_eq!(written, "![A](a.png)\n");
    let rendered = to_html(&written);
    assert!(!rendered.contains('^'), "rendered as {rendered}");
    // The image survives; only the figure ROLE is spent, and that is strictly
    // better than the caret the bare line used to add.
    assert!(
        rendered.contains("<img src=\"a.png\""),
        "rendered as {rendered}"
    );
}

/// THE CONTROL a blanket "stop writing captions" would fail. A caption that
/// spells something keeps its line, becomes the element it names, and round
/// trips.
#[test]
fn a_caption_that_spells_something_keeps_its_line() {
    let html = "<table><caption>Cap</caption><tr><td>a</td></tr></table>";
    let written = imported(html, HtmlImportMode::Semantic);
    assert_eq!(written, "| a |\n^ Cap\n");
    assert!(to_html(&written).contains("<caption>Cap</caption>"));

    // And the writer is a fixed point on it.
    assert_eq!(to_carve("| a |\n^ Cap\n"), "| a |\n^ Cap\n");
    assert_eq!(
        to_html(&to_carve("| a |\n^ Cap\n")),
        to_html("| a |\n^ Cap\n")
    );

    // The figure slot, same control.
    assert_eq!(to_carve("![A](a.png)\n^ Cap\n"), "![A](a.png)\n^ Cap\n");
}

/// U+00A0 IS CONTENT (PART 11 §7). It is the boundary the predicate must not
/// sweep, and the reason the guard defers to the writer's own trimming rather
/// than to a hand-written character set.
#[test]
fn a_caption_holding_a_no_break_space_keeps_its_line() {
    let html = "<table><caption>\u{a0}</caption><tr><td>a</td></tr></table>";
    let written = imported(html, HtmlImportMode::Semantic);
    assert_eq!(written, "| a |\n^ \u{a0}\n");
    assert!(
        to_html(&written).contains("<caption>"),
        "the no-break space lost its caption element"
    );

    // Through the AST door as well.
    let ingested = written_with_caption("| a |\n", "[{\"type\":\"text\",\"value\":\"\\u00a0\"}]");
    assert_eq!(ingested, "| a |\n^ \u{a0}\n");
}

/// A table with NO caption at all was never affected, and stays that way.
#[test]
fn a_table_with_no_caption_is_unchanged() {
    let html = "<table><tr><td>a</td></tr></table>";
    assert_eq!(imported(html, HtmlImportMode::Semantic), "| a |\n");
    assert_eq!(to_carve("| a |\n"), "| a |\n");
}
