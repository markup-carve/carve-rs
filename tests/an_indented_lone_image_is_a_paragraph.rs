//! markup-carve/carve#1660: an indented LONE image is a paragraph holding an
//! inline image, not a block image.
//!
//! `docs/divergence-from-djot.md` section 15 says a top-level block opener must
//! start at column 0, and gives the worked example ` # H` rendering
//! `<p># H</p>`. A block image is a top-level block construct, so an indented
//! one cannot be one - the leading space cannot be inert for an image and
//! decisive for a heading. This engine and carve-php read it the other way; the
//! ruling moved them, against the engine split two to one.
//!
//! WHAT MAKES THESE FIXTURES ABLE TO FAIL, and it is the whole lesson of the
//! ticket: the discriminating shape is a lone indented image with NO
//! CONTINUATION LINE. The promotion only fires on a paragraph whose entire
//! content is one image, so every pinned document that indents an image - all
//! three of `158-indented-image-and-caption-stay-literal` - carries a caption
//! line that keeps it from firing at all. Three engines held two readings with
//! every gate green because no fixture anywhere had one line.
//!
//! AND THE HTML CANNOT SEE IT EITHER, which is why these are asserted on the
//! TREE. A paragraph whose whole content is one image renders as a bare `<img>`
//! with no `<p>` wrapper, at every column - so both readings emit the same
//! bytes and an HTML fixture pins nothing. Each assertion below therefore states
//! the tree AND the identical HTML: a fix that changed the rendering to
//! `<p><img></p>` would be a different bug, and the HTML halves are what catch
//! it.

use carve::ast::{BlockNode, InlineNode};
use carve::{parse, to_html};

fn types(blocks: &[BlockNode]) -> Vec<&'static str> {
    blocks
        .iter()
        .map(|b| match b {
            BlockNode::Paragraph(_) => "paragraph",
            BlockNode::BlockImage(_) => "image",
            BlockNode::Figure(_) => "figure",
            BlockNode::BlockQuote(_) => "block_quote",
            BlockNode::LinkReferenceDefinition(_) => "link_reference_definition",
            _ => "other",
        })
        .collect()
}

fn sole_image_paragraph(block: &BlockNode) -> bool {
    matches!(block, BlockNode::Paragraph(p)
        if p.children.len() == 1 && matches!(p.children[0], InlineNode::Image(_)))
}

#[test]
fn an_indented_lone_image_is_a_paragraph_and_a_flush_left_one_is_a_block_image() {
    let doc = parse(" ![a](u)\n");
    assert_eq!(types(&doc.children), vec!["paragraph"]);
    assert!(
        sole_image_paragraph(&doc.children[0]),
        "the paragraph must hold the image INLINE - a paragraph holding something \
         else would pass the type assertion and mean nothing"
    );

    // The control. Without it, a reader that never promotes anything passes the
    // assertion above and fails nothing else in this file.
    assert_eq!(types(&parse("![a](u)\n").children), vec!["image"]);
}

#[test]
fn every_indent_width_reads_the_same_way() {
    // One space is the boundary; three is the width the ticket measured. A fix
    // keyed to a particular width would pass one of these and fail the other.
    for indent in [" ", "  ", "   ", "\t"] {
        let source = format!("{indent}![a](u)\n");
        assert_eq!(
            types(&parse(&source).children),
            vec!["paragraph"],
            "indent {indent:?} did not fold"
        );
    }
}

#[test]
fn the_html_is_identical_at_both_columns() {
    // Stated as an assertion rather than a comment, because it is the reason the
    // corpus could not catch this and the reason the fix has two halves: the
    // tree stops promoting, and the renderer starts collapsing.
    let flush = to_html("![a](u)\n");
    let indented = to_html(" ![a](u)\n");
    assert_eq!(indented, flush);
    assert_eq!(indented.trim(), "<img src=\"u\" alt=\"a\">");
}

#[test]
fn the_reference_spelling_folds_the_same_way() {
    // A reference image is never a syntactic block image: it arrives as a
    // paragraph and is promoted afterwards or not at all, so it reaches the same
    // pass by a different route. A fix applied only to the syntactic path would
    // leave this one promoting.
    let source = " ![a][r]\n\n[r]: u\n";
    let doc = parse(source);
    assert_eq!(
        types(&doc.children),
        vec!["paragraph", "link_reference_definition"]
    );
    assert!(sole_image_paragraph(&doc.children[0]));
    assert_eq!(to_html(source).trim(), "<img src=\"u\" alt=\"a\">");
}

#[test]
fn the_column_is_the_containers_content_column_not_column_zero() {
    // A quote body one past its content column folds exactly as a top-level
    // indented image does. A fix that tested the literal source column would
    // pass every assertion above and fail here.
    let at_source = "> ![a](u)\n";
    let past_source = ">  ![a](u)\n";
    let at_column = parse(at_source);
    let past_column = parse(past_source);
    let BlockNode::BlockQuote(at) = &at_column.children[0] else {
        panic!("expected a quote")
    };
    let BlockNode::BlockQuote(past) = &past_column.children[0] else {
        panic!("expected a quote")
    };
    assert_eq!(types(&at.children), vec!["image"]);
    assert_eq!(types(&past.children), vec!["paragraph"]);

    // And the quote still uses the EXPANDED layout for both, rather than the
    // compact `<blockquote><p>…</p></blockquote>` form a lone paragraph would
    // otherwise take. This is the half a tree-only fix silently breaks.
    assert_eq!(to_html(past_source), to_html(at_source));
    assert_eq!(
        to_html(past_source).trim(),
        "<blockquote>\n  <img src=\"u\" alt=\"a\">\n</blockquote>"
    );
}

#[test]
fn a_caption_line_still_keeps_the_indented_pair_literal() {
    // Corpus `158-indented-image-and-caption-stay-literal`, restated here as the
    // control that bounds the change: the FIGURE promotion stays gated on the
    // content column at render time as well as at parse time, so lifting the
    // gate for the lone image did not start building figures nobody wrote.
    let source = " ![Apollo](a.jpg)\n ^ Figure 1: moon\n";
    assert_eq!(types(&parse(source).children), vec!["paragraph"]);
    assert_eq!(
        to_html(source).trim(),
        "<p><img src=\"a.jpg\" alt=\"Apollo\">\n^ Figure 1: moon</p>"
    );

    // Its flush-left twin is a figure, which is what makes the row above a
    // measurement rather than a tautology.
    assert_eq!(
        types(&parse("![Apollo](a.jpg)\n^ Figure 1: moon\n").children),
        vec!["figure"]
    );
}

#[test]
fn an_unresolved_reference_image_is_not_promoted_at_any_column() {
    // The pre-existing carve-out, asserted so the new gate cannot be read as the
    // only reason a paragraph survives here.
    for source in ["![a][missing]\n", " ![a][missing]\n"] {
        assert_eq!(
            types(&parse(source).children),
            vec!["paragraph"],
            "{source:?}"
        );
    }
}
