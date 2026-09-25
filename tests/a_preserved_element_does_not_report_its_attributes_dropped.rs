//! WHAT THE OUTPUT KEEPS IS NOT A LOSS (markup-carve/carve-js#1468).
//!
//! `roundtrip` hands some elements back verbatim as raw HTML. `attrs` reads
//! their attributes on the way past, long before any arm decides that, so
//! every attribute the policy refuses was reported `attribute-dropped` while
//! the preserved bytes carried it into the output. The report made a FALSE
//! claim, and it made it about the one attribute a consumer of this mode would
//! act on: `docs/html-import.md` calls `roundtrip` unsafe for untrusted input,
//! so a live `onclick` in the output is the row that matters.
//!
//! ROLLING THE ROWS BACK WOULD HAVE BEEN THE SAME DEFECT POINTED THE OTHER
//! WAY. It trades a false statement for a missing security-relevant fact, and
//! it does that silently. So the rows stay and stop claiming a drop:
//! `attribute-preserved` says the element was kept WITH the attribute on it,
//! and it is a code of its own because a consumer that filters on
//! `attribute-dropped` rather than reading the prose would still be told a
//! drop happened.
//!
//! SEVERITY IS NOT COPIED FROM THE DROP. A dropped handler is a `Warning`; a
//! SURVIVING one is a stronger signal, so it is `Error` - the only level left
//! that separates the two for a filter. An attribute refused for a reason that
//! is not safety rides along harmlessly and is `Info`.
//!
//! Every assertion reads the CODES. A test that only pinned messages would
//! pass on a report that still said `attribute-dropped`.

use carve::html_import::{
    html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity,
};

fn roundtrip(
    html: &str,
) -> (
    String,
    Vec<(HtmlImportDiagnosticCode, HtmlImportSeverity, String)>,
) {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.severity, d.message.clone()))
        .collect();
    (result.value, rows)
}

fn codes(
    rows: &[(HtmlImportDiagnosticCode, HtmlImportSeverity, String)],
) -> Vec<(HtmlImportDiagnosticCode, HtmlImportSeverity)> {
    rows.iter().map(|(c, s, _)| (*c, *s)).collect()
}

