//! A DEFINITION INDENTED INSIDE A NOTE BODY IS AT THE BODY'S OWN COLUMN
//! (markup-carve/carve#1918 row 14, markup-carve/carve-rs#1532).
//!
//! The collector's dedent has already moved a note body to column 0, so a
//! definition written under something inside that body keeps a residual
//! indent. The recursive pass read that as an indented line rather than a
//! definition, and the note never registered. The walk already absorbed the
//! same residual for a LINK definition; only the footnote shape kept it.
//!
//! WHAT BOUNDS THE GATE is that the body is OPEN and the line IS a definition
//! outside any fence - not the column it sits at. A footnote definition
//! registers wherever it lands and is hoisted to the endnotes either way, so
//! there is no upper column bound to enforce. A flush line opens the body; a
//! block opener at any column closes it.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at
//! carve `47e4f4b7`, spec main - also corpus section
//! `447-the-host-does-not-change-which-column-a-definition-reaches`, which this
//! takes from 1 failing row to 0.

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
    let normalize = |html: &str| html.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(&both_paths(src)),
        normalize(expected),
        "on {src:?}"
    );
}

/// The first code block's payload, EXACTLY - whitespace included.
///
/// `assert_html` collapses runs of space, which is what a structural comparison
/// wants and is exactly blind to a fence whose content lost a column. Both
/// findings `codex review` raised on this change were of that shape, so the
/// fence rows assert the payload rather than the shape.
fn assert_code_payload(src: &str, expected: &str) {
    let html = both_paths(src);
    let start = html.find("<code>").expect("a code block") + "<code>".len();
    let end = html[start..].find("</code>").expect("a closed code block") + start;
    assert_eq!(&html[start..end], expected, "on {src:?}");
}

/// Corpus row 14. `[^n]` is written under a note body with a residual indent,
/// so before this it reached the recursive pass as an indented line and never
/// registered; now the document holds three notes.
#[test]
fn the_reported_document_registers_three_notes() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n   [^n]: note text\n\nSee [^n] and [^f] and [^g].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a> and <a \
         id=\"fnref3\" href=\"#fn3\" role=\"doc-noteref\"><sup>3</sup></a>.</p> <section \
         role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> \
         <p>note text<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>b<a href=\"#fnref2\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> <li \
         id=\"fn3\"> <p>c<a href=\"#fnref3\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
}

/// THE LINK TWIN, which the walk already absorbed before this change - what
/// says the footnote shape was the outlier rather than the column rule.
#[test]
fn the_link_definition_twin_reads_the_same_column() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n   [r]: /u\n\nSee [r][] and [^f] and [^g].\n",
        "<p>See <a href=\"/u\">r</a> and <a id=\"fnref1\" href=\"#fn1\" \
         role=\"doc-noteref\"><sup>1</sup></a> and <a id=\"fnref2\" href=\"#fn2\" \
         role=\"doc-noteref\"><sup>2</sup></a>.</p> <section role=\"doc-endnotes\" \
         aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> <p>b<a href=\"#fnref1\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> <li \
         id=\"fn2\"> <p>c<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
}

/// AT THE BODY'S OWN COLUMN there is no residual to absorb and the line was
/// already the body's. Unchanged, before and after.
#[test]
fn a_definition_at_the_bodys_own_column_is_unchanged() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n  [^n]: note\n\nSee [^n] and [^f] and [^g].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a> and <a \
         id=\"fnref3\" href=\"#fn3\" role=\"doc-noteref\"><sup>3</sup></a>.</p> <section \
         role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> \
         <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>b<a href=\"#fnref2\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> <li \
         id=\"fn3\"> <p>c<a href=\"#fnref3\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
}

/// WITH NOTHING OPEN ABOVE IT the flag is still false, so the gate does not
/// fire and the definition is read where it was written.
#[test]
fn no_nested_note_means_nothing_to_be_between() {
    assert_html(
        "[^f]: b\n\n   [^n]: note\n\nSee [^n] and [^f].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a>.</p> \
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li \
         id=\"fn1\"> <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>b<a href=\"#fnref2\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> </ol> \
         </section>",
    );
}

/// PAST A NESTED NOTE'S OWN COLUMN it still registers. A first version
/// absorbed only strictly BELOW that note's floor, reading an at-or-past line
/// as the nested note's own. A footnote definition registers wherever it
/// lands and is hoisted either way, so the oracle takes it at every column -
/// the bound only withheld 50 documents it accepts, and once it was gone the
/// floor's VALUE was never read at all.
#[test]
fn a_definition_past_the_nested_notes_column_registers_too() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n     [^n]: note\n\nSee [^n] and [^f] and [^g].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a> and <a \
         id=\"fnref3\" href=\"#fn3\" role=\"doc-noteref\"><sup>3</sup></a>.</p> <section \
         role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> \
         <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>b<a href=\"#fnref2\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> <li \
         id=\"fn3\"> <p>c<a href=\"#fnref3\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
}

/// A FLUSH LINE OPENS THE BODY, not only a nested definition. The body is
/// what an indented definition below is measured against, and plain prose
/// leaves it open - requiring a definition to open it withheld 26 documents.
#[test]
fn prose_opens_the_body_for_an_indented_definition() {
    assert_html(
        "[^f]: b\n\n  text\n   [^n]: note\n\nSee [^n] and [^f].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a>.</p> \
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li \
         id=\"fn1\"> <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>b</p> <p>text<a href=\"#fnref2\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> </ol> \
         </section>",
    );
}

