// A comment or definition ends the open paragraph and closes no container
// (carve#4862), so a following line below the item's column has nothing to
// fold into and leaves (PART 0 S4, CARVE-P9R-002). Expected HTML is the spec
// model's.

#[test]
fn a_line_after_a_comment_below_a_marker_line_description_leaves_the_item() {
    assert_eq!(
        carve::to_html("- :: term\n  : desc\n%% c\n after\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>term</dt>\n      <dd>desc</dd>\n    </dl>\n  </li>\n</ul>\n<p>after</p>"
    );
    assert_eq!(
        carve::to_html("10. :: term\n    : desc\n %% c\n   after\n"),
        "<ol start=\"10\">\n  <li>\n    <dl>\n      <dt>term</dt>\n      <dd>desc</dd>\n    </dl>\n  </li>\n</ol>\n<p>after</p>"
    );
    assert_eq!(
        carve::to_html("- x\n  - :: term\n    : desc\n%% c\n   after\n"),
        "<ul>\n  <li>x\n    <ul>\n      <li>\n        <dl>\n          <dt>term</dt>\n          <dd>desc</dd>\n        </dl>\n      </li>\n    </ul>\n    after\n  </li>\n</ul>"
    );
}

#[test]
fn a_line_after_a_quoted_definition_in_a_note_leaves_the_note() {
    for source in [
        "[^n]: a\n  > prose\n  > [r]: /x\ntail\n",
        "[^n]: a\n\n  > prose\n  > [r]: /x\n tail\n",
        "[^n]: > prose\n      > [r]: /x\ntail\n",
    ] {
        assert_eq!(carve::to_html(source), "<p>tail</p>", "{source:?}");
    }
}
