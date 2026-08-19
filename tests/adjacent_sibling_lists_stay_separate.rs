//! Two adjacent sibling lists written at the same column with matching markers
//! merge on re-parse, so `parse(fmt(x)) == parse(x)` -- PART 11 section 1's
//! primary invariant -- is false for a document the parser reads as two lists
//! (carve#1088).
//!
//! carve#286 spent the marker axis, "emit the marker as authored", which
//! separates them only while the markers DIFFER. When both are `1.` at column 0
//! there is nothing left to preserve and indentation is the axis remaining.
//!
//! One space is the only offset safe for both kinds: a bullet's content column
//! is 2, so two spaces already NESTS. The step is cumulative per list, because
//! a flat +1 leaves the second and third at the same column, merging with each
//! other.

/// Top-level node kinds, as a comma-joined string. Only the two kinds these
/// cases produce are named; anything else would show up as `other` and fail the
/// comparison loudly rather than silently matching.
fn top_kinds(source: &str) -> String {
    carve::parse(source)
        .children
        .iter()
        .map(|block| match block {
            carve::BlockNode::List(_) => "list",
            carve::BlockNode::Paragraph(_) => "paragraph",
            _ => "other",
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn two_ordered_lists_are_separated_by_one_space() {
    let source = "1. a\n\n  1. b\n";
    assert_eq!(top_kinds(source), "list,list");
    assert_eq!(
        carve::render_carve(&carve::parse(source)).unwrap(),
        "1. a\n\n 1. b\n"
    );
    assert_eq!(
        top_kinds(&carve::render_carve(&carve::parse(source)).unwrap()),
        "list,list",
    );
}

#[test]
fn each_further_list_steps_by_one_more_space() {
    let source = "1. a\n\n  1. b\n\n   1. c\n";
    assert_eq!(
        carve::render_carve(&carve::parse(source)).unwrap(),
        "1. a\n\n 1. b\n\n  1. c\n",
    );
    assert_eq!(
        top_kinds(&carve::render_carve(&carve::parse(source)).unwrap()),
        "list,list,list",
    );
}

#[test]
fn the_writer_is_idempotent() {
    let once = carve::render_carve(&carve::parse("1. a\n\n  1. b\n\n   1. c\n")).unwrap();
    let twice = carve::render_carve(&carve::parse(&once)).unwrap();
    assert_eq!(once, twice);
    assert_eq!(twice, carve::render_carve(&carve::parse(&twice)).unwrap());
}

#[test]
fn the_html_is_unchanged() {
    let source = "1. a\n\n  1. b\n";
    let written = carve::render_carve(&carve::parse(source)).unwrap();
    assert_eq!(carve::to_html(source), carve::to_html(&written));
}

/// BOUND, not proof: where the bullet character already separates the lists
/// (carve#286) no space is owed and none is added. Removing the offset entirely
/// leaves this passing - it is here so a fix cannot pass by indenting every
/// list that follows another one.
#[test]
fn nothing_is_added_when_the_marker_already_separates_them() {
    let source = "- a\n\n * b\n";
    assert_eq!(top_kinds(source), "list,list");
    assert_eq!(
        carve::render_carve(&carve::parse(source)).unwrap(),
        "- a\n\n* b\n"
    );
}

/// BOUND: a single list, and two lists with a paragraph between them, are
/// untouched by any offset.
#[test]
fn a_single_list_and_a_separated_pair_are_unchanged() {
    assert_eq!(
        carve::render_carve(&carve::parse("1. a\n1. b\n")).unwrap(),
        "1. a\n2. b\n",
    );
    assert_eq!(
        carve::render_carve(&carve::parse("1. a\n\nx\n\n1. b\n")).unwrap(),
        "1. a\n\nx\n\n1. b\n",
    );
}
