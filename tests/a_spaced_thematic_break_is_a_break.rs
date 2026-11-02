//! carve#4814 (option B) and carve#4880: `* * *` and `- - -` are thematic breaks,
//! tested before a list marker on every line.
use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn single_spaces_in_a_star_or_hyphen_run_make_a_break() {
    assert_eq!(html("* * *"), "<hr>");
    assert_eq!(html("- - -"), "<hr>");
    assert_eq!(html("* * * *  "), "<hr>");
    assert_eq!(
        html("*  *  *"),
        "<ul>\n  <li>\n    <ul>\n      <li>*</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(html("_ _ _"), "<p>_ _ _</p>");
}

#[test]
fn a_spaced_run_below_a_list_ends_it() {
    let after = "<ul>\n  <li>x</li>\n</ul>\n<hr>";
    assert_eq!(html("- x\n- - -"), after);
    assert_eq!(html("- x\n* * *"), after);
    assert_eq!(html("- x\n\n- - -"), after);
    assert_eq!(
        html("- - x\n  - - -"),
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n    <hr>\n  </li>\n</ul>"
    );
}
