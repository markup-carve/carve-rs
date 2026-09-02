//! A definition written BETWEEN two live content columns belongs to the outer
//! of the two and registers (markup-carve/carve-rs#1505, ruled by
//! markup-carve/carve#1896).
//!
//! The reported document is `- - x` with `   [r]: /url` under it. The live
//! columns are 2 and 4, the definition sits at 3, and this engine folded it
//! into the open paragraph: `[r]: /url` came back as visible text and
//! `See [r][].` stayed literal. One space less or one space more registered.
//!
//! WHY THE OUTER COLUMN OWNS IT. The item at column 4 opened on the marker line
//! and never on this one, so it owns nothing here; the item at 2 does. Reaching
//! a column is the deepest one AT OR BELOW the indent, and a deeper column the
//! line does not reach does not change that answer. Past a container's content
//! column, more indentation may change which container owns a line, never
//! whether the line is a definition at all.
//!
//! MEASURED, NOT ASSUMED. All 332 prefix/column pairs over the l(ist)/q(uote)
//! container prefixes up to depth four, with the definition at every column
//! from the last quote marker out to 14, were run through the executable spec
//! at the pinned corpus (`tests/spec` at carve `86569bd`) and through this
//! engine. Before: 11 disagreed, all of them this shape, all of them the engine
//! folding what the spec registers. After: 332/332 agree, for the link kind and
//! the footnote kind alike. The 11 are pinned below, with the controls a looser
//! fix breaks.
//!
//! The predecessor of this file asserted the opposite and claimed the
//! executable spec answered the reported document the same way. It does not,
//! and the sweep above is the first time that claim was measured.

use carve::to_html;

fn document(opener: &str, definition: &str) -> String {
    format!("{opener}\n{definition}\n\nSee [r][].\n")
}

/// The eleven documents the sweep found, as `(marker line, definition line)`.
const BETWEEN: &[(&str, &str)] = &[
    ("- - x", "   [r]: /url"),
    ("- - - x", "   [r]: /url"),
    ("- - - x", "     [r]: /url"),
    ("> - - x", ">    [r]: /url"),
    ("- - - - x", "   [r]: /url"),
    ("- - - - x", "     [r]: /url"),
    ("- - - - x", "       [r]: /url"),
    ("- > - - x", "  >    [r]: /url"),
    ("> - - - x", ">    [r]: /url"),
    ("> - - - x", ">      [r]: /url"),
    ("> > - - x", "> >    [r]: /url"),
];

#[test]
fn the_reported_document_renders_what_the_spec_renders() {
    let html = to_html(&document("- - x", "   [r]: /url"));
    assert_eq!(
        html.trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>x</li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>See <a href=\"/url\">r</a>.</p>",
        ),
        "{html}"
    );
}

#[test]
fn all_four_neighbouring_columns_answer_the_same_way() {
    // What markup-carve/carve#1896 asks a corpus row to pin, kept here because
    // the corpus is the pinned spec submodule and is not authored in this repo.
    // The hole was column 3 alone: 2 and 4 registered on either side of it, so
    // the tell was that one more space stopped the definition being one and one
    // further space brought it back. Column 1 reaches nothing and is the floor.
    let below = to_html(&document("- - x", " [r]: /url"));
    assert!(
        !below.contains("href=\"/url\""),
        "column 1 registered: {below}"
    );
    assert!(
        below.contains("[r]: /url"),
        "column 1 stopped being text: {below}"
    );

    let registered = to_html(&document("- - x", "  [r]: /url"));
    assert!(
        registered.contains("href=\"/url\""),
        "column 2: {registered}"
    );
    for indent in [3usize, 4, 5] {
        let html = to_html(&document(
            "- - x",
            &format!("{}[r]: /url", " ".repeat(indent)),
        ));
        assert_eq!(
            html, registered,
            "column {indent} does not answer as column 2 does: {html}"
        );
    }
}

#[test]
fn every_between_column_definition_registers_and_leaves_no_text_behind() {
    // Registering it must also REMOVE it: the failure was not lossy, the line
    // came back as paragraph text, so a fix that resolves the reference and
    // still prints the definition has only moved the defect.
    for (opener, definition) in BETWEEN {
        let html = to_html(&document(opener, definition));
        assert!(
            html.contains("href=\"/url\""),
            "{opener:?} / {definition:?}: {html}"
        );
        assert!(
            !html.contains("[r]: /url"),
            "the definition stayed visible for {opener:?} / {definition:?}: {html}"
        );
    }
}

#[test]
fn the_footnote_kind_moves_with_it() {
    // Both prepasses share the column strip, so both kinds move together. A fix
    // that moved only one would sort definitions by kind, which is the tell this
    // family keeps producing.
    for (opener, definition) in BETWEEN {
        let note = definition.replace("[r]: /url", "[^f]: note");
        let src = format!("{opener}\n{note}\n\nSee [^f].\n");
        let html = to_html(&src);
        assert!(html.contains("doc-endnotes"), "{src:?}: {html}");
        assert!(
            !html.contains("[^f]: note"),
            "the definition stayed visible: {html}"
        );
    }
}

#[test]
fn a_line_that_reaches_no_column_is_still_lazy_paragraph_text() {
    // THE CONTROLS, and the ones a looser fix breaks. Absorbing the residual
    // indent is allowed only once the line has REACHED a content column; below
    // every column there is nothing to absorb it into and the line is a lazy
    // continuation of the open paragraph, which defines nothing (PART 9R R1a).
    // Each of these renders byte-identically to the executable spec.
    for src in [
        // No container at all.
        "x\n [r]: /url\n\nSee [r][].\n",
        // Inside a quote whose only block is the paragraph.
        "> x\n>  [r]: /url\n\nSee [r][].\n",
        "- > x\n  >  [r]: /url\n\nSee [r][].\n",
        // Above the outermost item's own content column.
        "- - x\n [r]: /url\n\nSee [r][].\n",
        // Between the quote's content column and the first item's.
        "- > - - x\n  >  [r]: /url\n\nSee [r][].\n",
    ] {
        let html = to_html(src);
        assert!(
            !html.contains("href=\"/url\""),
            "registered from lazy text: {src:?}: {html}"
        );
        assert!(
            html.contains("[r]: /url"),
            "the line stopped being text: {src:?}: {html}"
        );
    }
}

#[test]
fn a_commented_out_between_column_definition_still_registers_nothing() {
    // INTENDED SURVIVOR (markup-carve/carve#1341): reaching the definition is
    // not registering it, and the widened strip must still stop at a comment
    // span. Byte-identical to the executable spec.
    let src = "- - x\n   %%%\n   [r]: /url\n   %%%\n\nSee [r][].\n";
    assert!(!to_html(src).contains("href=\"/url\""), "{}", to_html(src));
}

#[test]
fn a_fenced_between_column_definition_is_still_content() {
    // The other survivor of the same kind: a definition inside a fence written
    // at a between-column indent is content, and the widened strip must not
    // walk into it. Only the DEFINITION question is pinned here. Which block
    // the fence itself opens at that indent is a SEPARATE divergence from the
    // executable spec, unchanged by this fix and filed as carve-rs#1506.
    let src = "- - x\n   ```\n   [r]: /url\n   ```\n\nSee [r][].\n";
    let html = to_html(src);
    assert!(!html.contains("href=\"/url\""), "{html}");
    assert!(
        html.contains("[r]: /url"),
        "the fenced line disappeared: {html}"
    );
    assert!(html.contains("<p>See [r][].</p>"), "{html}");
}
