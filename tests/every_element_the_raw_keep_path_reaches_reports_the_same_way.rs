//! `<form>` is not the bug; the raw-keep path is, and a great many elements with
//! no Carve spelling reach it. This sweeps the whole population rather than the
//! four names one clause happened to list, and pins the two edges of the report:
//! content whose "handler" is escaped text owes no row, and a `style` inside kept
//! bytes gets none yet (markup-carve/carve#2261, markup-carve/carve#2267).

use carve::html_import::{
    html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity,
};

use HtmlImportDiagnosticCode::{AttributePreserved, RawPreserved, StyleUnmapped};
use HtmlImportSeverity::{Error, Info, Warning};

fn rows(html: &str) -> Vec<(HtmlImportDiagnosticCode, HtmlImportSeverity, String)> {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.severity, d.message.clone()))
        .collect()
}

fn written(html: &str) -> String {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    html_to_carve(html, &options).unwrap().value
}

fn codes(html: &str) -> Vec<(HtmlImportDiagnosticCode, HtmlImportSeverity)> {
    rows(html)
        .into_iter()
        .map(|(code, severity, _)| (code, severity))
        .collect()
}

/// A payload live in the kept bytes: a handler and a denied-scheme destination,
/// both on a DESCENDANT of the element being kept.
const INSIDE: &str = r#"<a href="javascript:alert(1)" onclick="y()">t</a>"#;

/// The block-level names with no Carve mapping, which the keep path answers with
/// a raw block.
const BLOCK_ARM: [&str; 4] = ["form", "fieldset", "address", "hgroup"];

/// A sample of the inline arm, which answers with a raw span. It is by far the
/// larger half, which is why a test exercising one element of it would say very
/// little about the population.
const INLINE_ARM: [&str; 10] = [
    "dialog", "output", "progress", "meter", "select", "object", "canvas", "video", "audio", "map",
];

/// Every element the keep path reaches answers the same way, whichever arm of it
/// takes the element.
#[test]
fn fourteen_kept_elements_all_report_their_descendants() {
    for tag in BLOCK_ARM.iter().chain(INLINE_ARM.iter()) {
        let html = format!("<{tag}>{INSIDE}</{tag}>");
        assert_eq!(
            codes(&html),
            vec![
                (RawPreserved, Warning),
                (AttributePreserved, Error),
                (AttributePreserved, Error),
            ],
            "<{tag}>"
        );
        assert!(
            !rows(&html)
                .iter()
                .any(|(code, _, _)| *code == HtmlImportDiagnosticCode::AttributeDropped),
            "<{tag}> reports a drop for an attribute in the kept bytes"
        );
    }
}

/// The four block-level names are a raw BLOCK and the inline ones a raw span, and
/// the sweep above asserts one report for both - so the shapes are pinned here
/// rather than inferred from it.
#[test]
fn the_two_arms_differ_in_shape_and_not_in_report() {
    for tag in BLOCK_ARM {
        let out = written(&format!("<{tag}>t</{tag}>"));
        assert_eq!(out, format!("```=html\n<{tag}>t</{tag}>\n```\n"), "<{tag}>");
    }
    for tag in INLINE_ARM {
        let out = written(&format!("<{tag}>t</{tag}>"));
        assert_eq!(out, format!("`<{tag}>t</{tag}>`{{=html}}\n"), "<{tag}>");
    }
}

/// THE EDGE THAT LOOKS LIKE THE BUG AND IS NOT. A `<textarea>`'s and an
/// `<iframe>`'s content is TEXT, so the kept bytes hold an escaped `<a>` that no
/// parser will ever read as an element and no handler that can fire. There is no
/// refused attribute in there to report, and inventing a row for the escaped text
/// would name a danger that is not present.
#[test]
fn an_escaped_handler_in_kept_text_owes_no_row() {
    for tag in ["textarea", "iframe"] {
        let html = format!("<{tag}>{INSIDE}</{tag}>");
        assert!(
            written(&html).contains("&lt;a href="),
            "<{tag}>: {}",
            written(&html)
        );
        assert_eq!(codes(&html), vec![(RawPreserved, Warning)], "<{tag}>");
    }
}

/// THE BOUNDARY OF THE carve#2261 FIX, pinned so it is visible rather than
/// assumed. Every attribute class routed through the refusal policy reaches the
/// preserved reading by construction. `style` is the one class that is not: it
/// takes its own branch and answers `style-unmapped`, a code that says a CSS
/// mapping was attempted and matched nothing.
///
/// So a dangerous declaration inside kept bytes gets no `error` row, and a
/// descendant's `style` gets no row at all. carve#2267 rules `style-unmapped`
/// wrong inside kept bytes and pins the replacement wording in a clause; this
/// test is what changes when that clause lands.
#[test]
fn a_dangerous_style_inside_kept_bytes_has_no_row_yet() {
    let html = concat!(
        r#"<form style="background:url(javascript:x)" onclick="y()">"#,
        r#"<p style="width:expression(alert(1))">a</p></form>"#
    );
    let report = rows(html);
    // The handler in the same element IS reported, which is what carve#2261 fixed.
    assert!(report
        .iter()
        .any(|(code, severity, _)| *code == AttributePreserved && *severity == Error));
    // Neither style value is named, at any severity.
    let named: Vec<&(HtmlImportDiagnosticCode, HtmlImportSeverity, String)> = report
        .iter()
        .filter(|(_, _, message)| message.contains("style"))
        .collect();
    assert!(named.is_empty(), "{named:?}");
    // The kept element's own style keeps the one row it has today. The
    // descendant's has none, which is the silence carve#2267 describes.
    assert_eq!(
        report
            .iter()
            .filter(|(code, _, _)| *code == StyleUnmapped)
            .count(),
        1,
        "{report:?}"
    );
}

/// Benign CSS inside a kept element reads the same through either arm. The inline
/// arm walks its children before it knows the outcome and used to keep the rows
/// that walk wrote, so a `<p>` inside a kept `<dialog>` was reported as raw-kept
/// itself and its CSS as unmapped, neither of which described the output.
#[test]
fn benign_css_reads_the_same_through_either_arm() {
    let inner = r#"<p style="font-weight:bold">a</p>"#;
    let expected = vec![(StyleUnmapped, Info), (RawPreserved, Warning)];
    assert_eq!(
        codes(&format!(r#"<form style="color:red">{inner}</form>"#)),
        expected
    );
    assert_eq!(
        codes(&format!(r#"<dialog style="color:red">{inner}</dialog>"#)),
        expected
    );
}
