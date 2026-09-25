//! The report row for an attribute refused because of the shape of its NAME is
//! RULED (carve-rs#1882), not inferred from a description: all three engines
//! had picked a different half of it, and markup-carve/carve#2271 compares these
//! rows across the engines byte for byte.
//!
//! The row is pinned beside the two patterns the ruling read it off, so a later
//! edit cannot move it to match a neighbour that was never the question:
//! `unsupported` is this report's word for "Carve has no spelling for this", and
//! a colon plus a reason is how it explains a refusal.

use carve::html_import::{
    html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity,
};

use HtmlImportDiagnosticCode::{AttributePreserved, RawPreserved};
use HtmlImportSeverity::{Info, Warning};

type Row = (HtmlImportDiagnosticCode, HtmlImportSeverity, String, String);

/// `xlink:href` on imported SVG is the ordinary way to reach this row, so it is
/// the probe. The three attributes beside it are the CONTROLS: each already
/// carried its wording before the ruling and none of them may move.
const PROBE: &str = r#"<form><a xlink:href="q">l</a><div style="text-align:right" align="left">a</div><cite cite="https://x">t</cite></form>"#;

fn rows(html: &str) -> (String, Vec<Row>) {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code,
                d.severity,
                d.path.clone().unwrap_or_default(),
                d.message.clone(),
            )
        })
        .collect();
    (result.value, rows)
}

fn row(
    code: HtmlImportDiagnosticCode,
    severity: HtmlImportSeverity,
    path: &str,
    message: &str,
) -> Row {
    (code, severity, path.into(), message.into())
}

#[test]
fn the_row_reads_unsupported_attribute_and_names_the_rule_that_refused_it() {
    let (value, reported) = rows(PROBE);
    // The attribute is in the kept bytes, which is why the row says preserved.
    assert!(value.contains(r#"xlink:href="q""#), "{value}");
    assert_eq!(
        reported,
        vec![
            row(
                RawPreserved,
                Warning,
                "/form[1]",
                "Preserved unsupported <form> element as raw HTML"
            ),
            row(
                AttributePreserved,
                Info,
                "/form[1]/a[1]",
                "Preserved unsupported attribute xlink:href on <a> inside the raw HTML <form> is kept as: not spellable as a Carve attribute name"
            ),
            // The `style` beside it takes a row of its own now that kept bytes
            // read it through the refusal policy (markup-carve/carve#2267). It
            // does not move the `align` row: both are spelling-ordered.
            row(
                AttributePreserved,
                Info,
                "/form[1]/div[2]",
                "Preserved style on <div> inside the raw HTML <form> is kept as"
            ),
            row(
                AttributePreserved,
                Info,
                "/form[1]/div[2]",
                "Preserved attribute align on <div> inside the raw HTML <form> is kept as: a mapped CSS declaration already sets it"
            ),
            row(
                AttributePreserved,
                Info,
                "/form[1]/cite[3]",
                "Preserved attribute cite on <cite> inside the raw HTML <form> is kept as: the semantic span's marker owns that key"
            ),
        ]
    );
}

#[test]
fn the_name_is_what_refuses_it_and_a_spellable_name_beside_it_takes_no_row() {
    // The rule is about the NAME, so an attribute Carve can spell rides along
    // with no row at all - without this the test above would pass on a report
    // that named every attribute on the element.
    let (_, reported) = rows(r#"<form><a xlink:href="q" data-k="v">l</a></form>"#);
    let messages: Vec<&str> = reported.iter().map(|r| r.3.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "Preserved unsupported <form> element as raw HTML",
            "Preserved unsupported attribute xlink:href on <a> inside the raw HTML <form> is kept as: not spellable as a Carve attribute name",
        ]
    );
}
