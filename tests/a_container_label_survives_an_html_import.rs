//! PART 9 §10's grouping `[label]` comes back on the OPENER, not as a paragraph.
//!
//! The renderer surfaces an unconsumed label as `<p class="div-label">` so that
//! an extension nobody loaded does not swallow what the author wrote. Reading
//! that paragraph back as an ordinary paragraph is render-neutral on every
//! container but one, and that one is why this file exists: `::: figure` with NO
//! title and NO label is a composite figure (§4c), so moving the label off the
//! opener changes the ELEMENT.
//!
//! `::: figure [g]` rendered `<div class="figure">` and came back as
//! `<figure class="carve-figure-group">` - the same document naming two
//! different elements across one round trip (markup-carve/carve-rs#1310).
//!
//! The title has had this lift since it was written and the label never got
//! one. That asymmetry is the whole defect, and closing it also ends a second
//! loss on every container: a label is RAW and a paragraph is not.

use carve::{html_to_carve, to_html, HtmlImportMode, HtmlImportOptions};

fn roundtrip(source: &str) -> String {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    html_to_carve(&to_html(source), &options)
        .expect("import")
        .value
}

#[test]
fn a_figure_keeps_its_element_across_the_round_trip() {
    // THE TICKET'S OWN REPRO, asserted on the ELEMENT rather than on the source:
    // this is the half a source comparison would not explain.
    let source = "::: figure [g]\nBody.\n:::\n";

    assert_eq!(to_html(&roundtrip(source)), to_html(source));
}

#[test]
fn a_figures_bracket_label_comes_back_on_the_opener() {
    assert_eq!(
        roundtrip("::: figure [g]\nBody.\n:::\n"),
        "::: figure [g]\nBody.\n:::\n"
    );
}

#[test]
fn a_figure_that_kept_its_element_did_not_become_a_group() {
    // The precise shape of the flip, so a future change that stabilizes the
    // round trip by making BOTH sides a group still fails here.
    assert!(to_html("::: figure [g]\nBody.\n:::\n").contains("<div class=\"figure\">"));
    assert!(!to_html(&roundtrip("::: figure [g]\nBody.\n:::\n")).contains("carve-figure-group"));
}

#[test]
fn a_quoted_title_still_survives_it() {
    // The control the ticket measured: the title's own lift already worked, and
    // the label's arrival must not disturb it.
    assert_eq!(
        roundtrip("::: figure \"T\"\nBody.\n:::\n"),
        "::: figure \"T\"\nBody.\n:::\n"
    );
}

#[test]
fn a_title_and_a_label_both_come_back() {
    assert_eq!(
        roundtrip("::: figure \"T\" [g]\nBody.\n:::\n"),
        "::: figure \"T\" [g]\nBody.\n:::\n"
    );
}

#[test]
fn an_admonitions_label_comes_back_too() {
    // `::: note [g]` round tripped BEFORE this change, because the label's
    // degradation is render-neutral on a container whose element does not turn
    // on it. It was still losing the SOURCE spelling, which is what moves here.
    assert_eq!(
        roundtrip("::: note [g]\nBody.\n:::\n"),
        "::: note [g]\nBody.\n:::\n"
    );
}

#[test]
fn a_label_comes_back_raw() {
    // THE SECOND LOSS, on every container. A label is a raw run and a paragraph
    // is not, so an asterisk in one came back escaped and the document said
    // something new on each format pass.
    assert_eq!(
        roundtrip("::: figure [a *b*]\nBody.\n:::\n"),
        "::: figure [a *b*]\nBody.\n:::\n"
    );
}

#[test]
fn a_label_shaped_paragraph_further_down_is_not_lifted() {
    // THE NEAR MISS. Unlike the title, which is taken from wherever it stands,
    // the label is taken only from the FIRST element - the renderer writes it
    // there and nowhere else. A paragraph found later would be MOVED to the
    // opener, which changes a document rather than restoring one.
    let source = "::: figure \"T\"\nBody.\n\n{.div-label}\nnot a label\n:::\n";

    assert_eq!(roundtrip(source), source);
}

#[test]
fn a_label_paragraph_holding_markup_is_not_lifted() {
    // The field is a raw `String` and the writer emits it raw, so lifting a
    // paragraph holding markup would flatten it and lose it without a word.
    let html = "<div class=\"figure\"><p class=\"div-label\">a <em>b</em></p><p>Body.</p></div>";
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let written = html_to_carve(html, &options).expect("import").value;

    assert!(!written.starts_with("::: figure ["), "{written}");
    assert!(
        written.contains("/b/"),
        "the emphasis was flattened: {written}"
    );
}

#[test]
fn a_label_holding_a_bracket_is_not_lifted() {
    // Every reader of this run takes it up to the FIRST `]`, with no balance and
    // no escape, so writing `[a]b]` back would not read as a label at all - it
    // would take the opener line with it.
    let html = "<div class=\"figure\"><p class=\"div-label\">a]b</p><p>Body.</p></div>";
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let written = html_to_carve(html, &options).expect("import").value;

    assert!(!written.contains("::: figure ["), "{written}");
    assert_eq!(to_html(&written), to_html(&written), "sanity");
}

#[test]
fn an_attribute_on_the_label_paragraph_is_reported() {
    // The label has no attribute slot, the same way the title has none, so an
    // attribute riding the degraded paragraph is a stated loss rather than a
    // silent one.
    let html = "<div class=\"figure\"><p class=\"div-label\" id=\"x\">g</p><p>Body.</p></div>";
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).expect("import");

    assert!(
        result.value.starts_with("::: figure [g]"),
        "{}",
        result.value
    );
    assert!(
        result.report.diagnostics.iter().any(|d| d.code
            == carve::HtmlImportDiagnosticCode::AttributeDropped
            && d.message.contains("container label")),
        "{:?}",
        result.report.diagnostics
    );
}

#[test]
fn a_plain_divs_label_comes_back_on_the_opener_too() {
    // THE LIFT'S OWN GAP, found while widening the unwrap boundary
    // (markup-carve/carve-rs#1315). The lift lived on the arm that recognizes a
    // CONTAINER CLASS, so `::: figure` and `::: note` got it and a plain `<div>`
    // did not - a div that survived on an attribute still came back with its
    // label as a `{.div-label}` paragraph inside the fence. The two are
    // separate: this one is about a div that never lost its fence.
    assert_eq!(
        roundtrip("{#foo}\n::: [g]\nBody.\n:::\n"),
        "{#foo}\n::: [g]\nBody.\n:::\n"
    );
}

#[test]
fn a_plain_divs_label_comes_back_raw_too() {
    // The raw-run half, on the arm that never had the lift. A paragraph escapes
    // what a label holds verbatim, so this said something new on each pass.
    assert_eq!(
        roundtrip("{#foo}\n::: [a *b*]\nBody.\n:::\n"),
        "{#foo}\n::: [a *b*]\nBody.\n:::\n"
    );
}

#[test]
fn a_plain_div_with_a_label_and_no_attribute_keeps_both_the_fence_and_the_label() {
    // Where the widened boundary and this lift COMPOSE, which is the only place
    // both are needed at once: the boundary is what stops the div unwrapping,
    // and the lift is what puts the label back on the opener it saved.
    assert_eq!(roundtrip("::: [g]\nBody.\n:::\n"), "::: [g]\nBody.\n:::\n");
}
