//! List nesting uses the content column (Model A): a child nests only when
//! indented to at least the parent item's content column. Below it, a marker
//! folds -- under symmetric §10 no list marker (ordered, unordered, or task)
//! interrupts a paragraph, so a below-content-column child is lazy continuation.

#[test]
fn ordered_child_below_content_column_folds() {
    // col 2 < `1. ` content column (3): lazy continuation, not a sub-list.
    assert_eq!(
        carve::to_html("1. a\n  1. b"),
        "<ol>\n  <li>a\n1. b</li>\n</ol>"
    );
}

#[test]
fn unordered_child_below_content_column_folds() {
    // col 1 < `- ` content column (2): under symmetric §10 a bullet does not
    // interrupt, so a below-content-column child folds as lazy continuation.
    assert_eq!(
        carve::to_html("- a\n - b"),
        "<ul>\n  <li>a\n- b</li>\n</ul>"
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
fn tab_indented_ordered_child_reaches_content_column() {
    assert_eq!(
        carve::to_html("1. a\n\t1. b"),
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
fn tab_indented_unordered_child_nests() {
    assert_eq!(
        carve::to_html("- a\n\t- b"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn task_child_nests_at_the_bullet_content_column() {
    // A task's content column is the bullet width (2) -- the checkbox is
    // content, not marker -- so a child at column 2 nests (matches carve-php).
    assert_eq!(
        carve::to_html("- [ ] a\n  - b"),
        "<ul>\n  <li><input type=\"checkbox\" disabled> a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn unordered_child_below_the_ordered_content_column_folds() {
    // `- b` at col 2 is below the `10. ` content column (4); under symmetric §10
    // a bullet does not interrupt, so it folds as lazy continuation (it would
    // nest only at or past the content column).
    assert_eq!(
        carve::to_html("10. a\n  - b"),
        "<ol start=\"10\">\n  <li>a\n- b</li>\n</ol>"
    );
}

#[test]
fn tab_space_aligned_ordered_subitems_are_siblings() {
    // `\t  1. b` (tab to col 4, +2 = col 6) and `      2. c` (6 spaces) sit at the
    // same visual column, so they are siblings. A sub-list marker line is dedented
    // residual-aware so the partially-consumed tab keeps them aligned (matches
    // carve-php).
    assert_eq!(
        carve::to_html("1. a\n\t  1. b\n      2. c"),
        "<ol>\n  <li>a\n    <ol>\n      <li>b</li>\n      <li>c</li>\n    </ol>\n  </li>\n</ol>"
    );
}

#[test]
fn tab_space_aligned_unordered_subitems_are_siblings() {
    assert_eq!(
        carve::to_html("- a\n\t  - b\n      - c"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n      <li>c</li>\n    </ul>\n  </li>\n</ul>"
    );
}
