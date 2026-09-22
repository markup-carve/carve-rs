//! An empty term marker opens nothing, and the empty-marker rule ignores
//! trailing whitespace, so `::` and `:: ` are the same line (PART 2,
//! CARVE-P2-025). Below a body's column only a line that opens a block ends the
//! body (CARVE-P2-017), so both fold into the open description as text
//! (carve-rs#1812).

/// Record a row that did not render as expected, so one run names every row
/// that moved instead of stopping at the first.
fn row(source: &str, expected: &str, moved: &mut Vec<String>) {
    let actual = carve::to_html(source);
    let actual = actual.trim_end();
    if actual != expected {
        moved.push(format!(
            "{source:?}\n  expected: {expected}\n  actual:   {actual}"
        ));
    }
}

#[test]
fn a_term_marker_with_trailing_whitespace_folds_into_the_body() {
    let mut moved = Vec::new();
    row(
        ":: t\n: a\n:: \nc\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n::\nc</dd>\n</dl>",
        &mut moved,
    );
    row(
        ":: t\n: a\n::  \nc\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n::\nc</dd>\n</dl>",
        &mut moved,
    );
    row(
        ":: t\n: a\n:: \n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n::</dd>\n</dl>",
        &mut moved,
    );
    row(
        ":: t\n: a\nb\n:: \nc\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\nb\n::\nc</dd>\n</dl>",
        &mut moved,
    );
    row(
        ":: t\n: a\n:: \n: c\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n::</dd>\n  <dd>c</dd>\n</dl>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

/// CONTROLS: the bare marker already folded, the same line at document level
/// is paragraph text, and a term marker with a term still starts an item.
#[test]
fn the_neighbours_of_the_rule_are_unchanged() {
    let mut moved = Vec::new();
    row(
        ":: t\n: a\n::\nc\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n::\nc</dd>\n</dl>",
        &mut moved,
    );
    row(":: \nc\n", "<p>::\nc</p>", &mut moved);
    row(
        ":: t\n: a\n:: u\n: c\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n  <dt>u</dt>\n  <dd>c</dd>\n</dl>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}