#[test]
fn a_preserved_event_handler_is_reported_as_preserved_and_louder_than_a_drop() {
    let (value, rows) = roundtrip(r#"<form onclick="x()" id="q">a</form>"#);
    assert!(value.contains(r#"onclick="x()""#), "{value}");
    assert_eq!(
        codes(&rows),
        vec![
            (
                HtmlImportDiagnosticCode::AttributePreserved,
                HtmlImportSeverity::Error
            ),
            (
                HtmlImportDiagnosticCode::RawPreserved,
                HtmlImportSeverity::Warning
            ),
        ]
    );
    assert_eq!(
        rows[0].2,
        "Preserved event-handler attribute onclick on <form> in the raw HTML this element is kept as"
    );
}

#[test]
fn a_reason_that_is_not_safety_is_preserved_at_info() {
    // A name Carve cannot spell: refused for shape rather than for safety,
    // kept by the preserved bytes all the same, and no louder than `Info`.
    let (value, rows) = roundtrip(r#"<form 5x="1">a</form>"#);
    assert!(value.contains(r#"5x="1""#), "{value}");
    assert_eq!(
        codes(&rows),
        vec![
            (
                HtmlImportDiagnosticCode::AttributePreserved,
                HtmlImportSeverity::Info
            ),
            (
                HtmlImportDiagnosticCode::RawPreserved,
                HtmlImportSeverity::Warning
            ),
        ]
    );
    assert_eq!(
        rows[0].2,
        "Preserved attribute 5x on <form> in the raw HTML this element is kept as: not a Carve attribute name"
    );
}

#[test]
fn several_refused_attributes_on_one_element_keep_their_own_severities() {
    // THE GENERAL CASE, and why one `onclick` on one `<form>` is not a test.
    // Two handlers, one unspellable name, and two attributes refused by no rule
    // at all - `id` and `data-k` take no row, in either direction.
    let (value, rows) = roundtrip(
        r#"<fieldset id="f" onmouseover="y()" 9bad="1" onfocus="z()" data-k="v">a</fieldset>"#,
    );
    for attribute in [
        r#"id="f""#,
        r#"onmouseover="y()""#,
        r#"9bad="1""#,
        r#"onfocus="z()""#,
        r#"data-k="v""#,
    ] {
        assert!(
            value.contains(attribute),
            "{attribute} missing from {value}"
        );
    }
    assert_eq!(
        codes(&rows),
        vec![
            (
                HtmlImportDiagnosticCode::AttributePreserved,
                HtmlImportSeverity::Error
            ),
            (
                HtmlImportDiagnosticCode::AttributePreserved,
                HtmlImportSeverity::Info
            ),
            (
                HtmlImportDiagnosticCode::AttributePreserved,
                HtmlImportSeverity::Error
            ),
            (
                HtmlImportDiagnosticCode::RawPreserved,
                HtmlImportSeverity::Warning
            ),
        ]
    );
}

#[test]
fn the_inline_preserve_arm_answers_the_same_way() {
    // A different arm of the same mode, reporting an injection sink
    // rather than an event handler. `srcdoc` is not an `on*` name and is
    // refused by the same renderer filter, so it is the same fact.
    let (value, rows) = roundtrip(r#"<iframe srcdoc="<p>x</p>" onload="z()"></iframe>"#);
    assert!(value.contains("srcdoc="), "{value}");
    assert_eq!(rows[0].0, HtmlImportDiagnosticCode::AttributePreserved);
    assert_eq!(rows[0].1, HtmlImportSeverity::Error);
    assert_eq!(
        rows[0].2,
        "Preserved injection-sink attribute srcdoc on <iframe> in the raw HTML this element is kept as"
    );
}

#[test]
fn the_figure_preserve_arm_answers_the_same_way() {
    // markup-carve/carve#1704's arm, whose own rationale named this defect and
    // left the element's rows alone so no arm would disagree with its
    // neighbours while it was open. It agrees with them now.
    let (value, rows) = roundtrip(
        r#"<figure onclick="c()" id="g"><ul><li>a</li></ul><figcaption>Cap</figcaption></figure>"#,
    );
    assert!(value.contains(r#"onclick="c()""#), "{value}");
    assert_eq!(
        codes(&rows),
        vec![
            (
                HtmlImportDiagnosticCode::AttributePreserved,
                HtmlImportSeverity::Error
            ),
            (
                HtmlImportDiagnosticCode::RawPreserved,
                HtmlImportSeverity::Warning
            ),
        ]
    );
}

#[test]
fn an_element_that_is_not_preserved_still_reports_a_drop() {
    // THE OTHER HALF, and what keeps this from being a blanket rename. The
    // same `<form onclick>` outside `roundtrip` really does lose the handler,
    // so the row that says so has to survive untouched.
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Semantic,
        ..Default::default()
    };
    let result = html_to_carve(r#"<form onclick="x()" id="q">a</form>"#, &options).unwrap();
    assert!(!result.value.contains("onclick"), "{}", result.value);
    assert_eq!(
        result.report.diagnostics[0].code,
        HtmlImportDiagnosticCode::AttributeDropped
    );
    assert_eq!(
        result.report.diagnostics[0].severity,
        HtmlImportSeverity::Warning
    );
    assert_eq!(
        result.report.diagnostics[0].message,
        "Dropped event-handler attribute onclick on <form>"
    );
}

#[test]
fn the_rewrite_reaches_the_preserved_element_and_nothing_else() {
    // THE REWRITE IS SCOPED TO THE ELEMENT THE ARM DECIDED ABOUT.
    // `preserve_own_attributes` matches on the NODE rather than on the path, so
    // a paragraph that really does lose its handler goes on saying so while its
    // `<form>` neighbour says the opposite in the same report.
    let (value, rows) = roundtrip(r#"<p onclick="a()">x</p><form onclick="b()">y</form>"#);
    assert!(!value.contains(r#"onclick="a()""#), "{value}");
    assert!(value.contains(r#"onclick="b()""#), "{value}");
    assert_eq!(
        rows.iter().map(|(c, _, _)| *c).collect::<Vec<_>>(),
        vec![
            HtmlImportDiagnosticCode::AttributeDropped,
            HtmlImportDiagnosticCode::AttributePreserved,
            HtmlImportDiagnosticCode::RawPreserved,
        ]
    );
}

#[test]
fn the_reports_subject_vocabulary_is_the_one_the_other_engines_spell() {
    // THE SUBJECT NOUNS ARE A CROSS-ENGINE CONTRACT, so this pins the whole row
    // set for one input rather than the one word that moved. carve-js
    // (`src/html-import.ts`, `isDangerousAttrName` arm) and carve-php
    // (`HtmlToCarve::reportPreservedAttribute`) both say `injection-sink` for a
    // sink that is not an `on*` handler, `docs/security.md` calls `srcdoc` and
    // `formaction` sinks, and this engine said `active-content` - a noun no
    // other engine and no part of the spec uses (carve-rs#1881).
    let (value, rows) = roundtrip(
        r#"<form onclick="go()" action="javascript:alert(1)"><button formaction="javascript:alert(3)">go</button></form>"#,
    );
    assert!(
        value.contains(r#"formaction="javascript:alert(3)""#),
        "{value}"
    );
    let messages: Vec<&str> = rows.iter().map(|(_, _, m)| m.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "Preserved event-handler attribute onclick on <form> in the raw HTML this element is kept as",
            "Preserved action with a denied URL scheme on <form> in the raw HTML this element is kept as",
            "Preserved unsupported <form> element as raw HTML",
            "Preserved injection-sink attribute formaction on <button> inside the raw HTML <form> is kept as",
        ]
    );
    assert!(
        !messages.iter().any(|m| m.contains("active-content")),
        "no row may name a sink with a noun the spec and the other engines do not use: {messages:?}"
    );
}
