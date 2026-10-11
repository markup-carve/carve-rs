//! The import half of the PART 10 §13a carrier inventory
//! (markup-carve/carve#2894).
//!
//! The export half landed first (carve-rs#2471), so every case here is a REAL
//! ROUND TRIP - Carve, export with the carriers on, import, compare - rather
//! than an assertion against hand-written HTML. A hand-written fixture can only
//! prove the importer reads a string the test itself wrote; the round trip
//! proves the two halves agree, which is the acceptance criterion #2894 states.
//!
//! Each case is paired with a CONTROL exported WITHOUT the carriers. Without it
//! a test cannot tell a carrier being read from an import that happened to be
//! right anyway: a passing round trip is only evidence when the same document
//! fails to round trip once the carrier is gone.

use carve::{
    html_to_carve, to_html, to_html_with_options, HtmlImportMode, HtmlImportOptions, Options,
};

fn export(source: &str, carriers: bool) -> String {
    to_html_with_options(source, &Options::default().with_roundtrip(carriers))
}

fn import(html: &str) -> String {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    html_to_carve(html, &options).unwrap().value
}

/// The round trip with the carriers on must reproduce the source, and the same
/// round trip with them off must NOT - or the carrier is carrying nothing.
fn carrier_is_load_bearing(source: &str) {
    assert_eq!(
        import(&export(source, true)),
        source,
        "the carrier round trip did not reproduce the source"
    );
    assert_ne!(
        import(&export(source, false)),
        source,
        "the source round trips WITHOUT the carrier, so this case proves nothing"
    );
}

#[test]
fn an_ordered_lists_marker_carrier_restores_its_delimiter() {
    // §13a lists `data-marker` on `<ul>` AND `<ol>`. carve-php and carve-js
    // both write it on an ordered list beside §12's `data-delim`; this engine's
    // writer leaves the delimiter to §12 alone, so the carrier arrives only in
    // the other two engines' documents - which is exactly why the READER has to
    // take it. The control is a hand-written element for that reason.
    let html = r#"<ol data-marker=")"><li>a</li></ol>"#;
    assert_eq!(import(html), "1) a\n");
    // The deprecated spelling does not apply: `data-marker` carries no prefix
    // and so was never renamed (carve#2902 ruling 3).
    assert_eq!(import(r#"<ol data-marker="."><li>a</li></ol>"#), "1. a\n");
}

#[test]
fn an_ordered_marker_carrier_is_never_written_back_as_an_attribute() {
    // Ruling 5: a reader consumes the inventory and never returns a reserved
    // name as the author's content. Before `data-marker` was read on an `<ol>`
    // it fell through to the attribute policy, which refuses a carrier - so
    // this guards the consumption, not the refusal.
    let back = import(r#"<ol data-marker=")"><li>a</li></ol>"#);
    assert!(
        !back.contains("data-marker"),
        "a reserved carrier name came back as content: {back:?}"
    );
}

#[test]
fn a_footnote_label_carrier_restores_the_authored_label() {
    // Labels are assigned 1..N over document order, because an id is generated
    // navigation. So `[^note]` came back as `[^1]` - silently, since both
    // render the same `<sup>1</sup>` - and two documents differing only in
    // their labels imported identically.
    carrier_is_load_bearing("a[^note]\n\n[^note]: n\n");
}

#[test]
fn a_malformed_footnote_label_falls_back_to_the_ordinal() {
    // A label has to be spellable inside `[^...]`. A value holding a space or a
    // bracket would not reparse, and this serializer does not write one, so it
    // is malformed input: the ordinal stands and the import does not fail.
    for bad in ["a b", "a[b", "a]b", ""] {
        let html = format!(
            r##"<p>a<a id="fnref1" href="#fn1" role="doc-noteref" data-carve-footnote-label="{bad}"><sup>1</sup></a></p>"##
        ) + r##"<section role="doc-endnotes"><ol><li id="fn1"><p>n</p></li></ol></section>"##;
        let back = import(&html);
        assert!(
            back.contains("[^1]"),
            "a malformed label {bad:?} was taken as the label: {back:?}"
        );
    }
}

#[test]
fn two_endnotes_cannot_be_merged_by_a_repeated_label() {
    // A repeated label would fold two notes into one and lose a definition. The
    // second note keeps its ordinal rather than colliding.
    let html = concat!(
        r##"<p>a<a id="fnref1" href="#fn1" role="doc-noteref" data-carve-footnote-label="n"><sup>1</sup></a>"##,
        r##"b<a id="fnref2" href="#fn2" role="doc-noteref" data-carve-footnote-label="n"><sup>2</sup></a></p>"##,
        r##"<section role="doc-endnotes"><ol><li id="fn1"><p>one</p></li><li id="fn2"><p>two</p></li></ol></section>"##,
    );
    let back = import(html);
    assert!(
        back.contains("[^n]"),
        "the first label was not taken: {back:?}"
    );
    assert!(
        back.contains("[^2]"),
        "the second note did not keep an ordinal of its own: {back:?}"
    );
}

#[test]
fn the_deprecated_spelling_is_read_for_a_prefixed_carrier() {
    // Documents released carve-php versions exported carry `data-djot-`
    // (carve#2902 ruling 0). The alias has to buy the SAME import, so this is a
    // differential against the canonical spelling rather than a literal.
    let canonical = export("a[^note]\n\n[^note]: n\n", true);
    let legacy = canonical.replace("data-carve-", "data-djot-");
    assert_ne!(canonical, legacy, "the fixture carries no prefixed carrier");
    assert_eq!(import(&legacy), import(&canonical));
}

#[test]
fn the_default_export_is_unchanged_by_this_work() {
    // §13 keeps the option OFF by default on every surface, and #2894 says the
    // default export is unchanged by the follow-up.
    for source in ["{#H}\n# H\n", "a[^note]\n\n[^note]: n\n", "1) a\n"] {
        assert_eq!(to_html(source), export(source, false));
        assert!(!to_html(source).contains("data-carve-"));
    }
}
