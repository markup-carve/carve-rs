//! A definition written inside a definition list's `dd` is COLLECTED, and the
//! entry keeps no trace of it (carve-rs#668, spec markup-carve/carve#801,
//! corpus 227).
//!
//! Two halves had to move together, and each alone makes the output worse:
//!
//! - the prefix scan learns the DESCRIPTION marker, so the definition is
//!   collected. Alone, this leaves the marker behind as a stray `<p>:</p>` and
//!   the `dd` disappears;
//! - the stripped line carries the empty-content placeholder the list arm
//!   already used, so the entry survives its own line's removal as an EMPTY
//!   description. Alone, this does nothing.
//!
//! Before either, the definition rendered as visible text (`<dd>[r]: /u</dd>`)
//! and registered nothing, so a reference to it stayed literal further down.
//!
//! THE MARKER IS NOT STRIPPED UNCONDITIONALLY. A `:` line with no term above it
//! is not a description at all - it is paragraph text, and a definition in it
//! defines nothing (corpus `216-a-description-line-needs-a-term-above-it`).

#[test]
fn a_link_definition_in_a_description_is_collected() {
    let html = carve::to_html(":: term\n:  [r]: /u\n\nsee [t][r]\n");
    assert!(
        html.contains("<a href=\"/u\">t</a>"),
        "the reference did not resolve:\n{html}"
    );
    assert!(
        html.contains("<dd></dd>"),
        "the entry did not survive as an empty description:\n{html}"
    );
    assert!(
        !html.contains("<p>:</p>"),
        "the description marker was left behind as a paragraph:\n{html}"
    );
}

#[test]
fn a_footnote_definition_in_a_description_is_collected() {
    let html = carve::to_html(":: term\n:  [^f]: x\n\nsee[^f]\n");
    assert!(
        html.contains("role=\"doc-noteref\""),
        "the reference did not resolve:\n{html}"
    );
    assert!(html.contains("<dd></dd>"), "no empty description:\n{html}");
}

#[test]
fn a_link_definition_needs_a_term_above_it() {
    // Corpus 216. Without a term the line is not a description at all.
    let html = carve::to_html(":  [r]: /u\n\nsee [t][r]\n");
    assert!(
        html.contains("<p>:  [r]: /u</p>"),
        "the line should stay visible:\n{html}"
    );
    assert!(
        !html.contains("<a href=\"/u\">"),
        "it defined a reference it should not have:\n{html}"
    );
}

#[test]
fn a_footnote_definition_needs_a_term_above_it() {
    let html = carve::to_html(":  [^f]: x\n\nsee[^f]\n");
    assert!(
        !html.contains("role=\"doc-noteref\""),
        "it defined a footnote it should not have:\n{html}"
    );
}

#[test]
fn a_second_description_in_the_same_entry_collects_too() {
    // An entry is continued by a further description, so a term is not the only
    // thing that can precede one.
    let html = carve::to_html(":: term\n:  a\n:  [r]: /u\n\nsee [t][r]\n");
    assert!(
        html.contains("<a href=\"/u\">t</a>"),
        "the reference did not resolve:\n{html}"
    );
}

#[test]
fn a_term_marker_is_not_a_description_marker() {
    // `::` needs whitespace after a SINGLE colon to be a description, and does
    // not have it - so this is a term and the line is its content.
    let html = carve::to_html(":: [r]: /u\n\nsee [t][r]\n");
    assert!(
        !html.contains("<a href=\"/u\">t</a>"),
        "a term line was read as a description:\n{html}"
    );
}

#[test]
fn a_colon_fence_is_not_a_description_marker() {
    let html = carve::to_html(":: term\n\n::: note\nbody\n:::\n\nx\n");
    assert!(
        !html.contains("<dd>::: note</dd>"),
        "a fence opener was read as a description:\n{html}"
    );
}

#[test]
fn a_description_with_ordinary_content_is_untouched() {
    // The common case: nothing about a description that holds prose changes.
    let html = carve::to_html(":: term\n:  body\n");
    assert!(
        html.contains("<dd>body</dd>"),
        "an ordinary description changed:\n{html}"
    );
}
