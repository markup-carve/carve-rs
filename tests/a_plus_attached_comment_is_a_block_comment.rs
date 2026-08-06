//! A comment attached by a `+` continuation marker is a BLOCK comment, the
//! same node the identical line produces at top level (carve-rs#678).
//!
//! `parse_continuation_block` hands the attached lines to the real block
//! parser, so the two paths ought to agree. They did not: the multi-block
//! parser has an arm for a `%%` line, and the single-block one does not, so the
//! line fell through to a paragraph and the inline parser made an inline
//! comment inside it.
//!
//! The rendered HTML agrees either way - a comment renders nothing - so only
//! the tree diverged, which is why nothing caught it.

use carve::{ast::BlockNode, parse};

fn item_blocks(source: &str) -> Vec<String> {
    let doc = parse(source);
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected a list, got {:?}", doc.children[0]);
    };

    list.items[0]
        .children
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(_) => "paragraph".to_string(),
            BlockNode::Comment(c) => format!("comment(block={})", c.block),
            other => format!("{other:?}")
                .split('(')
                .next()
                .unwrap()
                .to_lowercase(),
        })
        .collect()
}

#[test]
fn a_plus_attached_comment_is_a_block_comment() {
    let blocks = item_blocks("- a\n+\n%% c\n\nx\n");

    assert_eq!(blocks, vec!["paragraph", "comment(block=false)"]);
}

#[test]
fn it_matches_what_the_same_line_produces_at_top_level() {
    // The deciding comparison, and the one that makes this a defect rather
    // than a preference: the same text, through the same parser, two shapes.
    let doc = parse("%% c\n\nx\n");
    let BlockNode::Comment(top) = &doc.children[0] else {
        panic!("expected a comment at top level, got {:?}", doc.children[0]);
    };

    assert_eq!(top.content, "c");
    assert_eq!(
        item_blocks("- a\n+\n%% c\n\nx\n")[1],
        "comment(block=false)"
    );
}

#[test]
fn a_plus_attached_comment_fence_is_a_block_comment_too() {
    // The other spelling of the same construct, which shares the arm.
    let blocks = item_blocks("- a\n+\n%%%\nc\nd\n%%%\n\nx\n");

    assert_eq!(blocks, vec!["paragraph", "comment(block=true)"]);
}

#[test]
fn a_plus_attached_paragraph_is_still_a_paragraph() {
    // The control. Turning every `+`-attached block into a comment would
    // satisfy the assertions above.
    let blocks = item_blocks("- a\n+\nb\n\nx\n");

    assert_eq!(blocks, vec!["paragraph", "paragraph"]);
}
