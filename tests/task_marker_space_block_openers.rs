use carve::to_html;

#[test]
fn a_footnote_definition_after_task_content_space_registers() {
    let expected = "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> </li>\n</ul>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    for marker in ["-", "*"] {
        for extra_space in [" ", "  "] {
            let source = format!("{marker} [ ] {extra_space}[^f]: x\n\nr[^f]\n");
            assert_eq!(to_html(&source).trim(), expected, "{source}");
        }
    }
}

#[test]
fn a_heading_after_task_content_space_opens_as_a_block() {
    assert_eq!(
        to_html("- [ ]  # h\n").trim(),
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn a_tab_before_a_marker_line_definition_is_not_content_spacing() {
    assert_eq!(
        to_html("- \t[^f]: note\n- \tafter[^f]\n").trim(),
        "<ul>\n  <li>\t[^f]: note</li>\n  <li>\tafter[^f]</li>\n</ul>"
    );
}
