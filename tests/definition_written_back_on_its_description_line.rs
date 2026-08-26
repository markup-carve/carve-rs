//! A definition collected from a definition list's description is written back
//! ON THAT DESCRIPTION LINE (spec markup-carve/carve#805).
//!
//! Collecting it empties the `dd` (markup-carve/carve#801), and an empty
//! description has no source spelling - the production requires content after
//! the marker - so the writer emitted a bare `:` line, which re-parses as a
//! continuation of the term above it. `to_html(fmt(x)) == to_html(x)`, PART 11
//! section 1, failed on the two corpus documents that rule added, and the
//! corpus bump was blocked on it.
//!
//! Nothing new was needed in the language. The description already keeps the
//! span of its own marker line and the hoisted definition keeps the span it was
//! written at (PART 12 section 4), and the two name the SAME line - so the
//! description writes the definition back on it and the document-level pass
//! skips what a description already claimed.

use carve::{to_carve, to_html};

fn round_trips(source: &str) -> bool {
    to_html(&to_carve(source)) == to_html(source)
}

/// The spans the writer depends on, exercised THROUGH the writer.
///
/// Asking `parse_with_options` for spans here would prove nothing: it would
/// enable them itself, and pass against a `to_carve` that parses without any.
/// So the assertion is on `to_carve`'s own output for the one shape whose
/// spelling is reconstructed from a span - if the carve target ever stops
/// parsing with spans, this comes back as the bare `:` line the ticket
/// describes.
#[test]
fn the_carve_target_parses_with_spans() {
    let out = to_carve(":: term\n: [r]: /u\n\nsee [t][r]\n");
    assert!(
        !out.contains(":\n"),
        "an emptied description came back as a bare colon, so the carve target \
         parsed without spans: {out:?}"
    );
    assert!(out.contains(": [r]: /u"), "{out:?}");
}

#[test]
fn a_link_definition_is_written_back_on_its_own_line() {
    let source = ":: term\n: [r]: /u\n\nsee [t][r]\n";
    assert_eq!(to_carve(source), source);
    assert!(round_trips(source));
}

#[test]
fn a_footnote_definition_is_written_back_on_its_own_line() {
    let source = ":: term\n: [^f]: x\n\nsee[^f]\n";
    assert_eq!(to_carve(source), source);
    assert!(round_trips(source));
}

/// The document-level pass must skip what a description claimed; writing both
/// would define the label twice.
#[test]
fn the_definition_is_not_written_twice() {
    let out = to_carve(":: term\n: [r]: /u\n\nsee [t][r]\n");
    assert_eq!(out.matches("[r]: /u").count(), 1, "{out}");
}

#[test]
fn the_footnote_is_not_written_twice() {
    let out = to_carve(":: term\n: [^f]: x\n\nsee[^f]\n");
    assert_eq!(out.matches("[^f]: x").count(), 1, "{out}");
}

/// `render_carve_once` renders the document up to THREE times and picks between
/// the forms (PART 11 section 4). Bookkeeping that survives one pass tells the
/// next that every definition is already placed, so the description emits a bare
/// `:` again AND the document-level arm emits nothing: the definition is deleted
/// outright. A second entry is enough to reach a later form.
#[test]
fn an_emptied_description_survives_every_escape_pass() {
    let source = ":: t1\n: [r]: /u\n\n:: t2\n: d2\n\nsee [t][r]\n";
    let out = to_carve(source);
    assert_eq!(out.matches("[r]: /u").count(), 1, "{out}");
    assert!(round_trips(source));
}

#[test]
fn an_emptied_description_as_the_last_entry_survives_every_pass() {
    let source = ":: t1\n: d1\n\n:: t2\n: [r]: /u\n\nsee [t][r]\n";
    let out = to_carve(source);
    assert_eq!(out.matches("[r]: /u").count(), 1, "{out}");
    assert!(round_trips(source));
}

/// The neighbouring shapes, so the fix is bounded rather than shaped around the
/// two corpus documents that found it.
#[test]
fn an_emptied_description_beside_an_ordinary_one() {
    let source = ":: term\n: [r]: /u\n: body\n\nsee [t][r]\n";
    assert_eq!(to_carve(source), source);
    assert!(round_trips(source));
}

#[test]
fn an_emptied_description_inside_a_block_quote() {
    let source = "> :: term\n> : [r]: /u\n>\n> see [t][r]\n";
    assert!(round_trips(source));
}

#[test]
fn an_emptied_description_inside_a_list_item() {
    let source = "- :: term\n  : [r]: /u\n\nsee [t][r]\n";
    assert!(round_trips(source));
}

/// The container walk that finds an emptied description, exercised where it is
/// reachable. Inside an admonition the definition IS collected and the `dd` IS
/// emptied, so a walk that stopped at the document's own children would leave
/// this one writing a bare `:`.
#[test]
fn an_emptied_description_inside_an_admonition() {
    let source = "::: note\n:: term\n: [r]: /u\n:::\n\nsee [t][r]\n";
    assert_eq!(to_carve(source), source);
    assert!(round_trips(source));
}

#[test]
fn an_ordinary_description_is_unchanged() {
    assert_eq!(to_carve(":: term\n: body\n"), ":: term\n: body\n");
}

#[test]
fn a_definition_written_at_document_level_stays_where_it_was() {
    assert_eq!(
        to_carve("[r]: /u\n\nsee [t][r]\n"),
        "see [t][r]\n\n[r]: /u\n"
    );
}

#[test]
fn a_footnote_written_at_document_level_stays_where_it_was() {
    assert_eq!(to_carve("[^f]: x\n\nsee[^f]\n"), "see[^f]\n\n[^f]: x\n");
}
