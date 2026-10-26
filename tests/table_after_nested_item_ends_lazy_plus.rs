#[test]
fn table_after_nested_item_ends_lazy_plus() {
    let source = "- a\n  - b\n   | x |\n+ c\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n    <table>\n      <tbody>\n        <tr><td>x</td></tr>\n      </tbody>\n    </table>\n  </li>\n</ul>\n<p>+ c</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}
