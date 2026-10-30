#[test]
fn unattached_caret_continues_list_paragraph() {
    let html = carve::to_html("- a\n^ caption\n");
    assert!(html.contains("<li>a\n^ caption</li>"), "{html}");
    let heading = carve::to_html("- a\n# H\n");
    assert!(heading.contains("</ul>\n<section id=\"H\">"), "{heading}");
}

#[test]
fn caret_at_definition_term_column_ends_term() {
    let html = carve::to_html(":: term\n^ caption\n");
    assert!(html.contains("<dt>term</dt>"), "{html}");
    assert!(html.contains("<p>^ caption</p>"), "{html}");

    let plain = carve::to_html(":: term\nplain\n");
    assert!(plain.contains("<dt>term\nplain</dt>"), "{plain}");
}

#[test]
fn indented_row_ends_table() {
    let html = carve::to_html("| a |\n   | b |\n");
    assert!(html.contains("<tr><td>b</td></tr>"), "{html}");
    assert_eq!(html.matches("<tr>").count(), 2, "{html}");

    let adjacent = carve::to_html("| a |\n| b |\n");
    assert_eq!(adjacent.matches("<tr>").count(), 2, "{adjacent}");
}

#[test]
fn flush_left_line_leaves_nested_quoted_colon() {
    let html = carve::to_html("> - ::: box\ntail\n");
    assert!(html.contains("</blockquote>\n<p>tail</p>"), "{html}");

    let nested = carve::to_html("> - ::: box\n>   tail\n");
    assert!(nested.contains("tail</p>"), "{nested}");
    assert!(!nested.contains("</blockquote>\n<p>tail</p>"), "{nested}");
}

#[test]
fn note_definition_in_nested_code_is_literal() {
    let html = carve::to_html("::::\n```\n[^n]: note\n");
    assert!(html.contains("<code>[^n]: note\n</code>"), "{html}");
    assert!(!html.contains("%%"), "{html}");

    let plain = carve::to_html("::::\n```\nplain\n");
    assert!(plain.contains("<code>plain\n</code>"), "{plain}");
}

#[test]
fn heading_after_marker_line_comment_opens_as_child() {
    let html = carve::to_html("- %%\n   # H\n");
    assert!(html.contains("<li>\n"), "{html}");
    assert!(html.contains("<h1 id=\"H\">H</h1>"), "{html}");
    assert!(!html.contains("<li># H</li>"), "{html}");

    let at_column = carve::to_html("- %%\n  # H\n");
    assert!(at_column.contains("<h1 id=\"H\">H</h1>"), "{at_column}");
}

#[test]
fn indented_plus_after_table_is_text() {
    let html = carve::to_html("| a |\n  + |\n");
    assert!(html.contains("</table>\n<p>+ |</p>"), "{html}");

    let at_column = carve::to_html("| a |\n+ |\n");
    assert!(!at_column.contains("<p>+ |</p>"), "{at_column}");
}

#[test]
fn line_after_note_comment_remains_in_unreferenced_note() {
    let html = carve::to_html("[^n]: note\n%%\n > x\n# H\n");
    assert!(
        html.starts_with("<blockquote><p>x</p></blockquote>\n<section id=\"H\">"),
        "{html}"
    );
    assert!(!html.contains("&gt; x"), "{html}");

    let control = carve::to_html("%%\n > x\n# H\n");
    assert_eq!(control, html);
}

#[test]
fn three_percent_comment_does_not_hide_nested_colon() {
    let html = carve::to_html("- tail\n  %%%\n   ::::\n");
    assert!(html.contains("<div>"), "{html}");
    assert!(!html.contains("::::"), "{html}");

    let control = carve::to_html("- tail\n  %%\n   ::::\n");
    assert!(control.contains("<div>"), "{control}");
}

#[test]
fn definition_interrupts_unclosed_code_in_list_paragraph() {
    let html = carve::to_html("- x\n```\n  [r]: l\n");
    assert!(!html.contains("<code></code>"), "{html}");
    assert!(html.contains("[r]: l"), "{html}");

    let control = carve::to_html("- x\n```\n  plain\n");
    assert!(control.contains("plain\n</code>"), "{control}");
}

#[test]
fn marker_line_comment_does_not_hide_indented_quote() {
    let html = carve::to_html("- %%%\n   > x\nl\n");
    assert!(html.contains("<blockquote>"), "{html}");
    assert!(html.contains("x\nl"), "{html}");

    let control = carve::to_html("- %%%\n  > x\nl\n");
    assert!(control.contains("<blockquote>"), "{control}");
}

#[test]
fn flush_left_list_leaves_quoted_definition_term() {
    let html = carve::to_html("> :: c\n- t\n");
    assert!(html.contains("<dt>c</dt>"), "{html}");
    assert!(html.contains("</blockquote>\n<ul>"), "{html}");

    let nested = carve::to_html("> :: c\n> - t\n");
    assert!(nested.contains("<ul>"), "{nested}");
    assert!(!nested.contains("</blockquote>\n<ul>"), "{nested}");
}

#[test]
fn below_column_list_marker_leaves_empty_quote_item() {
    let html = carve::to_html("1. >\n - x\n");
    assert!(html.contains("</ol>\n<ul>"), "{html}");
    assert!(!html.contains("    - x"), "{html}");

    let nested = carve::to_html("1. >\n  - x\n");
    assert!(nested.contains("<ul>"), "{nested}");
    assert!(nested.contains("</ol>\n<ul>"), "{nested}");
}

#[test]
fn definition_shaped_marker_stays_in_figure_caption() {
    for continuation in ["- [a]: /u", "- [^f]: note"] {
        let source = format!("> q\n^ c\n{continuation}\n");
        let html = carve::to_html(&source);
        assert!(
            html.contains(&format!("<figcaption>c\n{continuation}</figcaption>")),
            "{html}"
        );
    }
}

#[test]
fn new_quote_does_not_inherit_nested_list_definition_column() {
    let html = carve::to_html("> - > - p\n> >   [^f]: n\n");
    assert!(!html.contains("[^f]: n"), "{html}");
    assert!(
        html.ends_with("  <blockquote>\n\n  </blockquote>\n</blockquote>"),
        "{html}"
    );
    let at_column = carve::to_html("> - > - p\n> > [^f]: n\n\nx[^f]\n");
    assert!(!at_column.contains("[^f]: n"), "{at_column}");
    assert!(at_column.contains("role=\"doc-endnotes\""), "{at_column}");
}
