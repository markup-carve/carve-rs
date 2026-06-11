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
fn unordered_child_below_content_column_still_nests() {
    // Unordered markers interrupt (§10), so any indent past the parent base
    // column nests even below the parent content column.
    assert_eq!(
        carve::to_html("- a\n - b"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
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
fn unordered_child_nests_regardless_of_the_ordered_content_column() {
    // Unordered markers interrupt (§10), so a `- b` indented under `10. ` nests
    // even below the ordered content column -- only ordered markers are gated.
    assert_eq!(
        carve::to_html("10. a\n  - b"),
        "<ol start=\"10\">\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ol>"
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
