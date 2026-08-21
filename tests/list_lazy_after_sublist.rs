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
    // stays tight (`<li>a`), not `<li><p>a</p>`. The innermost open paragraph
    // receives the lazy line, matching carve-js and carve-php.
    assert_eq!(
        carve::to_html("- a\n  - b\n\n    c\n  d\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li><p>b</p>\n        <p>c\nd</p>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_inside_inner_task_item_does_not_loosen_outer() {
    // Same as above with a TASK sub-item: the inner item's content column is the
    // bullet width (2), not the post-checkbox column, so `c` (indented to the
    // inner content column) nests in the task item and the blank belongs to the
    // inner item. The OUTER item stays tight and the innermost open paragraph
    // receives `d`, as it does in the other implementations.
    assert_eq!(
        carve::to_html("- a\n  - [ ] b\n\n    c\n  d\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li><input type=\"checkbox\" disabled aria-label=\"b\"> <p>b</p>\n        <p>c\nd</p>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn flush_line_folds_into_an_items_second_open_paragraph() {
    assert_eq!(
        carve::to_html("1. item\n\n   spaced\nflush\n"),
        "<ol>\n  <li><p>item</p>\n    <p>spaced\nflush</p>\n  </li>\n</ol>"
    );
}

#[test]
fn continuation_marker_after_the_second_paragraph_is_not_lazy_text() {
    assert_eq!(
        carve::to_html("1. item\n\n   spaced\n+\nquote\n"),
        "<ol>\n  <li><p>item</p>\n    <p>spaced</p>\n    <p>quote</p>\n  </li>\n</ol>"
    );
}

#[test]
fn blank_before_non_paragraph_outer_block_stays_tight() {
    // The blank directly precedes an outer `<hr>` (a thematic break), NOT a
    // paragraph. Only a blank directly before an attached PARAGRAPH loosens the
    // outer item, so it stays tight even though a paragraph (`p`) follows the
    // `<hr>`. Being tight, the trailing text `p` renders BARE. Matches carve-js.
    assert_eq!(
        carve::to_html("- a\n  - b\n\n  ---\n  p\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n    <hr>\n    p\n  </li>\n</ul>"
    );
}

// --- Below every content column: lazy text, not a block (carve-rs#512) ---

#[test]
fn a_below_column_heading_after_a_marker_line_sublist_folds_as_text() {
    // One column in, `# H` reaches no content column, so it opens nothing and
    // folds into the sub-item's open paragraph. This engine published an `<h1>`
    // attached to the OUTER item, because collection dedented the line to the
    // body's column 0 and the recursive parse then read a heading.
    assert_eq!(
        carve::to_html("- - a\n # H"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n# H</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_below_column_marker_after_a_marker_line_sublist_folds_as_text() {
    assert_eq!(
        carve::to_html("- - a\n - b"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n- b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn the_same_holds_for_a_following_line_sublist() {
    assert_eq!(
        carve::to_html("- x\n  - a\n - b"),
        "<ul>\n  <li>x\n    <ul>\n      <li>a\n- b</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- x\n  - a\n # H"),
        "<ul>\n  <li>x\n    <ul>\n      <li>a\n# H</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_at_the_sublists_own_column_is_still_a_sibling() {
    // The fold is about being BELOW every content column, not about the marker.
    assert_eq!(
        carve::to_html("- - a\n  - b"),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn two_columns_in_folds_as_well() {
    // §24 C3 does not ask how deep the indent is: below the content column a
    // marker folds as lazy item text, at any depth. Every engine used to nest
    // this one under `a`, because the folded line kept its own indentation and
    // that reached the SUB-list's content column on the reparse (carve#603).
    assert_eq!(
        carve::to_html("-   x\n    - a\n  - b"),
        "<ul>\n  <li>x\n    <ul>\n      <li>a\n- b</li>\n    </ul>\n  </li>\n</ul>"
    );
    // Three columns in, and with the whole list indented, are the same line.
    assert_eq!(
        carve::to_html("-   x\n    - a\n   - b"),
        "<ul>\n  <li>x\n    <ul>\n      <li>a\n- b</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("  - x\n    - a\n   - b"),
        "<ul>\n  <li>x\n    <ul>\n      <li>a\n- b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn an_under_indented_definition_still_attaches() {
    // The documented exception: a definition marker attaches to the term above
    // it from ANY column, so it keeps dedenting to the body's column 0 rather
    // than folding as text (corpus 154).
    assert_eq!(
        carve::to_html("- one\n  :: term\n :  def\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>def</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn a_below_column_definition_term_folds_but_its_definition_still_attaches() {
    // The TERM is not lenient: one column in it is text like any other opener.
    assert_eq!(
        carve::to_html("- - a\n :: term"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n:: term</li>\n    </ul>\n  </li>\n</ul>"
    );
    // Its DEFINITION is: `:  def` attaches to the term above it from any
    // column, which is corpus 154 and the reason the two are separated here.
    assert_eq!(
        carve::to_html("- one\n  :: term\n :  def\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>def</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn a_below_column_quote_and_table_row_fold_as_text() {
    assert_eq!(
        carve::to_html("- - a\n > q"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n&gt; q</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- - a\n | c |"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n| c |</li>\n    </ul>\n  </li>\n</ul>"
    );
}
