//! PART 12 section 7: `*[TERM]: expansion` is structural only as a direct
//! document child. Inside containers it is paragraph text, so it neither
//! defines nor expands an abbreviation and it still participates in paragraph
//! continuation.

use carve::ast::BlockNode;
use carve::{parse, to_html};

fn types(blocks: &[BlockNode]) -> Vec<&'static str> {
    blocks
        .iter()
        .map(|b| match b {
            BlockNode::Div(_) => "div",
            BlockNode::List(_) => "list",
            BlockNode::BlockQuote(_) => "block_quote",
            BlockNode::Paragraph(_) => "paragraph",
            BlockNode::AbbreviationDef(_) => "abbreviation_def",
            _ => "other",
        })
        .collect()
}

fn contains_abbreviation_def(block: &BlockNode) -> bool {
    match block {
        BlockNode::AbbreviationDef(_) => true,
        BlockNode::BlockQuote(b) => b.children.iter().any(contains_abbreviation_def),
        BlockNode::Div(d) => d.children.iter().any(contains_abbreviation_def),
        BlockNode::Admonition(a) => a.children.iter().any(contains_abbreviation_def),
        BlockNode::Extension(e) => e.children.iter().any(contains_abbreviation_def),
        BlockNode::List(l) => l
            .items
            .iter()
            .any(|item| item.children.iter().any(contains_abbreviation_def)),
        BlockNode::DefinitionList(dl) => dl.items.iter().any(|item| {
            item.definitions
                .iter()
                .any(|def| def.children.iter().any(contains_abbreviation_def))
        }),
        _ => false,
    }
}

fn has_abbreviation_def_anywhere(blocks: &[BlockNode]) -> bool {
    blocks.iter().any(contains_abbreviation_def)
}

#[test]
fn document_level_definition_expands_later_text() {
    assert_eq!(
        to_html("*[HTML]: Hyper Text\n\nThe HTML spec.").trim(),
        "<p>The <abbr title=\"Hyper Text\">HTML</abbr> spec.</p>"
    );

    let doc = parse("*[HTML]: Hyper Text\n\nThe HTML spec.");
    assert_eq!(types(&doc.children), vec!["abbreviation_def", "paragraph"]);
}

#[test]
fn block_quote_definition_shape_is_paragraph_text() {
    assert_eq!(
        to_html("> *[HTML]: Hyper Text\n\nThe HTML spec.").trim(),
        "<blockquote><p>*[HTML]: Hyper Text</p></blockquote>\n<p>The HTML spec.</p>"
    );

    let doc = parse("> *[HTML]: Hyper Text\n\nThe HTML spec.");
    assert_eq!(types(&doc.children), vec!["block_quote", "paragraph"]);
    assert!(!has_abbreviation_def_anywhere(&doc.children));
}

#[test]
fn list_item_definition_shape_is_paragraph_text() {
    assert_eq!(
        to_html("- *[HTML]: Hyper Text\n\nThe HTML spec.").trim(),
        "<ul>\n  <li>*[HTML]: Hyper Text</li>\n</ul>\n<p>The HTML spec.</p>"
    );
}

#[test]
fn div_definition_shape_is_paragraph_text() {
    assert_eq!(
        to_html(":::\n*[HTML]: Hyper Text\n\nbody\n:::\n\nThe HTML spec.").trim(),
        "<div>\n  <p>*[HTML]: Hyper Text</p>\n  <p>body</p>\n</div>\n<p>The HTML spec.</p>"
    );
}

#[test]
fn container_definition_line_holds_an_open_paragraph() {
    assert_eq!(
        to_html("> *[A]: b\nc").trim(),
        "<blockquote><p>*[A]: b\nc</p></blockquote>"
    );
}

#[test]
fn nested_definition_line_does_not_interrupt_an_open_paragraph() {
    assert_eq!(
        to_html("- x\n  *[A]: b").trim(),
        "<ul>\n  <li>x\n*[A]: b</li>\n</ul>"
    );
}

#[test]
fn flush_left_line_after_an_open_item_still_belongs_to_the_item() {
    assert_eq!(
        to_html("- x\n*[A]: b\n\nThe A spec.").trim(),
        "<ul>\n  <li>x\n*[A]: b</li>\n</ul>\n<p>The A spec.</p>"
    );
}

#[test]
fn blank_line_closes_the_item_before_a_document_level_definition() {
    assert_eq!(
        to_html("- x\n\n*[A]: b\n\nThe A spec.").trim(),
        "<ul>\n  <li>x</li>\n</ul>\n<p>The <abbr title=\"b\">A</abbr> spec.</p>"
    );
}

#[test]
fn later_document_level_definition_expands_earlier_text() {
    assert_eq!(
        to_html("The HTML spec.\n\n*[HTML]: Hyper Text").trim(),
        "<p>The <abbr title=\"Hyper Text\">HTML</abbr> spec.</p>"
    );
}
