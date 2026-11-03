//! carve#4865: a hoisted container definition keeps its authored order, so the
//! last definition of a term wins with and without positions.

fn expansion(source: &str) -> String {
    let html = carve::to_html(source);
    let at = html.find("<abbr title=\"").expect("an abbreviation") + 13;
    html[at..at + html[at..].find('"').unwrap()].to_string()
}

#[test]
fn a_later_document_definition_beats_an_earlier_quoted_one() {
    assert_eq!(expansion("> *[A]: first\n\n*[A]: second\n\nA\n"), "second");
}

#[test]
fn a_later_quoted_definition_beats_an_earlier_document_one() {
    assert_eq!(expansion("*[A]: first\n\n> *[A]: second\n\nA\n"), "second");
}

#[test]
fn a_later_item_definition_beats_an_earlier_note_one() {
    assert_eq!(
        expansion("[^n]: x\n  *[A]: first\n\n- y\n  *[A]: second\n\nA[^n]\n"),
        "second"
    );
}

#[test]
fn a_probed_container_definition_is_hoisted_once() {
    for source in [
        "- a\n  - b\n    *[A]: x\n\nA\n",
        ":: t\n:  - b\n     *[A]: x\n\nA\n",
        "- a\n  :: t\n  :  *[A]: x\n\nA\n",
    ] {
        let doc = carve::parse(source);
        let count = doc
            .children
            .iter()
            .filter(|block| matches!(block, carve::ast::BlockNode::AbbreviationDef(_)))
            .count();
        assert_eq!(count, 1, "{source:?}");
    }
}

#[test]
fn a_definition_shaped_run_in_prose_does_not_move_the_order() {
    assert_eq!(
        expansion("mention *[A]: second\n\n> *[A]: first\n\n*[A]: second\n\nA\n"),
        "second"
    );
}

#[test]
fn an_indented_document_definition_keeps_its_place_in_the_order() {
    // Carve has no indented code: the indented line defines too.
    assert_eq!(
        expansion("    *[A]: first\n\n*[A]: second\n\n> *[A]: first\n\nA\n"),
        "first"
    );
}

fn block_types(doc: &carve::ast::Document) -> Vec<&'static str> {
    doc.children
        .iter()
        .map(|block| match block {
            carve::ast::BlockNode::AbbreviationDef(_) => "abbreviation_def",
            carve::ast::BlockNode::BlockQuote(_) => "block_quote",
            carve::ast::BlockNode::List(_) => "list",
            carve::ast::BlockNode::Paragraph(_) => "paragraph",
            _ => "other",
        })
        .collect()
}

#[test]
fn a_hoisted_definition_stands_after_its_container_without_positions() {
    // CARVE-P12-070: document order by source position, whether or not the
    // caller asked for positions.
    for (source, expected) in [
        (
            "> *[HTML]: Hyper Text\n\nThe HTML spec.\n",
            vec!["block_quote", "abbreviation_def", "paragraph"],
        ),
        (
            "- a\n  *[A]: b\n\nA here.\n",
            vec!["list", "abbreviation_def", "paragraph"],
        ),
    ] {
        let plain = carve::parse(source);
        let positioned =
            carve::parse_with_options(source, &carve::Options::default().with_positions(true));
        assert_eq!(block_types(&plain), expected, "{source:?}");
        assert_eq!(block_types(&positioned), expected, "{source:?}");
    }
}

#[test]
fn a_quoted_definition_starts_at_its_star_with_lone_carriage_returns() {
    let doc = carve::parse_with_options(
        "x\r\r> *[A]: b\r\rA\r",
        &carve::Options::default().with_positions(true),
    );
    let pos = doc
        .children
        .iter()
        .find_map(|block| match block {
            carve::ast::BlockNode::AbbreviationDef(def) => def.pos.clone(),
            _ => None,
        })
        .expect("a positioned definition");
    assert_eq!((pos.start_line, pos.start_column), (3, 3));
}
