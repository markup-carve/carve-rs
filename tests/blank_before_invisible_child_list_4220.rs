use carve::to_html;

#[test]
fn a_braced_comment_before_an_incompatible_child_list_keeps_the_blank() {
    let expected = "<ul>\n  <li><p>a</p>\n    <ol>\n      <li>b</li>\n    </ol>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>";
    for comment in ["{%%}", "{% hidden %}", "{%%} {%%}"] {
        let source = format!("- a\n  1. b\n\n  {comment}\n     - b\n");
        assert_eq!(to_html(&source), expected, "{source:?}");
    }
}

#[test]
fn a_compatible_sibling_still_joins_its_child_list() {
    let source = "- a\n  1. b\n\n  {%%}\n  2. b\n";
    let html = to_html(source);
    assert!(html.contains("<li>a\n    <ol>"), "{html}");
    assert!(html.contains("<li><p>b</p></li>"), "{html}");
}

#[test]
fn a_terminal_invisible_paragraph_does_not_loosen_the_parent() {
    let source = "- a\n  - b\n\n  {%%}\n";
    assert!(!to_html(source).contains("<li><p>a</p>"));
}
