#[test]
fn dangerous_scheme_angle_autolink_is_recognized_then_sanitized() {
    assert_eq!(
        carve::to_html("<vbscript:msgbox>"),
        "<p><a href=\"\">vbscript:msgbox</a></p>"
    );
}

// `email_autolink = {email_char}+ '@' {email_char}+ '.' {letter}+` — the
// trailing `.TLD` is MANDATORY and `:` is not an email_char (grammar.ebnf).

#[test]
fn email_autolink_with_tld_links() {
    assert_eq!(
        carve::to_html("<a@b.com>"),
        "<p><a href=\"mailto:a@b.com\">a@b.com</a></p>"
    );
}

#[test]
fn email_autolink_without_tld_stays_literal() {
    assert_eq!(carve::to_html("<a@b>"), "<p>&lt;a@b&gt;</p>");
}

#[test]
fn email_autolink_with_colon_stays_literal() {
    // `:` is not an email_char and `x@y` is not a valid url scheme.
    assert_eq!(carve::to_html("<x@y:z>"), "<p>&lt;x@y:z&gt;</p>");
}

#[test]
fn email_autolink_empty_local_part_is_a_mention_in_literal_brackets() {
    assert_eq!(
        carve::to_html("<@foo>"),
        "<p>&lt;<span class=\"mention\"><strong>@foo</strong></span>&gt;</p>"
    );
    assert_eq!(
        carve::to_html("<@foo:bar>"),
        "<p>&lt;<span class=\"mention\"><strong>@foo</strong></span>:bar&gt;</p>"
    );
}

#[test]
fn nbsp_inside_code_span_serializes_as_entity() {
    // A literal U+00A0 inside a code span renders as the named entity.
    assert_eq!(
        carve::to_html("`a\u{00a0}b`"),
        "<p><code>a&nbsp;b</code></p>"
    );
}

#[test]
fn malformed_prefixes_do_not_hide_the_final_autolink() {
    for prefix in ["<a:x ", "<a:x<", "<word "] {
        let source = format!("{}<https://example.com>{{.last}}", prefix.repeat(512));
        let html = carve::to_html(&source);
        assert_eq!(html.matches("<a ").count(), 1);
        assert!(html.contains("class=\"last\""));
        assert!(html.contains("href=\"https://example.com\""));
    }
}

#[test]
fn invalid_crossrefs_do_not_hide_later_valid_targets() {
    let source = format!("{} x></#valid>\n\n# valid", "</#".repeat(512));
    let html = carve::to_html(&source);
    assert!(html.contains("href=\"#valid\""));
    assert!(html.contains("&lt;"));
}

#[test]
fn a_nested_angle_in_a_crossref_target_remains_accepted() {
    let doc = carve::parse("</#a<b>");
    let carve::ast::BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    let carve::ast::InlineNode::CrossRef(crossref) = &paragraph.children[0] else {
        panic!("expected a cross-reference");
    };
    assert_eq!(crossref.target, "a<b");
}
