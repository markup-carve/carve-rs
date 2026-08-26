//! A collected definition is written back on the line the author wrote it on
//! (markup-carve/carve#805, corpus 227 and 228).
//!
//! Collecting a definition out of a container empties the place it was written.
//! Two shapes, and the emptied place has no source spelling in either:
//!
//!   - a definition-list description becomes an empty `dd`, and the writer
//!     emitted a bare `:` line, which re-parses as a CONTINUATION of the term
//!     above it;
//!   - a definition between two blocks of a list item leaves a gap, and the gap
//!     is what SPLIT one paragraph into two - dropping the line rejoins them.
//!
//! Either way `parse(fmt(x)) == parse(x)` (PART 11 §1) failed. Nothing new was
//! needed: the entry keeps the line it was written on and the definition node
//! keeps its `pos` (PART 12 §4), so the two name the same line.
//!
//! carve-js#748 and carve-php#903 did this first; this is the port.

fn fmt(source: &str) -> String {
    carve::to_carve(source)
}

fn round_trips(source: &str) -> bool {
    carve::to_html(source) == carve::to_html(&fmt(source))
}

#[test]
fn a_definition_in_a_description_is_written_back_on_that_line() {
    let source = ":: term\n: [r]: /u\n\nsee [t][r]\n";
    assert_eq!(fmt(source), source);
    assert!(round_trips(source));
}

#[test]
fn a_footnote_in_a_description_is_written_back_on_that_line() {
    let source = ":: term\n: [^f]: x\n\nsee[^f]\n";
    assert_eq!(fmt(source), source);
    assert!(round_trips(source));
}

#[test]
fn a_footnote_in_an_item_gap_is_written_back_on_that_line() {
    let source = "- a\n  [^f]: x\n  more\n\nsee[^f]\n";
    assert_eq!(fmt(source), source);
    assert!(round_trips(source));
}

#[test]
fn a_link_definition_in_an_item_gap_is_written_back_on_that_line() {
    let source = "- a\n  [r]: /u\n  more\n\nsee [t][r]\n";
    assert_eq!(fmt(source), source);
    assert!(round_trips(source));
}

#[test]
fn the_definition_is_not_also_written_at_document_level() {
    // Writing it in both places would define the label twice.
    let out = fmt("- a\n  [r]: /u\n  more\n\nsee [t][r]\n");
    assert_eq!(out.matches("[r]: /u").count(), 1, "written twice:\n{out}");
}

#[test]
fn the_item_split_survives_the_round_trip() {
    // The point of writing the line back: without it the two blocks rejoin into
    // one paragraph, and a paragraph with a soft break renders differently from
    // two tight blocks.
    let html = carve::to_html(&fmt("- a\n  [^f]: x\n  more\n\nsee[^f]\n"));
    assert!(
        html.contains("<li>a\n    more\n  </li>"),
        "the item's two blocks were rejoined:\n{html}"
    );
}

#[test]
fn a_definition_written_at_document_level_stays_there() {
    // The neighbouring case: nothing claims it, so the writer's ordinary
    // placement is unchanged.
    assert_eq!(fmt("[r]: /u\n\nsee [t][r]\n"), "see [t][r]\n\n[r]: /u\n");
    assert_eq!(fmt("[^f]: x\n\nsee[^f]\n"), "see[^f]\n\n[^f]: x\n");
}

#[test]
fn an_ordinary_description_is_unchanged() {
    // The control: a description with real content must not gain a definition
    // from anywhere.
    assert_eq!(fmt(":: term\n: body\n"), ":: term\n: body\n");
}

#[test]
fn an_item_with_no_definition_is_unchanged() {
    // The other control: an item whose blocks were separated by something the
    // writer keeps must not gain a definition line.
    let source = "- a\n\n  more\n";
    assert!(round_trips(source));
    assert!(
        !fmt(source).contains("]:"),
        "invented a definition: {:?}",
        fmt(source)
    );
}
