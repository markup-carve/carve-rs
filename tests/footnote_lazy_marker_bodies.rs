use carve::to_html;

fn note_body(source: &str) -> String {
    let html = to_html(source);
    let start = html.find("<li id=\"fn1\">").expect("missing note");
    let end = html[start..].find("</li>").expect("unclosed note") + start;
    html[start..end].to_owned()
}

#[test]
fn below_column_list_marker_is_lazy_note_text() {
    let source = "see[^n]\n\n[^n]: a\n- li\n";
    let html = to_html(source);
    assert!(note_body(source).contains("<p>a\n- li<a"), "{html}");
    assert!(!html.contains("<ul>"), "{html}");
}

#[test]
fn quoted_note_keeps_its_unmarked_lazy_follower() {
    let source = "> see[^n]\n>\n> [^n]: note\n> tail\n";
    let html = to_html(source);
    assert!(note_body(source).contains("<p>note\ntail<a"), "{html}");
    assert!(!html.contains("<p>tail</p>"), "{html}");
}

#[test]
fn blank_line_ends_the_lazy_note_paragraph() {
    let source = "see[^n]\n\n[^n]: a\n\n- li\n";
    let html = to_html(source);
    assert!(!note_body(source).contains("- li"), "{html}");
    assert!(html.contains("<ul>"), "{html}");
}

#[test]
fn note_does_not_capture_a_line_outside_its_list_host() {
    let source = "- see[^n]\n  [^n]: a\ntail\n";
    let html = to_html(source);
    assert!(!note_body(source).contains("tail"), "{html}");
    assert!(html.contains("<p>tail</p>"), "{html}");
}

#[test]
fn indented_link_shape_is_lazy_only_without_a_reachable_owner() {
    let source = "see[^n]\n\n[^n]: a\n [r]: /u\n";
    let html = to_html(source);
    assert!(note_body(source).contains("[r]: /u"), "{html}");

    let source = "- see[^n]\n  [^n]: a\n   [r]: /u\n";
    let html = to_html(source);
    assert!(!note_body(source).contains("[r]: /u"), "{html}");
}

#[test]
fn a_lazy_line_keeps_the_note_open_below_its_list_host() {
    let source = "- [^n]: a\n  - li\nx\n\nsee[^n]\n";
    let html = to_html(source);
    assert!(note_body(source).contains("a\n- li\nx"), "{html}");
    assert!(!html.contains("<p>x</p>"), "{html}");
}

#[test]
fn a_refused_first_block_plus_does_not_take_a_list() {
    let source = "see[^n]\n\n[^n]: +\n- li\n";
    let html = to_html(source);
    assert!(note_body(source).contains("<p>+<a"), "{html}");
    assert!(html.contains("<ul>"), "{html}");
}

#[test]
fn a_one_column_indented_marker_after_literal_plus_is_lazy_note_text() {
    let source = "see[^n]\n\n[^n]: +\n - li\n";
    let html = to_html(source);
    assert!(note_body(source).contains("<p>+\n- li<a"), "{html}");
    assert!(!html.contains("<ul>"), "{html}");
}

#[test]
fn an_open_note_keeps_a_line_after_its_quote_prefix_is_missed() {
    let source = "> see[^n]\n\n> [^n]: a\n> - li\nend\n";
    let html = to_html(source);
    assert!(note_body(source).contains("<p>a\n- li\nend<a"), "{html}");
    assert!(!html.contains("<p>end</p>"), "{html}");
}

#[test]
fn a_quoted_first_block_plus_attaches_the_following_paragraph() {
    let source = "> see[^n]\n\n> [^n]: +\n> tail\n";
    let html = to_html(source);
    assert!(note_body(source).contains("<p>tail<a"), "{html}");
    assert!(!note_body(source).contains("<p>+\n"), "{html}");
}

#[test]
fn an_attached_first_block_registers_a_link_definition_at_the_note_column() {
    let source = "see[^n] [r][]\n\n[^n]: +\ntail\n  [r]: /u\n";
    let html = to_html(source);
    assert!(html.contains("<a href=\"/u\">r</a>"), "{html}");
    assert!(!note_body(source).contains("[r]: /u"), "{html}");
}

#[test]
fn an_attached_code_fence_keeps_its_link_shaped_payload() {
    let source = "see[^n] [r][]\n\n[^n]: +\n```\n  [r]: /u\n```\n";
    let html = to_html(source);
    assert!(
        note_body(source).contains("<code>  [r]: /u\n</code>"),
        "{html}"
    );
    assert!(html.contains("[r][]"), "{html}");
}

#[test]
fn a_blank_releases_the_line_below_an_unclosed_note_body() {
    let source = "[^f]: b\n\n  [^g]: c\n  ```\n   [^n]: note\n\nSee [^n] and [^f] and [^g].\n";
    let html = to_html(source);
    assert!(html.starts_with("<p>See [^n] and "), "{html}");
}

#[test]
fn a_line_outside_the_quote_host_does_not_extend_its_note() {
    let source = "see[^p]\n\n> para\n> [^p]: n\ntail\n";
    let html = to_html(source);
    assert!(html.contains("</blockquote>\n<p>tail</p>"), "{html}");
    assert!(!note_body(source).contains("tail"), "{html}");
}
