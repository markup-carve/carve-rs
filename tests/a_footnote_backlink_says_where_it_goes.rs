//! PART 9 §16 + §16a (markup-carve/carve#1455, markup-carve/carve#1456).
//!
//! `role="doc-backlink"` was already right and the accessible NAME was the `↩`
//! glyph, so a screen reader announced "leftwards arrow with hook" or skipped
//! the link: correct semantics, no way to know where it goes.

use carve::Options;

fn html(source: &str) -> String {
    carve::to_html_with_options(source, &Options::new())
}

#[test]
fn a_lone_backlink_is_named_by_the_label_alone() {
    assert!(html("Text[^a]\n\n[^a]: Note body.\n").contains(
        "<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a>"
    ));
}

#[test]
fn the_kth_of_several_takes_what_it_visibly_says() {
    // The number is the REFERENCE ORDINAL, matching the visible `<sup>k</sup>`
    // (WCAG 2.5.3). The note number appears nowhere in this link's text.
    let out = html("See[^a] and again[^a].\n\n[^a]: One note, two refs.\n");

    assert!(out.contains(
        "<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference 1\">\u{21a9}<sup>1</sup></a>"
    ), "{out}");
    assert!(out.contains(
        "<a href=\"#fnref1-2\" role=\"doc-backlink\" aria-label=\"Back to reference 2\">\u{21a9}<sup>2</sup></a>"
    ), "{out}");
}

#[test]
fn the_labels_option_carries_the_string() {
    let opts = Options::new().with_label("footnoteBacklink", "Zurück zur Fußnote");
    let out = carve::to_html_with_options("Text[^a]\n\n[^a]: n\n", &opts);

    assert!(out.contains("aria-label=\"Zurück zur Fußnote\""), "{out}");
    assert!(!out.contains("Back to reference"), "{out}");
}

#[test]
fn the_label_is_escaped_rather_than_emitted_raw() {
    // A label is TEXT, unlike a symbols-map value: a host reading its strings
    // from a translation catalog must not be handing the renderer an injection
    // vector.
    let opts = Options::new().with_label("footnoteBacklink", "\"><script>alert(1)</script>");
    let out = carve::to_html_with_options("Text[^a]\n\n[^a]: n\n", &opts);

    assert!(!out.contains("<script>"), "{out}");
    assert!(out.contains("&quot;&gt;&lt;script&gt;"), "{out}");
}

#[test]
fn the_english_default_stands_when_no_label_is_given() {
    assert_eq!(
        carve::label_default(carve::LABEL_FOOTNOTE_BACKLINK),
        "Back to reference"
    );
}
