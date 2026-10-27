#[test]
fn plain_lines_continue_a_list_table_caption_below_the_item_column() {
    for (tail, caption) in [("x", "c\nx"), ("x\ny", "c\nx\ny")] {
        let source = format!("- |a|\n  ^ c\n{tail}");
        let expected = format!("<ul>\n  <li>\n    <table>\n      <caption>{caption}</caption>\n      <tbody>\n        <tr><td>a</td></tr>\n      </tbody>\n    </table>\n  </li>\n</ul>");
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}

#[test]
fn a_sibling_item_ends_a_list_table_caption() {
    let source = "- |a|\n  ^ c\n- s";
    let expected = "<ul>\n  <li>\n    <table>\n      <caption>c</caption>\n      <tbody>\n        <tr><td>a</td></tr>\n      </tbody>\n    </table>\n  </li>\n  <li>s</li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
