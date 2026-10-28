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
