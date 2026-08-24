//! markup-carve/carve-rs#1336. An authored `<p>` kept the layout whitespace on
//! the edges of its inline run, and the writer dropped it, so the two import
//! exits disagreed about characters no reader can act on.
//!
//! Measured on `main` at `6ae35bd6`, for `<p>` newline `  <img>` newline `</p>`:
//!
//! ```text
//! html_to_ast   -> Paragraph { children: [Text(" "), Image, Text(" ")] }
//! html_to_carve -> "![G](g.jpg)\n"
//! ```
//!
//! ## The rule, which is wider than the case that spells it out
//!
//! PART 11 section 7 rules a block whose EVERY character is layout: it builds no
//! node. The principle underneath is that whitespace which is LAYOUT is not
//! content, and the edges of a run that also holds content are the same
//! characters doing the same job. "Whole block" versus "edge of a run" was a
//! proxy for the principle rather than the principle itself.
//!
//! `blocks_at` already trimmed the paragraph it SYNTHESIZES, and the reason its
//! helper gave - that the wrapper is not an element the document contains - is
//! real but is not the deciding one. Taking it as the boundary is what produced
//! the split between the two arms.
//!
//! The alternative was to keep the edges and declare the drop. It was rejected
//! because it would put a SECOND diagnostic row on a shape that already carries
//! one from carve-rs#1331, describing characters that are layout rather than
//! content - and `docs/html-import.md` asks the two exits to AGREE, which
//! trimming achieves and declaring does not. So this closes with no diagnostic
//! anywhere, and every case below asserts the ABSENCE of a row as well as the
//! shape of the tree.
//!
//! ## The boundary that rides along
//!
//! The trim is scoped to the two-character `whitespace` terminal and line
//! terminators, and nothing else. U+00A0, U+202F and U+3000 are CONTENT
//! (markup-carve/carve#1628, measured rather than reasoned: a lone one of them
//! on a line parses to a paragraph, where a lone space or tab line is a blank
//! line).
//!
//! That is not only a rule for the new arm. `trim_edge_whitespace` trimmed with
//! Rust's `str::trim_start` / `trim_end`, and `char::is_whitespace` includes all
//! three - so the SYNTHESIZED arm had been removing them since it was written. A
//! fixture padded with ordinary spaces cannot see the difference, which is why
//! the cases below pad with a no-break space that has to SURVIVE.

use carve::{html_to_ast, html_to_carve, parse, render_html, HtmlImportOptions};

const NBSP: &str = "\u{00a0}";
const NNBSP: &str = "\u{202f}";
const IDEOGRAPHIC: &str = "\u{3000}";

