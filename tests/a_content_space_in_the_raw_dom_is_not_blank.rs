//! markup-carve/carve-rs#1342. The RAW-DOM half of the blank test read
//! `str::trim` too, so a content space inside a `<figure>` was deleted outright
//! and one ahead of a list moved without the row that declares the move.
//!
//! `str::trim` is `char::is_whitespace`, which is Unicode `White_Space` and
//! holds NO-BREAK SPACE (U+00A0), NARROW NO-BREAK SPACE (U+202F) and
//! IDEOGRAPHIC SPACE (U+3000). markup-carve/carve#1628 puts all three on the
//! CONTENT side of the line, verified empirically rather than reasoned.
//!
//! The produced-inline spellings of this one predicate were corrected first:
//! `trim_edge_whitespace` and `visible` in carve-rs#1336, `inlines_are_blank` in
//! carve-rs#1339. These are the same rule spelled against the DOM instead.
//!
//! ## What was wrong, measured on `main` at `49083281`
//!
//! ```text
//! <figure>&#160;<img src="i.png" alt="a"><figcaption>c</figcaption></figure>
//!   before  "![a](i.png)\n^ c\n"            the character gone, diagnostics []
//!   after   "\u{a0}![a](i.png)\n^ c\n"      kept, exactly as an ordinary word is
//!
//! <ul>&#160;<li>a</li></ul>
//!   before  "\u{a0}\n\n- a\n"   diagnostics []
//!   after   "\u{a0}\n\n- a\n"   diagnostics [ElementUnwrapped]
//! ```
//!
//! The list case never lost the character - what was missing is the row saying
//! it left its place among the items, which is the part a reader cannot see
//! from the output. The same move for an ordinary word was reported all along.
//!
//! ## The controls, which are the point
//!
//! An ordinary space in either slot is LAYOUT and its treatment is unchanged: a
//! pretty-printed figure still writes no leading space, and a `<ul>` whose only
//! stray text is a newline and an indent still reports nothing. A fixture padded
//! with ordinary spaces cannot tell the two apart, so every case here carries a
//! content space and its layout twin side by side.

use carve::{html_to_ast, html_to_carve, HtmlImportOptions};

const NBSP: &str = "\u{a0}";
const NNBSP: &str = "\u{202f}";
const IDEOGRAPHIC: &str = "\u{3000}";

fn carve(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

/// EVERY code, from BOTH exits - the list half of this ticket is entirely about
/// whether a row is emitted, so a filter on one code would decide it by
/// assumption.
fn diagnostics(html: &str) -> Vec<String> {
    let mut all: Vec<String> = html_to_ast(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| format!("{:?}", d.code))
        .collect();
    all.extend(
        html_to_carve(html, &HtmlImportOptions::default())
            .expect("import")
            .report
            .diagnostics
            .iter()
            .map(|d| format!("{:?}", d.code)),
    );
    all
}

/// A CONTENT SPACE INSIDE A FIGURE IS CONTENT, so it survives on either side of
/// the image and behaves in EVERY respect like the ordinary word it is measured
/// against - including the row it earns.
///
/// ASSERTED AGAINST THE WORD RATHER THAN AGAINST AN EMPTY LIST, because the
/// resulting shape is a `<figure>` around a PARAGRAPH, which has no Carve
/// spelling and takes a real `structure-unspellable` row on the writing exit.
/// That row was there for `x` all along and is not this ticket's to remove; what
/// this ticket fixes is that the content space was not reaching the shape at
/// all. Asserting no diagnostics here would have been asserting the wrong thing
/// and would have hidden the fix behind a false expectation.
#[test]
fn a_content_space_in_a_figure_survives() {
    let word_before = "<figure>x<img src=\"i.png\" alt=\"a\"><figcaption>c</figcaption></figure>";
    let word_after = "<figure><img src=\"i.png\" alt=\"a\">x<figcaption>c</figcaption></figure>";
    assert_eq!(carve(word_before), "x![a](i.png)\n^ c\n");
    assert_eq!(carve(word_after), "![a](i.png)x\n^ c\n");

    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let before = format!(
            "<figure>{space}<img src=\"i.png\" alt=\"a\"><figcaption>c</figcaption></figure>"
        );
        assert_eq!(
            carve(&before),
            format!("{space}![a](i.png)\n^ c\n"),
            "{space:?} ahead of the image"
        );
        assert_eq!(
            diagnostics(&before),
            diagnostics(word_before),
            "{space:?} must report exactly what the word reports, no more"
        );

        let after = format!(
            "<figure><img src=\"i.png\" alt=\"a\">{space}<figcaption>c</figcaption></figure>"
        );
        assert_eq!(
            carve(&after),
            format!("![a](i.png){space}\n^ c\n"),
            "{space:?} after the image"
        );
        assert_eq!(
            diagnostics(&after),
            diagnostics(word_after),
            "{space:?} must report exactly what the word reports, no more"
        );
    }
}

