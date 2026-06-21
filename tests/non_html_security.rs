//! Non-HTML render targets are safe-by-default: Markdown cannot carry XSS into a
//! downstream Markdown -> HTML render, and ANSI/plain cannot inject terminal
//! escape sequences.

fn md(src: &str) -> String {
    carve::to_markdown(src).trim().to_string()
}

#[test]
fn markdown_blanks_dangerous_url_schemes() {
    assert!(md("[x](javascript:alert(1))").contains("[x]()"));
    assert!(md("![a](javascript:alert(1))").contains("![a]()"));
    assert!(md("[ok](https://e.com)").contains("[ok](https://e.com)"));
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