fn tree(html: &str) -> carve::Document {
    html_to_ast(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

fn carve(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

/// Every diagnostic from BOTH exits, because this ruling closes with none.
fn diagnostics(html: &str) -> Vec<String> {
    let options = HtmlImportOptions::default();
    let mut all: Vec<String> = html_to_ast(html, &options)
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| format!("ast {:?}", d.code))
        .collect();
    all.extend(
        html_to_carve(html, &options)
            .expect("import")
            .report
            .diagnostics
            .iter()
            .map(|d| format!("carve {:?}", d.code)),
    );
    all
}

fn text_runs(document: &carve::Document) -> Vec<String> {
    match document.children.first() {
        Some(carve::BlockNode::Paragraph(paragraph)) => paragraph
            .children
            .iter()
            .filter_map(|inline| match inline {
                carve::InlineNode::Text(text) => Some(text.value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// THE REPORTED SHAPE. The padded spelling builds the same tree as the unpadded
/// one, so the two exits agree - and nothing is reported on either.
#[test]
fn an_authored_paragraph_is_trimmed_like_a_synthesized_one() {
    let padded = "<p>\n  hello\n</p>";
    assert_eq!(text_runs(&tree(padded)), vec!["hello".to_string()]);
    assert_eq!(carve(padded), "hello\n");
    assert_eq!(tree(padded), tree("<p>hello</p>"));
    assert_eq!(
        diagnostics(padded),
        Vec::<String>::new(),
        "the trim closes with no row on either exit"
    );
}

/// THE SYNTHESIZED ARM IS THE CONTROL, and it must keep behaving the same way -
/// the point of the ruling is that the two arms stop differing, not that the
/// trim moves from one to the other.
#[test]
fn a_synthesized_wrapper_is_trimmed_the_same_way() {
    let padded = "<div>\n  hello\n</div>";
    assert_eq!(text_runs(&tree(padded)), vec!["hello".to_string()]);
    assert_eq!(carve(padded), "hello\n");
    assert_eq!(diagnostics(padded), Vec::<String>::new());
}

/// THE BOUNDARY, AND THE HALF AN ORDINARY-SPACE FIXTURE CANNOT SEE. U+00A0,
/// U+202F and U+3000 are content (markup-carve/carve#1628), so they survive on
/// an edge - of the AUTHORED paragraph and of the SYNTHESIZED wrapper alike.
/// Rust's own `trim` removes all three, which is what the previous
/// implementation used.
#[test]
fn a_content_space_survives_the_trim_on_either_arm() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        for (label, html) in [
            ("authored", format!("<p>{space}text{space}</p>")),
            ("synthesized", format!("<div>{space}text{space}</div>")),
        ] {
            assert_eq!(
                text_runs(&tree(&html)),
                vec![format!("{space}text{space}")],
                "{label}: {:?} is content and must survive the trim",
                space
            );
            assert_eq!(carve(&html), format!("{space}text{space}\n"), "{label}");
            assert_eq!(diagnostics(&html), Vec::<String>::new(), "{label}");
        }
    }
}

/// AND ONE OF THEM ALONE STILL BUILDS A PARAGRAPH, ON EITHER ARM. Whether a
/// block holding nothing else is dropped is decided by `is_layout_only` on the
/// authored arm and by `visible` on the synthesized one, and carve#1628 put
/// these three on the CONTENT side of that line. The authored arm already had
/// it right; the synthesized one read `str::trim`, so a `<div>` holding one
/// no-break space built no paragraph at all and the document came back EMPTY,
/// with no diagnostic - content deleted outright for the exact character the
/// ruling had just pinned.
#[test]
fn a_block_holding_only_a_content_space_survives_on_either_arm() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        for (label, html) in [
            ("authored", format!("<p>{space}</p>")),
            ("synthesized", format!("<div>{space}</div>")),
        ] {
            assert_eq!(
                text_runs(&tree(&html)),
                vec![space.to_string()],
                "{label}: {space:?} is content and must build a paragraph"
            );
            assert_eq!(carve(&html), format!("{space}\n"), "{label}");
            assert_eq!(diagnostics(&html), Vec::<String>::new(), "{label}");
        }
    }
}

/// THE LAYOUT-ONLY PARAGRAPH IS STILL DROPPED, AND STILL SAYS SO. The trim runs
/// after that test, so section 7's own case is untouched: an element the input
/// had contributes nothing, and `element-dropped` declares it.
#[test]
fn a_layout_only_paragraph_is_still_dropped_with_its_row() {
    let html = "<p>  \n\t</p>";
    assert!(tree(html).children.is_empty());
    assert_eq!(
        diagnostics(html),
        vec![
            "ast ElementDropped".to_string(),
            "carve ElementDropped".to_string()
        ]
    );
}

/// WITH carve-rs#1331, AND EXACTLY ONE ROW RATHER THAN TWO. The padded spelling
/// of a lone-image paragraph loses the paragraph, which is declared - and loses
/// nothing else, which is why no second row describes the spaces.
#[test]
fn the_padded_lone_image_paragraph_carries_one_row_and_no_more() {
    let html = "<p>\n  <img src=\"g.jpg\" alt=\"G\">\n</p>";
    assert_eq!(
        diagnostics(html),
        vec!["carve StructureUnspellable".to_string()],
        "the writing exit declares the paragraph, and the tree exit says nothing"
    );
    assert_eq!(tree(html), tree(r#"<p><img src="g.jpg" alt="G"></p>"#));
    assert_eq!(carve(html), "![G](g.jpg)\n");
}

/// WITH carve-rs#1334. A padded BARE image is a block image with no paragraph
/// anywhere, so the two exits agree and nothing is owed.
#[test]
fn a_padded_bare_image_is_a_block_image_with_no_row() {
    let html = "<div>\n  <img src=\"g.jpg\" alt=\"G\">\n</div>";
    assert_eq!(
        format!("{:?}", tree(html).children),
        format!("{:?}", tree(r#"<img src="g.jpg" alt="G">"#).children)
    );
    assert!(!format!("{:?}", tree(html)).contains("Paragraph"));
    assert_eq!(diagnostics(html), Vec::<String>::new());
}

/// THE TWO EXITS AGREE ON THE PADDED SHAPES, which is what
/// `docs/html-import.md` asks of them and what reading 2 could not deliver.
///
/// Compared through the RENDERER rather than by node equality, because a parsed
/// tree carries source positions an imported one has no way to have - the
/// question here is whether the two documents SAY the same thing.
#[test]
fn both_exits_agree_on_every_padded_shape() {
    for html in [
        "<p>\n  hello\n</p>".to_string(),
        "<div>\n  hello\n</div>".to_string(),
        format!("<p>{NBSP}text{NBSP}</p>"),
        "<blockquote>\n  <p>\n  hello\n</p>\n</blockquote>".to_string(),
    ] {
        let written = carve(&html);
        assert_eq!(
            render_html(&parse(&written)).expect("render"),
            render_html(&tree(&html)).expect("render"),
            "{html}"
        );
        assert_eq!(diagnostics(&html), Vec::<String>::new(), "{html}");
    }
}
