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
