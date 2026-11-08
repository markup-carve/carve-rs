//! An unclosed note fence ends when a line returns to its host item.

use carve::to_html;

#[test]
fn host_line_before_a_later_fence_closer_survives() {
    let source = "- [^n0]: p\n      ```\n  f1\n    [^n2]: ```x\n      ```\n";
    assert_eq!(to_html(source).trim_end(), "<ul>\n  <li>f1</li>\n</ul>");
}
