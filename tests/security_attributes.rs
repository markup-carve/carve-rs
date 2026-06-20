//! Attribute XSS hardening: dangerous attribute names and values are stripped
//! from ALL rendered attributes, unconditionally. This is core renderer
//! behavior, so every element that carries `{...}` attributes is covered
//! (spans, divs, headings, list-table cells, ...).

use carve::{ListTable, Options};

fn h(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn strips_event_handler_attributes() {
    assert_eq!(h("[x]{onclick=\"alert(1)\"}"), "<p><span>x</span></p>");
    assert_eq!(
        h("[x]{onmouseover=\"x\" class=\"c\"}"),
        "<p><span class=\"c\">x</span></p>"
    );
}

#[test]
fn strips_srcdoc_and_formaction() {
    assert_eq!(
        h("[x]{srcdoc=\"<script>\" formaction=\"y\" title=\"ok\"}"),
        "<p><span title=\"ok\">x</span></p>"
    );
}

#[test]
fn blanks_dangerous_scheme_values() {
    assert_eq!(
        h("[x]{background=\"javascript:alert(1)\"}"),
        "<p><span background=\"\">x</span></p>"
    );
}

#[test]
fn defeats_scheme_obfuscation() {
    assert_eq!(
        h("[x]{background=\"java\tscript:alert(1)\"}"),
        "<p><span background=\"\">x</span></p>"
    );
}

#[test]
fn blanks_css_expression_but_keeps_plain_style() {
    assert_eq!(
        h("[x]{style=\"x:expression(alert(1))\"}"),
        "<p><span style=\"\">x</span></p>"
    );
    assert_eq!(
        h("[x]{style=\"color:red\"}"),
        "<p><span style=\"color:red\">x</span></p>"
    );
}

#[test]
fn keeps_safe_attributes() {
    assert_eq!(
        h("[x]{title=\"hello\" data-id=\"42\" class=\"a b\"}"),
        "<p><span title=\"hello\" data-id=\"42\" class=\"a b\">x</span></p>"
    );
}

#[test]
fn applies_to_list_table_cells() {
    let ext = ListTable::new();
    let opts = Options::new().with_extension(&ext);
    let out =
        carve::to_html_with_options("::: list-table\n- -{onclick=\"x\"} A\n  - B\n:::", &opts);
    assert!(out.contains("<td>A</td>"));
    assert!(!out.contains("onclick"));
}

#[test]
fn over_large_list_table_defers_to_plain_div() {
    let ext = ListTable::new();
    let opts = Options::new().with_extension(&ext);
    let rows = "- - a\n  - b\n".repeat(10_001);
    let src = format!("::: list-table\n{rows}:::");
    let out = carve::to_html_with_options(&src, &opts);
    assert!(out.starts_with("<div class=\"list-table\">"));
    assert!(!out.contains("<table>"));
}
