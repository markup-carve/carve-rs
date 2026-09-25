//! Everything inside an element `roundtrip` keeps as raw HTML is in the kept
//! bytes, so every refused attribute in there is reported as
//! `attribute-preserved`, never `attribute-dropped` (markup-carve/carve#2261).

use carve::html_import::{
    html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity,
};

use HtmlImportDiagnosticCode::{AttributePreserved, RawPreserved};
use HtmlImportSeverity::{Error, Info, Warning};

type Row = (HtmlImportDiagnosticCode, HtmlImportSeverity, String, String);

fn import(html: &str, mode: HtmlImportMode) -> (String, Vec<Row>) {
    let options = HtmlImportOptions {
        mode,
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

const RULING_INPUT: &str = r#"<p>x</p><form onclick="a()" action="javascript:b()"><a href="javascript:alert(1)" onclick="y()">t</a></form>"#;

#[test]
fn the_ruling_example_reports_every_live_attribute_in_the_kept_bytes() {
    let (value, rows) = import(RULING_INPUT, HtmlImportMode::Roundtrip);
    assert_eq!(
        value,
        "x\n\n```=html\n<form onclick=\"a()\" action=\"javascript:b()\"><a href=\"javascript:alert(1)\" onclick=\"y()\">t</a></form>\n```\n"
    );
    assert_eq!(
        rows,
        vec![
            row(AttributePreserved, Error, "/form[2]", "Preserved event-handler attribute onclick on <form> in the raw HTML this element is kept as"),
            row(AttributePreserved, Error, "/form[2]", "Preserved action with a denied URL scheme on <form> in the raw HTML this element is kept as"),
            row(RawPreserved, Warning, "/form[2]", "Preserved unsupported <form> element as raw HTML"),
            row(AttributePreserved, Error, "/form[2]/a[1]", "Preserved href with a denied URL scheme on <a> inside the raw HTML <form> is kept as"),
            row(AttributePreserved, Error, "/form[2]/a[1]", "Preserved event-handler attribute onclick on <a> inside the raw HTML <form> is kept as"),
        ]
    );
}

#[test]
fn a_nested_descendant_gets_the_same_treatment_and_no_row_of_its_own_kind() {
    let (value, rows) = import(
        r#"<form><div id="d" 5x="1"><img src="data:text/html,x" alt="a"></div></form>"#,
        HtmlImportMode::Roundtrip,
    );
    assert!(value.starts_with("```=html\n<form>"), "{value}");
    assert_eq!(
        rows,
        vec![
            row(RawPreserved, Warning, "/form[1]", "Preserved unsupported <form> element as raw HTML"),
            row(AttributePreserved, Info, "/form[1]/div[1]", "Preserved attribute 5x on <div> inside the raw HTML <form> is kept as: not a Carve attribute name"),
            row(AttributePreserved, Error, "/form[1]/div[1]/img[1]", "Preserved src with a denied URL scheme on <img> inside the raw HTML <form> is kept as"),
        ]
    );
}

#[test]
fn a_descendant_row_carries_the_fidelity_of_an_own_row() {
    let result = html_to_carve(
        RULING_INPUT,
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .unwrap();
    let diagnostics = &result.report.diagnostics;
    assert_eq!(diagnostics[3].fidelity, diagnostics[0].fidelity);
    assert_eq!(diagnostics[3].confidence, diagnostics[0].confidence);
}

#[test]
fn an_inline_kept_element_stays_an_inline_raw_span_and_reports_its_descendants() {
    let (value, rows) = import(
        r#"<p>x<button onclick="a()"><b onclick="c()">t</b></button>y</p>"#,
        HtmlImportMode::Roundtrip,
    );
    assert_eq!(
        value,
        "x`<button onclick=\"a()\"><b onclick=\"c()\">t</b></button>`{=html}y\n"
    );
    assert_eq!(
        rows,
        vec![
            row(AttributePreserved, Error, "/p[1]/button[2]", "Preserved event-handler attribute onclick on <button> in the raw HTML this element is kept as"),
            row(RawPreserved, Warning, "/p[1]/button[2]", "Preserved unsupported <button> element as raw HTML"),
            row(AttributePreserved, Error, "/p[1]/button[2]/b[1]", "Preserved event-handler attribute onclick on <b> inside the raw HTML <button> is kept as"),
        ]
    );
}

#[test]
fn every_block_level_kept_element_is_a_raw_block() {
    for tag in ["address", "fieldset", "form", "hgroup"] {
        let (value, _) = import(
            &format!("<p>x</p><{tag}>y</{tag}>"),
            HtmlImportMode::Roundtrip,
        );
        assert_eq!(value, format!("x\n\n```=html\n<{tag}>y</{tag}>\n```\n"));
    }
}

#[test]
fn safe_and_semantic_modes_are_unchanged() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let (value, rows) = import(RULING_INPUT, mode);
        assert_eq!(value, "x\n\nt\n");
        assert_eq!(
            rows,
            vec![
                row(HtmlImportDiagnosticCode::AttributeDropped, Warning, "/form[2]", "Dropped event-handler attribute onclick on <form>"),
                row(HtmlImportDiagnosticCode::ElementUnwrapped, Info, "/form[2]", "Unwrapped unsupported <form> element"),
                row(HtmlImportDiagnosticCode::AttributeDropped, Info, "/form[2]", "Dropped action on <form>: the element was unwrapped and has no node to carry it"),
                row(HtmlImportDiagnosticCode::AttributeDropped, Warning, "/form[2]/a[1]", "Dropped event-handler attribute onclick on <a>"),
                row(HtmlImportDiagnosticCode::AttributeDropped, Warning, "/form[2]/a[1]", "Dropped href with a denied URL scheme on <a>"),
            ]
        );
    }
}

fn capped(html: &str) -> Vec<HtmlImportDiagnosticCode> {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        max_diagnostics: 1,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    result.report.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn rows_taken_back_do_not_leave_a_truncation_marker_behind() {
    assert_eq!(
        capped("<p><button><foo></foo><bar></bar></button></p>"),
        vec![RawPreserved]
    );
}

#[test]
fn a_row_turned_away_outside_the_kept_element_keeps_the_marker() {
    assert_eq!(
        capped("<p><qux></qux><button><foo></foo><bar></bar></button></p>"),
        vec![HtmlImportDiagnosticCode::DiagnosticsTruncated]
    );
}

#[test]
fn a_denied_token_later_in_a_url_list_is_live() {
    let (_, rows) = import(
        r#"<form><a href="/x" ping="https://safe.test javascript:alert(1)">t</a></form>"#,
        HtmlImportMode::Roundtrip,
    );
    assert_eq!(
        rows[1],
        row(
            AttributePreserved,
            Error,
            "/form[1]/a[1]",
            "Preserved ping with a denied URL scheme on <a> inside the raw HTML <form> is kept as"
        )
    );
}
