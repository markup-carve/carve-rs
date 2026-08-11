//! Post-blank list continuation follows the content-column model (carve#295,
//! spec PART 9 §24 C3). A block opener or sublist marker must reach the parent
//! item's content_column (`- `=2, `1. `=3, `10. `=4) to belong to the item.
//! Below content_column: after a blank it ends the item and parses at document
//! level, with no blank it lazily continues the item paragraph. At content_column
//! it nests. Above content_column it folds in as lazy paragraph text.
//!
//! The regression these guard: the boundary was previously keyed to a fixed
//! `base_indent + 2`, so an ordered item's deeper body column was mis-judged and
//! a below-content block opener wrongly nested.

// --- B1/B2/B3: below content_column, after a blank -> document level ---

#[test]
fn ordered_block_opener_below_content_after_blank_goes_to_document_level() {
    // `> q` at column 2 is BELOW `1. `'s content_column 3.
    assert_eq!(
        carve::to_html("1. one\n\n  > q"),
        "<ol>\n  <li>one</li>\n</ol>\n<p>&gt; q</p>"
    );
}

#[test]
fn ordered_paragraph_below_content_after_blank_goes_to_document_level() {
    assert_eq!(
        carve::to_html("1. one\n\n  text"),
        "<ol>\n  <li>one</li>\n</ol>\n<p>text</p>"
    );
}

#[test]
fn fence_below_content_after_blank_goes_to_document_level() {
    // Bullet content_column is 2; the 1-column fence is below it.
    assert_eq!(
        carve::to_html("- one\n\n ```\n c\n ```"),
        "<ul>\n  <li>one</li>\n</ul>\n<p><code>\nc\n</code></p>"
    );
}

// --- at content_column -> nests ---

#[test]
fn ordered_block_opener_at_content_column_nests() {
    // `> q` at column 3 IS `1. `'s content_column, so it nests.
    assert_eq!(
        carve::to_html("1. one\n\n   > q"),
        "<ol>\n  <li>one\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ol>"
    );
}

// --- B4: above content_column -> lazy paragraph text (inside the item) ---

#[test]
fn block_opener_above_content_column_folds_as_lazy_text() {
    // `# h` at column 3 is above the bullet's content_column 2: no longer a
    // heading, folds in as lazy paragraph text.
    assert_eq!(
        carve::to_html("- one\n\n   # h"),
        "<ul>\n  <li><p>one</p>\n    <p># h</p>\n  </li>\n</ul>"
    );
}

// --- no blank: below content_column -> lazy continuation of the item paragraph ---

#[test]
fn no_blank_block_opener_below_content_lazily_continues_paragraph() {
    // `> q` at column 1, no blank: folds into the item's open paragraph.
    assert_eq!(
        carve::to_html("1. one\n > q"),
        "<ol>\n  <li>one\n&gt; q</li>\n</ol>"
    );
}

// --- content-column finalize (carve#295 follow-through): above-content lazy text
// strips residual indent; def-list `::` and table are first-class block openers ---

#[test]
fn cc_final_para_above_content_strips_residual_indent() {
    assert_eq!(
        carve::to_html("- one\n   text\n"),
        "<ul>\n  <li>one\ntext</li>\n</ul>"
    );
}

#[test]
fn cc_final_quote_above_content_is_lazy_text() {
    assert_eq!(
        carve::to_html("- one\n   > q\n"),
        "<ul>\n  <li>one\n&gt; q</li>\n</ul>"
    );
}

#[test]
fn cc_final_table_below_content_is_lazy_text() {
    assert_eq!(
        carve::to_html("- one\n |=H|\n |x|\n"),
        "<ul>\n  <li>one\n|=H|\n|x|</li>\n</ul>"
    );
}

#[test]
fn cc_final_table_below_content_after_blank_doc_level() {
    assert_eq!(
        carve::to_html("- one\n\n |=H|\n |x|\n"),
        "<ul>\n  <li>one</li>\n</ul>\n<p>|=H|\n|x|</p>"
    );
}

#[test]
fn cc_final_deflist_interrupts_at_column_zero() {
    assert_eq!(
        carve::to_html("- one\n\n:: term\n:  def\n"),
        "<ul>\n  <li>one</li>\n</ul>\n<dl>\n  <dt>term</dt>\n  <dd>def</dd>\n</dl>"
    );
}

#[test]
fn cc_final_deflist_nests_at_content_column() {
    assert_eq!(
        carve::to_html("- one\n\n  :: term\n  :  def\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>def</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

// --- lenient def-attach: a `:  ` definition line below the term's content
// column still attaches as a `<dd>` (carve#295 follow-through). A definition
// marker is NOT subject to the column-0-exits rule that ends a list item;
// only a blank line before it ends the entry. Matches carve-php / carve-js. ---

#[test]
fn cc_def_below_content_column_attaches_as_dd() {
    // Term nested at content column 2; the `:  def` at column 0 is below it but
    // still attaches to the open definition (it does NOT orphan to a paragraph).
    assert_eq!(
        carve::to_html("- one\n+\n:: term\n:  def\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>def</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn cc_multiple_defs_below_content_column_attach() {
    assert_eq!(
        carve::to_html("- one\n+\n:: term\n:  d1\n:  d2\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>d1</dd>\n      <dd>d2</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn cc_def_below_content_column_lazy_body_folds() {
    // A flush-left line after the below-content def lazily continues its body.
    assert_eq!(
        carve::to_html("- one\n+\n:: term\n:  def\nmore\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>def\nmore</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn cc_def_below_content_column_after_blank_ends_entry() {
    // A blank line before the `:  def` ends the entry: the def orphans to a
    // top-level paragraph (the blank is a genuine terminator, not a separator
    // here because the term's content column is not reached).
    assert_eq!(
        carve::to_html("- one\n\n  :: term\n\n:  def\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n    </dl>\n  </li>\n</ul>\n<p>:  def</p>"
    );
}

#[test]
fn cc_bare_below_content_new_term_starts_top_level_list() {
    // A `:: term2` at column 0 is a first-class block opener: it does NOT attach
    // to the item's def-list, it starts a new top-level definition list.
    assert_eq!(
        carve::to_html("- one\n\n  :: term\n\n:: term2\n:  def2\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n    </dl>\n  </li>\n</ul>\n<dl>\n  <dt>term2</dt>\n  <dd>def2</dd>\n</dl>"
    );
}

#[test]
fn cc_final_bare_indented_table_row_is_paragraph() {
    assert_eq!(carve::to_html(" |=H|\n |x|\n"), "<p>|=H|\n|x|</p>");
}

#[test]
fn cc_colon_fence_below_content_column_is_lazy() {
    // §24 C3: a `:::` colon fence below the item's content column folds as lazy
    // paragraph text, not a nested container (mirrors quote/heading/table).
    assert_eq!(
        carve::to_html("- one\n ::: note\n body\n :::\n"),
        "<ul>\n  <li>one\n::: note\nbody\n:::</li>\n</ul>"
    );
}

#[test]
fn cc_colon_fence_nests_at_content_column() {
    assert_eq!(
        carve::to_html("- one\n\n  ::: note\n  body\n  :::\n"),
        "<ul>\n  <li>one\n    <aside class=\"admonition note\">\n      <p>body</p>\n    </aside>\n  </li>\n</ul>"
    );
}

#[test]
fn cc_colon_fence_interrupts_at_column_zero() {
    assert_eq!(
        carve::to_html("- one\n\n::: note\nbody\n:::\n"),
        "<ul>\n  <li>one</li>\n</ul>\n<aside class=\"admonition note\">\n  <p>body</p>\n</aside>"
    );
}
