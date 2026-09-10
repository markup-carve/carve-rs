//! PART 0, A BLOCK'S EXTENT ENDS AT A DEFINITION IT CAN ONLY TAKE AS TEXT
//! (carve#1918), ruled for this family at markup-carve/carve#1971.
//!
//! A definition below a nested note's own floor is a line that note never
//! reached, so the note ends above it. A trailing line further down then
//! belongs to the surviving ancestor rather than reaching a dead floor.
//!
//! The gather dropped such a definition outright, which erased the column the
//! recursive pass needed: the nested note kept its floor live and claimed the
//! trailing line. It now leaves the invisible placeholder at the definition's
//! own column.

use carve::to_html;

/// `[^g]` sits at column 2 with a floor of 4; the definition is at 2, below it.
#[test]
fn a_definition_below_the_nested_floor_ends_the_note() {
    let html =
        to_html("[^f]: outer\n\n  [^g]: mid\n\n  [r]: /url\n    TAILWORD\n\nx[^f] [^g] [t][r]\n");

    let fn1 = html.find("id=\"fn1\"").expect("outer note");
    let fn2 = html.find("id=\"fn2\"").expect("inner note");
    let tail = html.find("TAILWORD").expect("trailing line is published");
    assert!(
        tail > fn1 && tail < fn2,
        "trailing line belongs to the outer note: {html}"
    );
    assert!(
        html.contains("href=\"/url\""),
        "the definition still registers: {html}"
    );
}

/// The control on the other side of the floor: a definition the nested note DOES
/// reach leaves it open, and a trailing line at or past its floor is its own.
#[test]
fn a_definition_inside_the_nested_body_leaves_it_open() {
    let html =
        to_html("[^f]: outer\n\n  [^g]: mid\n\n    [r]: /url\n    TAILWORD\n\nx[^f] [^g] [t][r]\n");

    let fn2 = html.find("id=\"fn2\"").expect("inner note");
    let tail = html.find("TAILWORD").expect("trailing line is published");
    assert!(
        tail > fn2,
        "trailing line belongs to the inner note: {html}"
    );
    assert!(
        html.contains("href=\"/url\""),
        "the definition still registers: {html}"
    );
}

/// The definition is still collected wherever it lands, so the reference below
/// resolves in both bands - the fix must not cost that.
#[test]
fn the_reference_resolves_either_way() {
    for src in [
        "[^f]: outer\n\n  [^g]: mid\n\n  [r]: /url\n    TAILWORD\n\nx[^f] [^g] [t][r]\n",
        "[^f]: outer\n\n  [^g]: mid\n\n    [r]: /url\n  TAILWORD\n\nx[^f] [^g] [t][r]\n",
    ] {
        assert!(
            to_html(src).contains("href=\"/url\""),
            "unresolved for {src:?}"
        );
    }
}

/// A note with no intervening definition is untouched by this.
#[test]
fn a_plain_nested_note_keeps_its_trailing_line() {
    let html = to_html("[^f]: outer\n\n  [^g]: mid\n    TAILWORD\n\nx[^f] [^g]\n");

    let fn2 = html.find("id=\"fn2\"").expect("inner note");
    let tail = html.find("TAILWORD").expect("trailing line is published");
    assert!(
        tail > fn2,
        "trailing line belongs to the inner note: {html}"
    );
}
