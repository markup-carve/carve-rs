//! PART 1 S4 asks ONE question - does the last block leave a paragraph open? -
//! and `markup-carve/carve#1280` ruled that the answer does not depend on which
//! container the block was written in. A definition body is such a container
//! (`markup-carve/carve#956`) and the container kind is not a parameter of the
//! rule (`markup-carve/carve#920`), so `:  X` / `tail` has to answer whatever
//! `- X` / `tail` answers for every X.
//!
//! It did not. The definition body answered from an ENUMERATION of block kinds,
//! and the enumeration disagreed with itself: a table, a thematic break and an
//! attribute block ended the body, while a HEADING and a COMMENT folded the
//! following line in - and the list spelling of both of those already ended, in
//! this same engine, one clause over (carve-rs#1049).
//!
//! THE PAIRING IS THE POINT. Every row asserts the definition against its list
//! twin rather than against a literal, because a literal would let the two drift
//! apart again without a test noticing - which is exactly how the two kinds
//! below got a second answer.

fn definition(content: &str) -> String {
    carve::to_html(&format!(":: t\n:  {content}\ntail\n"))
}

fn list(content: &str) -> String {
    carve::to_html(&format!("- {content}\ntail\n"))
}

/// The `dd`/`li` wrapper differs by construction; what has to agree is where
/// `tail` ended up. It is either the container's last content or a top-level
/// paragraph after the container closed, and nothing else.
fn tail_escaped(html: &str) -> bool {
    html.ends_with("<p>tail</p>")
}

fn assert_same_answer(content: &str) {
    let def = definition(content);
    let lst = list(content);
    assert_eq!(
        tail_escaped(&def),
        tail_escaped(&lst),
        "`:  {content}` and `- {content}` disagree about the following line\n  definition: {def}\n  list:       {lst}"
    );
}

#[test]
fn a_heading_ends_both() {
    assert_same_answer("# H");
    assert_eq!(
        definition("# H"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <h1 id=\"H\">H</h1>\n  </dd>\n</dl>\n<p>tail</p>"
    );
}

#[test]
fn a_comment_ends_both() {
    // A comment renders nothing, so the body holds no paragraph at all - not an
    // earlier one to look past, because the comment IS the marker line.
    assert_same_answer("%% c");
    assert_eq!(
        definition("%% c"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>tail</p>"
    );
}

#[test]
fn a_table_ends_both() {
    assert_same_answer("| a |");
}

#[test]
fn a_thematic_break_ends_both() {
    assert_same_answer("---");
}

#[test]
fn an_attribute_block_ends_both() {
    assert_same_answer("{.k}");
}

#[test]
fn control_a_paragraph_folds_in_both() {
    // The one shape that DOES leave a paragraph open, so the line folds. Without
    // it every row above would still pass if the body simply stopped folding
    // for everything.
    assert_same_answer("d");
    assert!(!tail_escaped(&definition("d")));
    assert_eq!(
        definition("d"),
        "<dl>\n  <dt>t</dt>\n  <dd>d\ntail</dd>\n</dl>"
    );
}

#[test]
fn control_a_bare_image_folds_in_both() {
    // A bare image line is a block ONLY while nothing folds into it, and the
    // line that decides that is the very line S4 is being asked about - which
    // the body collected so far does not hold yet. Both containers fold.
    assert_same_answer("![a](i.png)");
    assert!(!tail_escaped(&definition("![a](i.png)")));
}

#[test]
fn the_content_column_half_answers_the_same_way_now() {
    // S4's clause used to leave the CONTENT-COLUMN half open, and this row
    // asserted that a heading collected at a definition body's content column
    // still took the fold - on the reading that corpus
    // 75-list-nesting-and-looseness-4 pinned it. It does not: there the lazy
    // line lands in the OUTER item, not in the item whose last block is the
    // heading. markup-carve/carve#1911 closed the half, so both columns end the
    // body and the marker-line change above is no longer a narrowing.
    assert_eq!(
        carve::to_html(":: t\n:  d\n\n   # H\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>d</p>\n    <h1 id=\"H\">H</h1>\n  </dd>\n</dl>\n<p>tail</p>"
    );
}
