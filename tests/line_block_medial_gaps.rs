//! A line block preserves medial gaps, not only the indent.
//!
//! A medial gap is the inline alignment a caesura or a column of aligned text
//! is made of, and a line block preserves it for the same reason it preserves
//! the indent: the author's per-line layout IS the content. Collapsing it left
//! Old English verse and address blocks rendering as ordinary prose spacing.
//!
//! Only a run of two or more columns counts. A lone inner space stays an
//! ordinary collapsible space so a long line can still wrap between words,
//! which is what keeps this from being "every space is nbsp".
//!
//! Grammar §23 MEDIAL GAPS; carve-php has rendered it this way since its #127.

fn nbsp(n: usize) -> String {
    "&nbsp;".repeat(n)
}

#[test]
fn preserves_an_inner_run_of_two_or_more_spaces() {
    let html = carve::to_html("::: |\nTwo roads    diverged\n:::\n");
    assert!(
        html.contains(&format!("Two roads{}diverged", nbsp(4))),
        "{html}"
    );
}

#[test]
fn leaves_a_single_inner_space_collapsible() {
    let html = carve::to_html("::: |\nTwo roads diverged\n:::\n");
    assert!(html.contains("Two roads diverged"), "{html}");
    assert!(!html.contains("&nbsp;"), "{html}");
}

#[test]
fn preserves_a_trailing_run() {
    let html = carve::to_html("::: |\nword   \n:::\n");
    assert!(html.contains(&format!("word{}", nbsp(3))), "{html}");
}

#[test]
fn keeps_the_indent_and_the_gap_on_the_same_line() {
    let html = carve::to_html("::: |\n  indented    gapped\n:::\n");
    assert!(
        html.contains(&format!("{}indented{}gapped", nbsp(2), nbsp(4))),
        "{html}"
    );
}

#[test]
fn expands_a_medial_tab_to_its_column_stop() {
    // Same tab-stop arithmetic as the indent: a tab advances to the next
    // multiple of four, counted from the column the run starts at.
    let html = carve::to_html("::: |\nab\tcd\n:::\n");
    assert!(html.contains(&format!("ab{}cd", nbsp(2))), "{html}");
}

#[test]
fn parses_inline_content_on_both_sides_of_a_gap() {
    let html = carve::to_html("::: |\n*bold*    /em/\n:::\n");
    assert!(
        html.contains(&format!("<strong>bold</strong>{}<em>em</em>", nbsp(4))),
        "{html}"
    );
}

#[test]
fn resolves_the_placeholder_per_renderer() {
    // Markdown gets real non-breaking spaces, plain text ordinary ones -
    // the same split the indent already used.
    let source = "::: |\nTwo roads    diverged\n:::\n";
    assert!(
        carve::to_markdown(source).contains(&format!("Two roads{}diverged", "\u{a0}".repeat(4)))
    );
    assert!(carve::to_plain_text(source).contains("Two roads    diverged"));
    for out in [
        carve::to_html(source),
        carve::to_markdown(source),
        carve::to_plain_text(source),
    ] {
        assert!(!out.contains('\u{e000}'), "placeholder leaked: {out:?}");
    }
}

#[test]
fn round_trips_a_gapped_line_through_the_writer_byte_for_byte() {
    let source = "::: |\nTwo roads    diverged\nAnd looked   down\n:::\n";
    assert_eq!(carve::to_carve(source), source);
}

#[test]
fn a_lone_escaped_space_still_round_trips_as_written() {
    // A single placeholder mid-line can only be an escaped space, so the writer
    // must not mistake it for layout.
    let source = "::: |\na\\ b\n:::\n";
    assert_eq!(carve::to_carve(source), source);
}

#[test]
fn a_line_holding_a_tab_refuses_a_position() {
    // A rewrite is placeable only while it promotes a SPACE IN PLACE: one space
    // becomes one placeholder, so every column still maps one to one and the
    // node's value is still the source read differently.
    //
    // A tab is not that. It expands to up to four placeholders from one source
    // character, and even where the arithmetic yields exactly one column the
    // CHARACTER changed - `tab\tgap` published a position while its value read
    // `tab gap`, so a consumer asked to highlight that span got source the node
    // does not contain. carve-js publishes no position for the same lines.
    //
    // Per line: the tab-free neighbour keeps its position.
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options("::: |\ntab\tgap\nplain gap\n:::\n", &options);
    let carve::ast::BlockNode::LineBlock(block) = &doc.children[0] else {
        panic!("expected a line block, got {:?}", doc.children[0]);
    };
    let carve::ast::BlockNode::Paragraph(paragraph) = &block.children[0] else {
        panic!("expected a paragraph inside the line block");
    };

    let placed: Vec<(String, bool)> = paragraph
        .children
        .iter()
        .filter_map(|node| match node {
            carve::ast::InlineNode::Text(text) => Some((text.value.clone(), text.pos.is_some())),
            _ => None,
        })
        .collect();

    assert_eq!(
        placed,
        vec![
            ("tab gap".to_string(), false),
            ("plain gap".to_string(), true)
        ],
        "the tab-bearing line must refuse a position, and only that line"
    );
}
