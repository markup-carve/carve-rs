//! A hoisted definition is written where the tree puts it.
//!
//! §7: "Definitions appear in DOCUMENT ORDER by source position", which
//! `ast_json::ordered_document_entries` implements for the published tree
//! (carve#746). PART 11 §6 binds the writer to the same order - "fmt does not
//! reorder ... those are the author's choices and the AST records them".
//!
//! The writer rendered `children` and appended the footnote map afterwards, so
//! a link definition hoisted from INSIDE a footnote body came out before the
//! footnote containing it (carve-rs#682). A fixed kind order passes one of the
//! two shapes below and fails the other, whichever order it picks, so both are
//! pinned.

fn written(src: &str) -> String {
    carve::to_carve(src)
}

#[test]
fn a_footnote_comes_before_a_link_definition_that_follows_it() {
    // The link definition sits on the footnote body's continuation line, so it
    // is hoisted from inside the footnote and lands after it (corpus 202).
    let src = "[^a]: note\n  [r]: /u\n\nsee[^a] and [t][r]\n";
    assert_eq!(
        written(src),
        "see[^a] and [t][r]\n\n[^a]: note\n\n[r]: /u\n"
    );
}

#[test]
fn a_link_definition_comes_before_a_footnote_that_follows_it() {
    let src = "see[^a] and [t][r]\n\n[r]: /u\n\n[^a]: note\n";
    assert_eq!(written(src), src);
}

#[test]
fn two_footnotes_keep_source_order() {
    let src = "see[^b] and[^a]\n\n[^b]: bee\n\n[^a]: ay\n";
    assert_eq!(written(src), src);
}

#[test]
fn the_written_source_still_renders_the_same_html() {
    // PART 11 §1, so a reordering fix cannot change the document.
    let src = "[^a]: note\n  [r]: /u\n\nsee[^a] and [t][r]\n";
    let from_source = carve::render_html(&carve::parse(src)).expect("render");
    let from_written = carve::render_html(&carve::parse(&written(src))).expect("render");
    assert_eq!(from_source, from_written);
}
