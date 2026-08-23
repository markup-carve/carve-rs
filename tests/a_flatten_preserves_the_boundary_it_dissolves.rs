//! PART 11 §1b: a flatten preserves the boundary it dissolves
//! (markup-carve/carve#1325, corpus-convert 29 through 32).
//!
//! A slot that takes INLINE content only - a caption line, a fence title, a
//! table cell, an image's alternative text, a definition term - cannot carry
//! blocks, so a producer handed block content for one FLATTENS it. The flatten
//! is lossy by construction, so §1's round-trip invariant is not the rule that
//! applies. What the producer still owes is the BOUNDARY:
//!
//! > Where two former sibling blocks each contribute at least one TOKEN to the
//! > slot, the producer MUST emit a separator, and a separator is sufficient if
//! > and only if re-reading the emitted slot draws no token from both sides of
//! > the join.
//!
//! THE UNIT IS THE TOKEN, NOT THE NODE, and that difference is the whole rule.
//! A node test passes `onetwo` and `one two` alike - both are a single `text`
//! node - while the boundary a reader recovers from one and cannot recover from
//! the other is a token boundary. So the assertions here are on the BYTES the
//! importer writes and on the HTML they re-read as, never on the node count.
//!
//! It is PART 9 §23's test read backwards. §23 says a LINE boundary survives
//! because it leaves a node; a BLOCK boundary in an inline slot leaves none,
//! because the slot has nowhere to put one, so it survives only in the bytes.
//!
//! All three engines emitted the joined form when this was ruled, so there was
//! no reference to copy: the four corpus-convert documents are the reference.

use carve::{html_to_carve, to_html, HtmlImportOptions};

fn migrated(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

fn caption(inner: &str) -> String {
    format!("<figure><img src=\"/i\" alt=\"x\"><figcaption>{inner}</figcaption></figure>")
}

/// corpus-convert 29. The boundary lost as CONTENT: `onetwo` re-reads as one
/// word and the reader sees a misspelling.
#[test]
fn a_flattened_paragraph_boundary_survives_as_a_word_boundary() {
    let src = migrated(&caption("<p>one</p><p>two</p>"));
    assert!(src.contains("^ one two"), "{src}");
    assert!(to_html(&src).contains("<figcaption>one two</figcaption>"));
}

/// corpus-convert 30. The boundary lost as STRUCTURE: `*a**b*` re-reads as one
/// strong run holding a literal asterisk, so the second run is gone entirely.
#[test]
fn flattened_emphasis_runs_do_not_merge_into_one() {
    let src = migrated(&caption(
        "<p><strong>a</strong></p><p><strong>b</strong></p>",
    ));
    assert!(src.contains("^ *a* *b*"), "{src}");
    assert!(
        to_html(&src).contains("<figcaption><strong>a</strong> <strong>b</strong></figcaption>"),
        "{}",
        to_html(&src)
    );
}

/// corpus-convert 31. NOT among the shapes markup-carve/carve#1325 reported: it
/// fell out of stating the rule as a property rather than as a list of the pairs
/// that collide, which is the argument for stating it that way. Two code spans
/// join into one holding the delimiters.
#[test]
fn flattened_code_spans_do_not_merge_into_one() {
    let src = migrated(&caption("<p><code>a</code></p><p><code>b</code></p>"));
    assert!(src.contains("^ `a` `b`"), "{src}");
    assert!(
        to_html(&src).contains("<figcaption><code>a</code> <code>b</code></figcaption>"),
        "{}",
        to_html(&src)
    );
}

/// corpus-convert 32. A BLOCK THAT CONTRIBUTES NOTHING IS NOT A SIDE, which is
/// why the rule is written over contributed tokens rather than over sibling
/// positions: three blocks, one join, and the caption is `a b` rather than
/// `a  b`. This is the control on the naive reading "one separator per gap".
#[test]
fn an_empty_flattened_block_is_not_a_side() {
    let src = migrated(&caption("<p>a</p><p></p><p>b</p>"));
    assert!(src.contains("^ a b"), "{src}");
    assert!(!src.contains("a  b"), "a separator no author wrote: {src}");
}

/// AND NEITHER IS A WHITESPACE-ONLY ONE, which is the same question asked of a
/// block that holds something rather than nothing. A pretty-printed document
/// puts newlines between the paragraphs, and those must not each become a
/// separator of their own.
#[test]
fn whitespace_between_the_blocks_adds_no_second_separator() {
    let src = migrated(&caption("\n  <p>one</p>\n  <p>two</p>\n"));
    assert!(src.contains("^ one two"), "{src}");
    assert!(!src.contains("one  two"), "{src}");
}

/// A SINGLE BLOCK HAS NO BOUNDARY TO PRESERVE. The separator is emitted at a
/// JOIN, so a caption holding one paragraph must be byte-identical to the same
/// caption holding bare inline content, with no leading or trailing space.
#[test]
fn one_flattened_block_takes_no_separator() {
    assert_eq!(migrated(&caption("<p>one</p>")), migrated(&caption("one")));
    assert!(migrated(&caption("<p>one</p>")).contains("^ one\n"));
}

/// THE ESCAPING HALF IS A DIFFERENT QUESTION and is unchanged. A character that
/// was TEXT in one block and becomes a live delimiter once its neighbour is
/// beside it is answered by §2 read through §1a - the writer reads its own
/// output and escapes what would otherwise change the document. This clause
/// supplies the space and nothing else, so both are visible here at once.
///
/// ONE ASTERISK, not both. Strong needs an opener AND a closer, so suppressing
/// the opener is the whole of it and the second asterisk opens nothing on its
/// own - PART 11 §2 per opener occurrence (markup-carve/carve#1533). The
/// unit-scoped form escaped both, and the second backslash was idle.
#[test]
fn the_escaping_half_is_unchanged_and_the_space_is_added_beside_it() {
    let src = migrated(&caption("<p>a *b</p><p>c* d</p>"));
    assert!(
        src.contains(r"\*"),
        "the opening asterisk stopped being escaped: {src}"
    );
    assert!(src.contains("^ a \\*b c* d"), "{src}");
}

/// AN INLINE SIBLING IS NOT A BLOCK, so no separator is owed beside one. This is
/// the row that says the rule is about a dissolved BLOCK boundary rather than
/// about every gap between two children.
#[test]
fn adjacent_inline_children_are_not_joined_by_a_separator() {
    let src = migrated(&caption("<strong>a</strong><em>b</em>"));
    assert!(src.contains("^ *a*/b/"), "{src}");
}