/// A LIST DOES NOT CLOSE THE BODY. `codex review` asked for one to, on the
/// reading that `item_block_opener` excludes list markers deliberately.
/// Measured, closing here moves 11 documents OFF the oracle and fixes none,
/// so the finding is declined and this row is why.
#[test]
fn a_list_between_them_does_not_close_the_body() {
    assert_html(
        "[^f]: b\n\n  - z\n   [^n]: note\n\nSee [^n] and [^f].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a>.</p> \
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li \
         id=\"fn1\"> <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>b</p> <ul> <li>z</li> </ul> \
         <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
}

/// ONLY A DEFINITION IS ABSORBED. An indented line that is not one is the
/// body's own text and keeps its column - dedenting it moves it into the
/// note as a second paragraph.
#[test]
fn only_a_definition_is_absorbed() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n    text\n\nSee [^f] and [^g].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a>.</p> \
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li \
         id=\"fn1\"> <p>b<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>c text<a href=\"#fnref2\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> </ol> \
         </section>",
    );
}

/// A PAYLOAD LINE INSIDE A FENCE DOES NOT RE-OPEN THE BODY. The flush arm
/// would otherwise take a fence's own content as a body line, and the absorb
/// below it would eat a column from a definition-shaped line of code. Raised
/// by `codex review` on the second round, after the first round's fix.
#[test]
fn a_fence_payload_line_does_not_reopen_the_body() {
    assert_html(
        "[^f]: b\n\n  ```\n  payload\n   [^n]: note\n  ```\n\nSee [^n] and [^f].\n",
        "<p>See [^n] and <a id=\"fnref1\" href=\"#fn1\" \
         role=\"doc-noteref\"><sup>1</sup></a>.</p> <section role=\"doc-endnotes\" \
         aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> <p>b</p> \
         <pre><code>payload [^n]: note </code></pre> <p><a href=\"#fnref1\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> </ol> \
         </section>",
    );
    assert_code_payload(
        "[^f]: b\n\n  ```\n  payload\n   [^n]: note\n  ```\n\nSee [^n] and [^f].\n",
        "payload\n [^n]: note\n",
    );
}

/// AN OPAQUE PAYLOAD IS NOT ABSORBED. The fence tracking beside this walk
/// only sees a FLUSH fence, so an indented one left the flag standing and the
/// dedent ate a column of the code block - 40 documents. Raised by `codex
/// review`; no sweep here saw it, because the note still registered and only
/// the code block's indentation moved.
#[test]
fn an_indented_fence_keeps_its_payload_opaque() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n   ```\n    [^n]: note\n   ```\n\nSee [^n] and [^f] and [^g].\n",
        "<p>See [^n] and <a id=\"fnref1\" href=\"#fn1\" \
         role=\"doc-noteref\"><sup>1</sup></a> and <a id=\"fnref2\" href=\"#fn2\" \
         role=\"doc-noteref\"><sup>2</sup></a>.</p> <section role=\"doc-endnotes\" \
         aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> <p>b</p> <pre><code> [^n]: \
         note </code></pre> <p><a href=\"#fnref1\" role=\"doc-backlink\" \
         aria-label=\"Back to reference\">↩</a></p> </li> <li id=\"fn2\"> <p>c<a \
         href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
    assert_code_payload(
        "[^f]: b\n\n  [^g]: c\n   ```\n    [^n]: note\n   ```\n\nSee [^n] and [^f] and [^g].\n",
        " [^n]: note\n",
    );
}

/// A BLOCK CLOSES THE BODY for this purpose, so a definition below it is read
/// where it was written. Without the close the stale flag absorbs it and the
/// fence loses its content.
#[test]
fn a_block_between_them_closes_the_nested_body() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n  ```\n   [^n]: note\n\nSee [^n] and [^f] and [^g].\n",
        "<p>See [^n] and <a id=\"fnref1\" href=\"#fn1\" \
         role=\"doc-noteref\"><sup>1</sup></a> and <a id=\"fnref2\" href=\"#fn2\" \
         role=\"doc-noteref\"><sup>2</sup></a>.</p> <section role=\"doc-endnotes\" \
         aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> <p>b</p> <pre><code> [^n]: \
         note </code></pre> <p><a href=\"#fnref1\" role=\"doc-backlink\" \
         aria-label=\"Back to reference\">↩</a></p> </li> <li id=\"fn2\"> <p>c<a \
         href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
}

/// PLAIN PROSE DOES NOT CLOSE IT - it is the body's own lazy continuation -
/// so the definition below it still registers. Closing on every
/// non-definition line lost this.
#[test]
fn prose_between_them_keeps_the_nested_body_open() {
    assert_html(
        "[^f]: b\n\n  [^g]: c\n  text\n   [^n]: note\n\nSee [^n] and [^f] and [^g].\n",
        "<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> and \
         <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a> and <a \
         id=\"fnref3\" href=\"#fn3\" role=\"doc-noteref\"><sup>3</sup></a>.</p> <section \
         role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> \
         <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> <li id=\"fn2\"> <p>b</p> <p>text<a href=\"#fnref2\" \
         role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> <li \
         id=\"fn3\"> <p>c<a href=\"#fnref3\" role=\"doc-backlink\" aria-label=\"Back to \
         reference\">↩</a></p> </li> </ol> </section>",
    );
}
