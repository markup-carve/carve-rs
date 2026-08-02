//! §17 list tight/loose: text that follows a CLOSED block (fenced code, `:::`
//! div, admonition, table) inside a TIGHT list item stays BARE -- it is part of
//! the item's inline content, not a fresh `<p>` -- matching carve-js and the
//! executable-spec oracle. A blank line that separates the block from that
//! trailing text (blank-after / blank-both) instead loosens the item, so its
//! leading text and trailing text are each wrapped in `<p>`.

// ---- PART 1: trailing text after a closed block in a TIGHT item is bare ----

#[test]
fn tight_fence_trailing_text_is_bare() {
    assert_eq!(
        carve::to_html("- item\n  ```\n  c\n  ```\n  tail\n"),
        "<ul>\n  <li>item\n    <pre><code>c\n</code></pre>\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn tight_div_trailing_text_is_bare() {
    assert_eq!(
        carve::to_html("- item\n  ::: note\n  body\n  :::\n  tail\n"),
        "<ul>\n  <li>item\n    <aside class=\"admonition note\">\n      <p>body</p>\n    </aside>\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn tight_ordered_fence_trailing_text_is_bare() {
    assert_eq!(
        carve::to_html("1. item\n   ```\n   c\n   ```\n   tail\n"),
        "<ol>\n  <li>item\n    <pre><code>c\n</code></pre>\n    tail\n  </li>\n</ol>"
    );
}

#[test]
fn tight_fence_multiline_trailing_text_is_bare() {
    assert_eq!(
        carve::to_html("- item\n  ```\n  c\n  ```\n  t1\n  t2\n"),
        "<ul>\n  <li>item\n    <pre><code>c\n</code></pre>\n    t1\nt2\n  </li>\n</ul>"
    );
}

#[test]
fn tight_lead_fence_trailing_text_is_bare() {
    assert_eq!(
        carve::to_html("- ```\n  c\n  ```\n  tail\n"),
        "<ul>\n  <li>\n    <pre><code>c\n</code></pre>\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn tight_table_trailing_text_is_bare() {
    assert_eq!(
        carve::to_html("- item\n  | a | b |\n  | - | - |\n  | 1 | 2 |\n  tail\n"),
        "<ul>\n  <li>item\n    <table>\n      <thead><tr><th>a</th><th>b</th></tr></thead>\n      <tbody>\n        <tr><td>1</td><td>2</td></tr>\n      </tbody>\n    </table>\n    tail\n  </li>\n</ul>"
    );
}

// ---- PART 2: a blank that separates the block from trailing text loosens ----

#[test]
fn loose_fence_control_blank_both_wraps() {
    // A blank after the first line makes the item loose: its leading text and
    // trailing text are each wrapped in <p>.
    assert_eq!(
        carve::to_html("- item\n\n  ```\n  c\n  ```\n\n  tail\n"),
        "<ul>\n  <li><p>item</p>\n    <pre><code>c\n</code></pre>\n    <p>tail</p>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_after_block_loosens() {
    // Blank between the fence and the trailing text (no blank before the fence)
    // still loosens the item.
    assert_eq!(
        carve::to_html("- item\n  ```\n  c\n  ```\n\n  tail\n"),
        "<ul>\n  <li><p>item</p>\n    <pre><code>c\n</code></pre>\n    <p>tail</p>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_before_block_no_trailing_stays_tight() {
    // A blank BEFORE a single sub-block, with no trailing text, does NOT loosen
    // (the compact-block rule): the item stays tight, `item` is bare.
    assert_eq!(
        carve::to_html("- item\n\n  ```\n  c\n  ```\n"),
        "<ul>\n  <li>item\n    <pre><code>c\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_before_block_with_direct_trailing_stays_tight() {
    // Blank before the fence, but the trailing text follows the fence with NO
    // blank: the item stays tight and the trailing text is bare.
    assert_eq!(
        carve::to_html("- item\n\n  ```\n  c\n  ```\n  tail\n"),
        "<ul>\n  <li>item\n    <pre><code>c\n</code></pre>\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn blank_before_a_second_sub_block_stays_tight() {
    // A blank followed by another sub-block opener (a second fence) keeps the
    // item tight -- only a blank followed by plain prose loosens.
    assert_eq!(
        carve::to_html("- item\n  ```\n  c\n  ```\n\n  ```\n  d\n  ```\n"),
        "<ul>\n  <li>item\n    <pre><code>c\n</code></pre>\n    <pre><code>d\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn lead_fence_blank_trailing_text_loosens() {
    // A fence on the marker line, a blank, then trailing text: loose. The item
    // has no leading paragraph, so looseness shows only in the wrapped tail.
    assert_eq!(
        carve::to_html("- ```\n  c\n  ```\n\n  tail\n"),
        "<ul>\n  <li>\n    <pre><code>c\n</code></pre>\n    <p>tail</p>\n  </li>\n</ul>"
    );
}
