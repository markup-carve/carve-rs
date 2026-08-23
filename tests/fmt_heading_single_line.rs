//! `fmt` must never write a heading whose text carries a line break: a heading
//! ENDS AT THE NEWLINE (PART 2), so emitting one would close the heading and
//! re-parse the remainder as a following block, moving text out of the title.
//!
//! No parse builds such a heading. An ingested AST can - PART 12 lets any inline
//! sit in a heading, break nodes included - so the writer collapses the break.
//! Matches carve-js.

const HEADING_WITH_SOFT_BREAK: &str = r#"{"type":"document","srcByteLength":0,"children":[
  {"type":"heading","level":1,"children":[
    {"type":"text","value":"a"},{"type":"soft_break"},{"type":"text","value":"b"}
  ]}
]}"#;

const HEADING_WITH_HARD_BREAK: &str = r#"{"type":"document","srcByteLength":0,"children":[
  {"type":"heading","level":1,"children":[
    {"type":"text","value":"a"},{"type":"hard_break"},{"type":"text","value":"b"}
  ]}
]}"#;

#[test]
fn a_soft_break_in_an_ingested_heading_collapses_to_a_space() {
    let doc = carve::from_json(HEADING_WITH_SOFT_BREAK).unwrap();
    let out = carve::render_carve(&doc).expect("the tree under test is within the render ceiling");
    assert_eq!(out, "# a b\n");
    // The point of the collapse: re-parsing keeps every word in the title.
    assert_eq!(
        carve::to_html(&out),
        "<section id=\"a-b\">\n  <h1>a b</h1>\n</section>"
    );
}

#[test]
fn a_hard_break_in_an_ingested_heading_collapses_too() {
    let doc = carve::from_json(HEADING_WITH_HARD_BREAK).unwrap();
    let out = carve::render_carve(&doc).expect("the tree under test is within the render ceiling");
    assert_eq!(out, "# a b\n");
    assert_eq!(
        carve::to_html(&out),
        "<section id=\"a-b\">\n  <h1>a b</h1>\n</section>"
    );
}

const HEADING_WITH_LITERAL_BACKSLASH: &str = r#"{"type":"document","srcByteLength":0,"children":[
  {"type":"heading","level":1,"children":[
    {"type":"text","value":"a\\"},{"type":"soft_break"},{"type":"text","value":"b"}
  ]}
]}"#;

#[test]
fn a_literal_backslash_before_the_break_survives_the_collapse() {
    // Only an ODD run of backslashes is a hard break's marker. Dropping one
    // unconditionally wrote `# a\ b`, where the escape swallows the space and
    // the author's backslash disappears on re-parse.
    let doc = carve::from_json(HEADING_WITH_LITERAL_BACKSLASH).unwrap();
    let out = carve::render_carve(&doc).expect("the tree under test is within the render ceiling");
    assert_eq!(out, "# a\\\\ b\n");
    assert_eq!(
        carve::to_html(&out),
        "<section id=\"a-b\">\n  <h1>a\\ b</h1>\n</section>"
    );
}

#[test]
fn a_leading_tab_is_content_and_survives_fmt() {
    // A heading's marker separator is a run of SPACES and none of it is content
    // (markup-carve/carve#1587), so the tab here is the title's first
    // character. The writer trimmed it alongside the separator's spaces and
    // wrote `## x`, which re-parses to a DIFFERENT title - the PART 11 §1
    // invariant `to_html(fmt(x)) == to_html(x)` fails on it, and corpus 406's
    // third pair is exactly this document.
    let source = "## \tx\n";
    let out = carve::to_carve(source);
    assert_eq!(out, "## \tx\n");
    assert_eq!(carve::to_html(&out), carve::to_html(source));
    assert_eq!(
        carve::to_html(source),
        "<section id=\"x\">\n  <h2>\tx</h2>\n</section>"
    );
}

#[test]
fn the_separators_own_spaces_are_still_dropped() {
    // The control the fix must not break: a leading SPACE is separator, never
    // content, because the run absorbs it and the writer re-emits exactly one.
    let source = "##  h\n";
    let out = carve::to_carve(source);
    assert_eq!(out, "## h\n");
    assert_eq!(carve::to_html(&out), carve::to_html(source));
}