/// THE BASELINE THAT COMPARISON RESTS ON, pinned in its own right so a change to
/// the word's behavior cannot silently move the bar the content space is held to.
#[test]
fn an_ordinary_word_in_the_same_slot_was_always_kept() {
    let html = "<figure>x<img src=\"i.png\" alt=\"a\"><figcaption>c</figcaption></figure>";
    assert_eq!(carve(html), "x![a](i.png)\n^ c\n");
    assert_eq!(
        diagnostics(html),
        vec!["StructureUnspellable".to_string()],
        "a figure around a paragraph is a declared loss on the writing exit"
    );
}

/// THE CONTROL. A margin is LAYOUT, and only layout - a pretty-printed figure
/// still writes no leading space, or the writer's indented image line would
/// re-parse as prose.
#[test]
fn a_layout_margin_in_a_figure_is_still_dropped() {
    for html in [
        "<figure> <img src=\"i.png\" alt=\"a\"><figcaption>c</figcaption></figure>",
        "<figure>\n  <img src=\"i.png\" alt=\"a\">\n  <figcaption>c</figcaption>\n</figure>",
        "<figure><img src=\"i.png\" alt=\"a\"><figcaption>c</figcaption></figure>",
        "<figure><img src=\"i.png\" alt=\"a\">\t<figcaption>c</figcaption></figure>",
    ] {
        assert_eq!(carve(html), "![a](i.png)\n^ c\n", "{html}");
    }
}

/// A CONTENT SPACE AHEAD OF A LIST MOVES, AND THE MOVE IS DECLARED. The
/// character was never lost here; the row was.
#[test]
fn a_content_space_ahead_of_a_list_is_declared() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let html = format!("<ul>{space}<li>a</li></ul>");
        assert_eq!(carve(&html), format!("{space}\n\n- a\n"), "{space:?}");
        assert!(
            diagnostics(&html).contains(&"ElementUnwrapped".to_string()),
            "{space:?}: the move must be declared, got {:?}",
            diagnostics(&html)
        );
    }

    // The word it has to match, which was reported all along.
    assert_eq!(carve("<ul>x<li>a</li></ul>"), "x\n\n- a\n");
    assert!(diagnostics("<ul>x<li>a</li></ul>").contains(&"ElementUnwrapped".to_string()));
}

/// THE CONTROL ON THE LIST SIDE. Inter-element layout whitespace is not text an
/// author wrote, so it neither moves nor is reported.
#[test]
fn layout_whitespace_ahead_of_a_list_is_still_silent() {
    for html in [
        "<ul> <li>a</li></ul>",
        "<ul>\n  <li>a</li>\n</ul>",
        "<ul>\t<li>a</li></ul>",
    ] {
        assert_eq!(carve(html), "- a\n", "{html}");
        assert_eq!(diagnostics(html), Vec::<String>::new(), "{html}");
    }
}
