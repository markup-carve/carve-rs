//! A footnote definition nested inside another footnote's body is its own note,
//! at any indent past its parent, and a reference below the stack resolves
//! (markup-carve/carve#1946/#1959, corpus section 456).
//!
//! A note body recognizes a `[^x]: ` definition at ANY indent flatly, rather
//! than by a per-level content column - so a definition one column deeper than
//! its parent is still hoisted. This engine lost such a definition, reading it
//! as text inside the outer note.
//!
//! ORACLE: the executable spec at spec main; the deep-nesting footnote family
//! is corpus-pinned and 1687/1687 green (carve#1937), so the answer is the
//! oracle's correct reading, not a divergence.

use carve::to_html;

fn note_count(html: &str) -> usize {
    html.matches("<li id=\"fn").count()
}

/// The reported document: three nested definitions, a reference line resolving
/// all three plus a link reference consumed inside the stack.
#[test]
fn the_reported_stack_is_three_notes_and_the_reference_resolves() {
    let html = to_html(
        "[^f]: outer\n\n   [^g]: mid\n\n    [^h]: inner\n\n  [r]: /url\n  TAILWORD\n\nx[^f] [^g] [^h] [t][r]\n",
    );
    assert_eq!(note_count(&html), 3, "{html}");
    assert!(
        html.contains("href=\"/url\""),
        "the [r] reference resolves: {html}"
    );
    assert!(
        !html.contains("[^h]"),
        "[^h] is a note, not literal text: {html}"
    );
}

/// The innermost definition one column deeper than its parent is still a note.
#[test]
fn a_definition_one_column_deeper_than_its_parent_is_a_note() {
    let html = to_html("[^f]: outer\n\n   [^g]: mid\n\n    [^h]: inner\n\nx[^f] [^g] [^h]\n");
    assert_eq!(note_count(&html), 3, "{html}");
}

/// A definition three columns deeper still nests (this already worked).
#[test]
fn a_definition_three_columns_deeper_still_nests() {
    let html = to_html("[^f]: outer\n\n   [^g]: mid\n\n      [^h]: inner\n\nx[^f] [^g] [^h]\n");
    assert_eq!(note_count(&html), 3, "{html}");
}

/// The single-level control is unchanged: a definition at the top note's own
/// body floor (indent 2) nests; one at indent 1 stays text.
#[test]
fn the_single_level_floor_is_unchanged() {
    assert_eq!(
        note_count(&to_html("[^f]: o\n\n  [^g]: m\n\nx[^f] [^g]\n")),
        2,
        "indent 2 nests"
    );
    assert_eq!(
        note_count(&to_html("[^f]: o\n\n [^g]: m\n\nx[^f] [^g]\n")),
        1,
        "indent 1 stays text"
    );
}

/// Two flat footnotes are unaffected.
#[test]
fn flat_footnotes_are_unaffected() {
    assert_eq!(
        note_count(&to_html("[^a]: one\n[^b]: two\n\nx[^a] [^b]\n")),
        2
    );
}
