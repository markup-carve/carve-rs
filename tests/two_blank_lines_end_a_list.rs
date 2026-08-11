use carve::{to_carve, to_html};

#[test]
fn one_blank_line_still_makes_one_loose_list() {
    assert_eq!(
        to_html("1. a\n\n2. b\n"),
        "<ol>\n  <li><p>a</p></li>\n  <li><p>b</p></li>\n</ol>"
    );
}

#[test]
fn two_blank_lines_make_compatible_sibling_lists() {
    for (source, html) in [
        (
            "1. a\n\n\n2. b\n",
            "<ol>\n  <li>a</li>\n</ol>\n<ol start=\"2\">\n  <li>b</li>\n</ol>",
        ),
        (
            "- a\n\n\n- b\n",
            "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>",
        ),
    ] {
        assert_eq!(to_html(source), html);
        assert_eq!(to_carve(source), source);
        assert_eq!(to_carve(&to_carve(source)), source);
    }
}
