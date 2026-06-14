//! S4 cross-impl conformance: carve-rs brought in line with carve-js/carve-php.

#[test]
fn mention_tag_reject_doubled_dots() {
    assert!(carve::to_html("@a..b\n").contains("<strong>@a</strong></span>..b"));
    assert!(carve::to_html("#a..b\n").contains("<strong>#a</strong></span>..b"));
    // interior single dots still part of the name
    assert!(carve::to_html("@john.doe\n").contains("<strong>@john.doe</strong>"));
}

#[test]
fn frontmatter_handles_crlf_and_empty() {
    assert_eq!(
        carve::to_html("---\r\ntitle: x\r\n---\r\n\r\nBody\r\n"),
        "<p>Body</p>"
    );
    assert_eq!(carve::to_html("---\n---\n"), "");
    assert_eq!(
        carve::to_html("---\ntitle: x\n---\n\nBody\n"),
        "<p>Body</p>"
    );
}

#[test]
fn escaped_special_does_not_form_a_smart_operator() {
    assert_eq!(carve::to_html("\\<= 5\n"), "<p>&lt;= 5</p>");
    assert_eq!(carve::to_html("\\-> x\n"), "<p>-&gt; x</p>");
    // unescaped still converts
    assert_eq!(carve::to_html("a <= b -> c\n"), "<p>a ≤ b → c</p>");
}
