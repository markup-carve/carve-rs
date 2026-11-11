use carve::to_html;

#[test]
fn tab_straddling_task_item_column_leaves_note() {
    let source = "- [ ] [^n]: t\n\t\t> x\n\nx[^n]\n";
    let expected = "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <blockquote><p>x</p></blockquote>\n  </li>\n</ul>\n<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>t<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim(), expected);
}

#[test]
fn spaces_still_reach_note_body() {
    let html = to_html("- [ ] [^n]: t\n        > x\n\nx[^n]\n");
    assert!(html.contains("<blockquote><p>x</p></blockquote>"), "{html}");
    assert!(
        html.contains("<li><input type=\"checkbox\" disabled> </li>"),
        "{html}"
    );
}
