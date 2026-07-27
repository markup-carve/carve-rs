//! A column-0 plain continuation line after a nested sublist folds into the
//! DEEPEST open item (no blank line), matching carve-js and carve-php. A blank
//! line still ends the list; a block-opener or a sibling marker is not absorbed.

#[test]
fn lazy_after_sublist_folds_into_deepest() {
    assert_eq!(
        carve::to_html("- a\n  - b\nlazy"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\nlazy</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn sibling_marker_after_lazy_starts_new_item() {
    assert_eq!(
        carve::to_html("- a\n  - b\nlazy\n- c"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\nlazy</li>\n    </ul>\n  </li>\n  <li>c</li>\n</ul>"
    );
}

#[test]
fn indented_marker_after_lazy_resumes_same_sublist() {
    // The `lazy` line folds into the inner item; the following `2. sibling`,
    // indented to the sublist's content column, resumes the SAME sublist rather
    // than opening a fresh one (no stray `<ol start="2">`). Matches carve-php
    // and carve-js (carve spec corpus 05-lists-17).
    assert_eq!(
        carve::to_html("1. outer\n   1. inner\nlazy\n   2. sibling"),
        "<ol>\n  <li>outer\n    <ol>\n      <li>inner\nlazy</li>\n      <li>sibling</li>\n    </ol>\n  </li>\n</ol>"
    );
}

#[test]
fn blank_line_after_sublist_ends_the_list() {
    assert_eq!(
        carve::to_html("- a\n  - b\n\ntext"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<p>text</p>"
    );
}

// A blank line INSIDE the outer item -- before a PARAGRAPH that dedents to the
// outer item's content column after a nested sublist -- makes the OUTER item
// loose (its first paragraph is wrapped in `<p>`). The inner item stays tight:
// nested looseness does not propagate (corpus 142). Matches carve-js.

#[test]
fn blank_then_outer_paragraph_after_sublist_loosens_outer_only() {
    // `> q` at column 3 is above the outer content column (2) but below the
    // inner content column (4): the `>` does not open a block quote, it attaches
    // to the OUTER item as a paragraph, and the internal blank loosens the outer.
    assert_eq!(
        carve::to_html("- a\n  - b\n\n   > q\n"),
        "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>b</li>\n    </ul>\n    <p>&gt; q</p>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_then_plain_outer_paragraph_after_sublist_loosens_outer() {
    assert_eq!(
        carve::to_html("- a\n  - b\n\n  b\n"),
        "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>b</li>\n    </ul>\n    <p>b</p>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_then_outer_blockquote_after_sublist_stays_tight() {
    // Regression anchor: a real (flush-to-content-column) block quote attached
    // after the blank is NOT a paragraph, so the outer item stays tight.
    assert_eq!(
        carve::to_html("- a\n  - b\n\n  > q\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_then_inner_attached_block_does_not_loosen_outer() {
    // Regression anchor (corpus 142): `> q` at column 4 reaches the inner item's
    // content column, so it nests inside the inner item. The outer item has no
    // internal blank of its own and stays tight.
    assert_eq!(
        carve::to_html("- a\n  - b\n\n    > q\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <blockquote><p>q</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_inside_inner_item_does_not_loosen_outer() {
    // The blank precedes `c` (at the inner item's content column, so `c` nests
    // in item `b`); `d` then attaches to the outer item with NO blank of its own
    // directly before it. The blank belongs to the inner item, so the OUTER item
    // stays tight (`<li>a`), not `<li><p>a</p>`.
    assert_eq!(
        carve::to_html("- a\n  - b\n\n    c\n  d\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li><p>b</p>\n        <p>c</p>\n      </li>\n    </ul>\n    <p>d</p>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_inside_inner_task_item_does_not_loosen_outer() {
    // Same as above with a TASK sub-item: the inner item's content column is the
    // bullet width (2), not the post-checkbox column, so `c` (indented to the
    // inner content column) nests in the task item and the blank belongs to the
    // inner item. The OUTER item stays tight, matching a plain sub-item.
    assert_eq!(
        carve::to_html("- a\n  - [ ] b\n\n    c\n  d\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li><p><input type=\"checkbox\" disabled> b</p>\n        <p>c</p>\n      </li>\n    </ul>\n    <p>d</p>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_before_non_paragraph_outer_block_stays_tight() {
    // The blank directly precedes an outer `<hr>` (a thematic break), NOT a
    // paragraph. Only a blank directly before an attached PARAGRAPH loosens the
    // outer item, so it stays tight even though a paragraph (`p`) follows the
    // `<hr>`. Matches carve-js.
    assert_eq!(
        carve::to_html("- a\n  - b\n\n  ---\n  p\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n    <hr>\n    <p>p</p>\n  </li>\n</ul>"
    );
}
