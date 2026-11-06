// carve#5320: each `: ` line at a surviving item's marker column ends it.

#[test]
fn nested_description_markers_end_each_item_in_turn() {
    let input = "- - a\n    :: t\n    : u\n      ::: |\n        x\n  : b\n: c\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>a\n        <dl>\n          <dt>t</dt>\n          <dd>\n            <p>u</p>\n            <div class=\"line-block\">\n              <p>&nbsp;&nbsp;x</p>\n            </div>\n          </dd>\n        </dl>\n      </li>\n    </ul>\n    : b\n  </li>\n</ul>\n<p>: c</p>";
    assert_eq!(carve::to_html(input), expected);
}

#[test]
fn ordered_outer_item_ends_at_its_own_marker_column() {
    let input = "1. - a\n     :: t\n     : u\n       ::: |\n         x\n   : b\n: c\n";
    let html = carve::to_html(input);
    assert!(
        html.contains("    : b\n  </li>\n</ol>\n<p>: c</p>"),
        "{html}"
    );
}

#[test]
fn a_bare_stanza_still_takes_a_description_shaped_lazy_line() {
    let input = "- a\n  - b\n    ::: |\n      v\n      %%%\n: d\n";
    let html = carve::to_html(input);
    assert!(html.contains("v<br>\n: d</p>"), "{html}");
}

#[test]
fn a_description_shaped_stanza_line_does_not_end_an_item() {
    let input = "- a\n  - b\n    ::: |\n      : x\n: d\n";
    let html = carve::to_html(input);
    assert!(html.contains(": x<br>\n: d</p>"), "{html}");
}
