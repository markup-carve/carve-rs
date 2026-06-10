//! List nesting uses the content column (Model A): a child nests only when
//! indented to at least the parent item's content column. Below it, an ordered
//! marker folds (§10: ordered does not interrupt), an unordered/task marker
//! interrupts.

#[test]
fn ordered_child_below_content_column_folds() {
    // col 2 < `1. ` content column (3): lazy continuation, not a sub-list.
    assert_eq!(
        carve::to_html("1. a\n  1. b"),
        "<ol>\n  <li>a\n1. b</li>\n</ol>"
    );
}

#[test]
fn ordered_child_at_content_column_nests() {
    assert_eq!(
        carve::to_html("1. a\n   1. b"),
        "<ol>\n  <li>a\n    <ol>\n      <li>b</li>\n    </ol>\n  </li>\n</ol>"
    );
}

#[test]
fn unordered_child_at_content_column_nests() {
    assert_eq!(
        carve::to_html("- a\n  - b"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn marker_below_a_wide_marker_content_column_interrupts() {
    // `- b` at col 2 is below the task's content column (6); unordered
    // interrupts -> a new sibling list, not nesting.
    assert_eq!(
        carve::to_html("- [ ] a\n  - b"),
        "<ul>\n  <li><input type=\"checkbox\" disabled> a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
}
