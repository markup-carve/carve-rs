//! Non-HTML render targets are safe-by-default: Markdown cannot carry XSS into a
//! downstream Markdown -> HTML render, and ANSI/plain cannot inject terminal
//! escape sequences.

use std::collections::BTreeMap;

fn md(src: &str) -> String {
    carve::to_markdown(src).trim().to_string()
}

#[test]
fn markdown_blanks_dangerous_url_schemes() {
    assert!(md("[x](javascript:alert(1))").contains("[x]()"));
    assert!(md("![a](javascript:alert(1))").contains("![a]()"));
    assert!(md("[ok](https://e.com)").contains("[ok](https://e.com)"));
    assert_eq!(md("<javascript:alert(1)>"), "[javascript:alert(1)]()");
}

#[test]
fn markdown_percent_encodes_destination_breakouts() {
    assert_eq!(
        md("[x](https://e.com/a(b)c)"),
        "[x](https://e.com/a%28b%29c)"
    );
    let image_doc = carve::Document {
        frontmatter: BTreeMap::new(),
        footnote_defs: BTreeMap::new(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            attrs: None,
            children: vec![carve::InlineNode::Image(carve::Image {
                attrs: None,
                src: "https://e.com/a b<c>".to_string(),
                alt: "x".to_string(),
                title: None,
            })],
        })],
    };
    assert_eq!(
        carve::render_markdown(&image_doc).trim(),
        "![x](https://e.com/a%20b%3Cc%3E)"
    );
    assert_eq!(
        md("<https://example.com>"),
        "[https://example.com](https://example.com)"
    );
}

#[test]
fn markdown_escapes_raw_html() {
    let out = md("```=html\n<script>alert(1)</script>\n```");
    assert!(!out.contains("<script>"));
    assert!(out.contains("&lt;script&gt;"));
}

#[test]
fn markdown_neutralizes_embedded_html() {
    assert!(!md("plain <img onerror=x> text").contains("<img"));
    let sup = md("{^<img src=x onerror=alert(1)>^}");
    assert!(sup.contains("<sup>"));
    assert!(!sup.contains("<img"));
    assert_eq!(md("a < b & c"), "a &lt; b &amp; c");
}

#[test]
fn ansi_and_plain_strip_control_bytes() {
    let ansi = carve::to_ansi("hi \x1b[31mX\x1b[0m\x07 there");
    assert!(!ansi.contains('\x1b'));
    assert!(!ansi.contains('\x07'));
    assert!(ansi.contains("there"));
    let plain = carve::to_plain_text("a\x1bb\x07c");
    assert!(!plain.contains('\x1b'));
    assert!(!plain.contains('\x07'));
}

#[test]
fn ansi_and_plain_strip_link_href_control_bytes() {
    let src = "[x](http://a/\x1b]0;PWNED\x07/b)";
    let ansi = carve::to_ansi(src);
    assert!(!ansi.contains("\x1b]0;"), "{ansi:?}");
    assert!(!ansi.contains('\x07'), "{ansi:?}");

    let plain = carve::to_plain_text(src);
    assert!(!plain.contains('\x1b'), "{plain:?}");
    assert!(!plain.contains('\x07'), "{plain:?}");
}

#[test]
fn markdown_escapes_link_and_image_titles() {
    assert_eq!(md("[x](u \"a \\\"b\")"), "[x](u \"a \\\"b\")");
    assert_eq!(md("![alt](img \"a \\\"b\")"), "![alt](img \"a \\\"b\")");
}
