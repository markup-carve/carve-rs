//! Attribute XSS hardening: dangerous attribute names and values are stripped
//! from ALL rendered attributes, unconditionally. This is core renderer
//! behavior, so every element that carries `{...}` attributes is covered
//! (spans, divs, headings, list-table cells, ...).

use carve::{ListTable, Options};
use std::collections::BTreeMap;

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
fn defeats_unicode_whitespace_before_scheme() {
    // Finding 5: whitespace before the scheme must not let `javascript:`
    // through. Two different mechanisms now stop it, and which one applies
    // depends on whether the prefix is whitespace at all.

    // WHITESPACE ends a destination (PART 9 `link_destination`), so there is no
    // link and therefore no attribute to smuggle a scheme into. A stronger
    // outcome than blanking, and the assertion has to be about the ATTRIBUTE
    // rather than the string: the text survives as inert, escaped prose, so
    // `javascript:` still appears in the output with nothing executable around
    // it (carve#404, carve#407).
    for ws in ["\u{00A0}", "\u{2028}", "\u{2029}"] {
        let html = carve::to_html(&format!("[x]({ws}javascript:alert(1))"));
        assert!(
            !html.contains("<a"),
            "formed a link for prefix {ws:?}: {html}"
        );
        assert!(
            !html.contains("href"),
            "emitted an href for prefix {ws:?}: {html}"
        );
    }

    // A BOM is NOT whitespace (it has no Unicode White_Space property), so it
    // is an ordinary destination character and the link DOES form - which is
    // exactly why the scheme probe still has to strip it and blank the href.
    let bom = carve::to_html("[x](\u{FEFF}javascript:alert(1))");
    assert!(
        bom.contains("href=\"\""),
        "expected blanked href, got {bom}"
    );

    // A safe scheme behind whitespace is not a link either - the rule is about
    // the destination, not about the scheme.
    let safe = carve::to_html("[x](\u{00A0}https://e.com)");
    assert!(!safe.contains("<a"), "{safe}");
    // ... and an ordinary safe URL still links.
    assert!(carve::to_html("[x](https://e.com)").contains("href=\"https://e.com\""));
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

// Safe-by-default v2: URL denylist (always on), raw-HTML opt-out, CSS hardening.

#[test]
fn url_scheme_denylist_blanks_dangerous_links() {
    assert!(carve::to_html("[x](javascript:alert(1))").contains("href=\"\""));
    assert!(carve::to_html("![i](javascript:alert(1))").contains("src=\"\""));
    assert!(carve::to_html("[x](data:text/html,foo)").contains("href=\"\""));
}

#[test]
fn url_scheme_denylist_passes_ordinary_schemes() {
    assert!(carve::to_html("[x](https://e.com)").contains("href=\"https://e.com\""));
    assert!(carve::to_html("[c](tel:+1)").contains("href=\"tel:+1\""));
    assert!(carve::to_html("[r](/p)").contains("href=\"/p\""));
}

#[test]
fn raw_html_emitted_by_default() {
    assert_eq!(
        carve::to_html("`<b>x</b>`{=html}").trim(),
        "<p><b>x</b></p>"
    );
}

#[test]
fn raw_html_escaped_when_disabled() {
    let off = carve::Options::new().with_raw_html(false);
    assert_eq!(
        carve::to_html_with_options("`<img onerror=alert(1)>`{=html}", &off).trim(),
        "<p>&lt;img onerror=alert(1)&gt;</p>"
    );
    assert_eq!(
        carve::to_html_with_options("```=html\n<img onerror=x>\n```", &off).trim(),
        "&lt;img onerror=x&gt;"
    );
}

#[test]
fn css_style_hardening() {
    assert!(carve::to_html("[x]{style=\"background:url(javascript:1)\"}").contains("style=\"\""));
    assert!(carve::to_html("[x]{style=\"@import url(evil.css)\"}").contains("style=\"\""));
    assert!(carve::to_html("[x]{style=\"color:red\"}").contains("style=\"color:red\""));
}

#[test]
fn css_style_hardening_decodes_css_escapes() {
    assert_eq!(
        h("[x]{style=\"background:u\\72l(http://e/p)\"}"),
        "<p><span style=\"\">x</span></p>"
    );
}

#[test]
fn drops_invalid_attribute_names_before_rendering() {
    let mut key_values = BTreeMap::new();
    key_values.insert("bad name".to_string(), "x".to_string());
    key_values.insert("=breakout".to_string(), "x".to_string());
    key_values.insert("data-ok".to_string(), "1".to_string());
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        source_len: 0,
        footnote_defs: BTreeMap::new(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            at_content_column: false,
            attrs: Some(carve::Attrs {
                id: None,
                classes: Vec::new(),
                key_values,
                order: vec![
                    carve::AttrSlot::Key("bad name".to_string()),
                    carve::AttrSlot::Key("=breakout".to_string()),
                    carve::AttrSlot::Key("data-ok".to_string()),
                ],
            }),
            children: vec![carve::InlineNode::Text("x".to_string())],
        })],
    };

    assert_eq!(carve::render_html(&doc).trim(), "<p data-ok=\"1\">x</p>");
}
