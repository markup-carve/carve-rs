//! `fmt` writes a footnote body at TWO spaces, the body's own column.
//!
//! The writer used THREE. Three is legal continuation - §16 is `space, space,
//! {whitespace}` - but the body's blocks are read relative to the body's own
//! column, and an indented block opener does not open a block. So the structure
//! the indent carried was flattened on the way back in, breaking PART 11 §1: a
//! table in a note body came back as a paragraph (carve-rs#617).
//!
//! NOT only tables. Seven body shapes broke at three and hold at two: table, code
//! fence, block quote, heading, div, nested list, definition list.
//!
//! A BULLET LIST is the exception, and it is why this survived: a bullet opens a
//! list at any indent, so the block body shape authors write most often
//! round-tripped fine and nothing complained.
//!
//! The corpus pins how an AUTHORED body parses (`203-a-footnote-body-holds-blocks`
//! is exactly the right shape, at two spaces). Nothing pinned that the writer's
//! own output parses back the same way, which is how all three engines agreed on a
//! form their own readers could not read. So these assertions go per body SHAPE.
//!
//! carve-js writes three too and its round trip passes anyway, because its PARSER
//! accepts a table at three where this engine, carve-php and the executable spec
//! all read a paragraph (markup-carve/carve-js#677). It is not the oracle here.

use carve::{parse, render_carve, to_html};

fn fmt(src: &str) -> String {
    render_carve(&parse(src)).expect("render")
}

/// `intro` plus a body, then a reference so the note is used.
fn document(body: &str) -> String {
    format!("[^a]: intro\n\n{body}\nsee[^a]\n")
}

/// Every block shape a note body can hold. The last two round-tripped at three
/// spaces as well, and are kept so a narrowed fix still has to keep them working.
fn shapes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("table", "  | a |\n  | - |\n  | b |\n"),
        ("code fence", "  ```\n  code\n  ```\n"),
        ("block quote", "  > quoted\n"),
        ("heading", "  # H\n"),
        ("div", "  :::\n  body\n  :::\n"),
        ("nested list", "  - one\n    - deep\n"),
        ("definition list", "  :: term\n  :  def\n"),
        ("bullet list", "  - one\n  - two\n"),
        ("second paragraph", "  second para\n"),
    ]
}

#[test]
fn every_body_shape_round_trips_through_fmt() {
    let broken: Vec<&str> = shapes()
        .into_iter()
        .filter(|(_, body)| {
            let src = document(body);
            to_html(&fmt(&src)) != to_html(&src)
        })
        .map(|(name, _)| name)
        .collect();
    assert_eq!(broken, Vec::<&str>::new(), "body shapes lost their blocks");
}

#[test]
fn the_body_is_written_at_two_spaces() {
    // The round trip is the claim; this pins the MECHANISM, so a later change
    // that made the parser lenient instead would not read as a fix here.
    // The claim is about the body's OUTERMOST line. Lines deeper than that are
    // the body's own blocks reading their own indentation - a nested list's inner
    // item is legitimately at four - so this asserts the MINIMUM is two, not that
    // nothing exceeds it.
    for (name, body) in shapes() {
        let out = fmt(&document(body));
        let min_indent = out
            .lines()
            .filter(|l| l.starts_with(' '))
            .map(|l| l.len() - l.trim_start_matches(' ').len())
            .min();
        assert_eq!(min_indent, Some(2), "{name}: body column wrong: {out:?}");
    }
}

#[test]
fn a_three_space_body_is_still_read_as_a_paragraph() {
    // WHY two rather than three, stated as a fact about the READER: at three the
    // table opener is indented and does not open. If this starts failing, the
    // parse rule moved and the writer's column can be revisited.
    let html = to_html("[^a]: intro\n\n   | a |\n   | - |\n   | b |\n\nsee[^a]\n");
    assert!(!html.contains("<table>"), "{html}");
}

#[test]
fn the_two_space_form_is_read_as_a_table() {
    // The other half of that boundary - otherwise the assertion above would pass
    // just as well if tables were broken outright.
    let html = to_html("[^a]: intro\n\n  | a |\n  | - |\n  | b |\n\nsee[^a]\n");
    assert!(html.contains("<table>"), "{html}");
}

#[test]
fn an_inline_only_body_is_unchanged() {
    // No continuation lines, so nothing to indent. The shape most notes use.
    let src = "[^a]: just text\n\nsee[^a]\n";
    assert_eq!(to_html(&fmt(src)), to_html(src));
    assert!(fmt(src).contains("[^a]: just text"), "{:?}", fmt(src));
}

#[test]
fn a_wrapped_inline_body_still_continues() {
    // A plain continuation line, never broken: it must stay part of the body
    // rather than becoming a sibling paragraph. The soft break is preserved, so
    // the two words are one paragraph split by a newline, not joined by a space.
    let src = "[^a]: one\n  two\n\nsee[^a]\n";
    assert_eq!(to_html(&fmt(src)), to_html(src));
    assert!(
        to_html(&fmt(src)).contains("one\ntwo"),
        "{}",
        to_html(&fmt(src))
    );
}
