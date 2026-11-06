// carve#5312: after a description's line block in an item, a `: ` line at an
// item's marker column ends that item; right of it the line folds into the stanza.

#[test]
fn a_description_marker_at_the_item_column_ends_the_item() {
    let html = carve::to_html("- a\n  :: t\n  : u\n    ::: |\n      x\n: b\n");
    assert_eq!(html, "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <p>u</p>\n        <div class=\"line-block\">\n          <p>&nbsp;&nbsp;x</p>\n        </div>\n      </dd>\n    </dl>\n  </li>\n</ul>\n<p>: b</p>");
}

#[test]
fn a_description_marker_right_of_the_item_column_folds_into_the_stanza() {
    let html = carve::to_html("1. a\n   :: t\n   : u\n     ::: |\n       x\n  : b\n");
    assert_eq!(html, "<ol>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <p>u</p>\n        <div class=\"line-block\">\n          <p>&nbsp;&nbsp;x<br>\n: b</p>\n        </div>\n      </dd>\n    </dl>\n  </li>\n</ol>");
}

#[test]
fn a_description_marker_at_a_nested_item_column_ends_only_that_item() {
    let html = carve::to_html("- - a\n    :: t\n    : u\n      ::: |\n        x\n  : b\n");
    assert_eq!(html, "<ul>\n  <li>\n    <ul>\n      <li>a\n        <dl>\n          <dt>t</dt>\n          <dd>\n            <p>u</p>\n            <div class=\"line-block\">\n              <p>&nbsp;&nbsp;x</p>\n            </div>\n          </dd>\n        </dl>\n      </li>\n    </ul>\n    : b\n  </li>\n</ul>");
}
