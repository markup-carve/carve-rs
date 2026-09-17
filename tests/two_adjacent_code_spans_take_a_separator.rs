//! Two touching backtick runs merge into one, so two adjacent code spans, or a
//! code span and a raw inline, are written with an empty delimited comment
//! between them (PART 11 §10k N3, ruling markup-carve/carve-js#1818,
//! markup-carve/carve-rs#1743).

use carve::{
    html_to_carve, parse, render_carve, to_carve, BlockNode, Document, HtmlImportOptions,
    InlineNode,
};

/// Parse `source` and take every empty delimited comment out of its paragraph.
fn without_empty_comments(source: &str) -> Document {
    let mut doc = parse(source);
    for block in &mut doc.children {
        if let BlockNode::Paragraph(paragraph) = block {
            paragraph.children.retain(
                |node| !matches!(node, InlineNode::Comment(c) if c.delimited && c.content.trim().is_empty()),
            );
        }
    }
    doc
}

fn written(source: &str) -> String {
    render_carve(&without_empty_comments(source)).unwrap()
}

macro_rules! separated {
    ($($name:ident: $source:expr,)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn takes_the_separator() {
                    assert_eq!(written($source), format!("{}\n", $source));
                }

                #[test]
                fn reads_back_as_the_tree_it_was_written_from() {
                    assert_eq!(without_empty_comments(&written($source)).children, without_empty_comments($source).children);
                }

                #[test]
                fn is_a_fixed_point_of_fmt() {
                    let bytes = written($source);
                    assert_eq!(to_carve(&bytes), bytes);
                }
            }
        )*
    };
}

separated! {
    two_code_spans: "`a`{%  %}`b`",
    a_code_span_and_a_raw_inline: "`a`{%  %}`<b>`{=html}",
    three_code_spans: "`a`{%  %}`b`{%  %}`c`",
    code_spans_between_text: "t`a`{%  %}`b`z",
    an_inline_literal_before_a_code_span: "!`a`{%  %}`b`",
}

// Controls: the runs do not touch, so no separator is written.
macro_rules! unseparated {
    ($($name:ident: $source:expr,)*) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(written($source), format!("{}\n", $source));
            }
        )*
    };
}

unseparated! {
    a_raw_inline_before_a_code_span: "`<b>`{=html}`a`",
    an_attribute_block_between: "`a`{.x}`b`",
    a_character_between: "`a`!`b`",
    an_escaped_backtick_before: "x\\``b`",
}

#[test]
fn the_html_importer_writes_the_same_bytes() {
    let result = html_to_carve(
        "<p><code>a</code><code>b</code></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "`a`{%  %}`b`\n");
    assert!(result.report.diagnostics.is_empty());
}
