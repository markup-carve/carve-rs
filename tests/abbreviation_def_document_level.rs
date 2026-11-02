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
        BlockNode::ExtensionCarrier(e) => e.children.iter().any(contains_abbreviation_def),
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
fn block_quote_definition_defines_in_the_document() {
    // carve#4865: a definition in every container, with document scope.
    assert_eq!(
        to_html("> *[HTML]: Hyper Text\n\nThe HTML spec.").trim(),
        "<blockquote>\n\n</blockquote>\n<p>The <abbr title=\"Hyper Text\">HTML</abbr> spec.</p>"
    );

    let doc = parse("> *[HTML]: Hyper Text\n\nThe HTML spec.");
    assert_eq!(types(&doc.children)[0], "block_quote");
    assert!(has_abbreviation_def_anywhere(&doc.children));
}

#[test]
fn list_item_definition_defines_in_the_document() {
    assert_eq!(
        to_html("- *[HTML]: Hyper Text\n\nThe HTML spec.").trim(),
        "<ul>\n  <li></li>\n</ul>\n<p>The <abbr title=\"Hyper Text\">HTML</abbr> spec.</p>"
    );
}

#[test]
fn div_definition_defines_in_the_document() {
    assert_eq!(
        to_html(":::\n*[HTML]: Hyper Text\n\nbody\n:::\n\nThe HTML spec.").trim(),
        "<div>\n  <p>body</p>\n</div>\n<p>The <abbr title=\"Hyper Text\">HTML</abbr> spec.</p>"
    );
}

#[test]
fn container_definition_line_is_invisible() {
    assert_eq!(
        to_html("> *[A]: b\nc").trim(),
        "<blockquote>\n\n</blockquote>\n<p>c</p>"
    );
}

#[test]
fn nested_definition_line_interrupts_an_open_paragraph() {
    assert_eq!(
        to_html("- x\n  *[A]: b").trim(),
        "<ul>\n  <li>x</li>\n</ul>"
    );
}

#[test]
fn flush_left_definition_after_an_open_item_belongs_to_the_document() {
    assert_eq!(
        to_html("- x\n*[A]: b\n\nThe A spec.").trim(),
        "<ul>\n  <li>x</li>\n</ul>\n<p>The <abbr title=\"b\">A</abbr> spec.</p>"
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
