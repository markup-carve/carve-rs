//! Symmetric list interruption (grammar PART 9 §10).
//!
//! No list marker interrupts an open paragraph: a bullet (`- `/`* `) and a task
//! marker need a blank line, exactly like an ordered marker already did. Without
//! the blank line the marker folds into the open paragraph / heading / quoted
//! paragraph as lazy continuation. Tight nested lists are unaffected (a marker at
//! the content column nests; below it, it folds). Rule B: a marker that dedents
//! below an indented list's base column starts a sibling list.

fn h(src: &str) -> String {
    carve::to_html(src)
}

#[test]
fn bullet_folds_into_prose() {
    assert_eq!(h("intro\n- a"), "<p>intro\n- a</p>");
}

#[test]
fn ordered_folds_into_prose() {
    assert_eq!(h("intro\n1. a"), "<p>intro\n1. a</p>");
}

#[test]
fn blank_line_starts_the_list() {
    assert_eq!(h("intro\n\n- a"), "<p>intro</p>\n<ul>\n  <li>a</li>\n</ul>");
}

#[test]
fn thematic_break_is_separate_with_explicit_boundaries() {
    assert_eq!(h("intro\n\n---\n\nmore"), "<p>intro</p>\n<hr>\n<p>more</p>");
}

#[test]
fn bullet_ends_a_heading_and_starts_a_sibling_list() {
    // A list marker does not fold into a heading: it ends the heading and starts
    // a top-level sibling list (matches djot; bullet and ordered alike).
    assert_eq!(
        h("# T\n- item"),
        "<section id=\"T\">\n  <h1>T</h1>\n  <ul>\n    <li>item</li>\n  </ul>\n</section>"
    );
}

#[test]
fn bullet_folds_into_an_open_quoted_paragraph() {
    // A list marker folds into the open quoted paragraph as lazy continuation,
    // exactly like it folds into a top-level paragraph: a list needs a blank
    // line before it. (A heading is the sole construct a list marker ends.)
    assert_eq!(
        h("> quoted\n- item"),
        "<blockquote><p>quoted\n- item</p></blockquote>"
    );
}

#[test]
fn ordered_folds_into_an_open_quoted_paragraph() {
    assert_eq!(
        h("> quoted\n1. item"),
        "<blockquote><p>quoted\n1. item</p></blockquote>"
    );
}

#[test]
fn bullet_ends_a_quoted_heading_and_starts_a_sibling_list() {
    // A list marker folds only into an OPEN PARAGRAPH. A quoted heading leaves
    // no open paragraph, so the marker ends the quote and starts a top-level
    // sibling list -- mirroring the top-level `# T` / `- item` case.
    assert_eq!(
        h("> # h\n- item"),
        "<blockquote>\n  <h1 id=\"h\">h</h1>\n</blockquote>\n<ul>\n  <li>item</li>\n</ul>"
    );
}

#[test]
fn ordered_ends_a_quoted_heading_and_starts_a_sibling_list() {
    assert_eq!(
        h("> # h\n1. item"),
        "<blockquote>\n  <h1 id=\"h\">h</h1>\n</blockquote>\n<ol>\n  <li>item</li>\n</ol>"
    );
}

#[test]
fn bullet_ends_a_quote_whose_last_line_is_a_heading() {
    // A paragraph followed by a quoted heading: the heading closes the open
    // paragraph, so the trailing list marker ends the quote (does not fold).
    assert_eq!(
        h("> a\n>\n> # h\n\n- item"),
        "<blockquote>\n  <p>a</p>\n  <h1 id=\"h\">h</h1>\n</blockquote>\n<ul>\n  <li>item</li>\n</ul>"
    );
}

#[test]
fn tight_nesting_at_content_column_is_kept() {
    assert_eq!(
        h("- a\n  - tight\n- list"),
        "<ul>\n  <li>a\n    <ul>\n      <li>tight</li>\n    </ul>\n  </li>\n  <li>list</li>\n</ul>"
    );
}

#[test]
fn below_content_column_bullet_folds() {
    assert_eq!(h("- a\n - b"), "<ul>\n  <li>a\n- b</li>\n</ul>");
}

#[test]
fn dedent_below_indented_base_starts_sibling_list() {
    // Rule B: distinct base columns are distinct lists.
    assert_eq!(
        h("  - a\n  - b\n- c"),
        "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>\n<ul>\n  <li>c</li>\n</ul>"
    );
}

#[test]
fn dedented_plain_text_lazily_continues_the_item() {
    // Only a dedented MARKER starts a sibling list; plain text folds in.
    assert_eq!(
        h("  - a\n continued"),
        "<ul>\n  <li>a\ncontinued</li>\n</ul>"
    );
}
