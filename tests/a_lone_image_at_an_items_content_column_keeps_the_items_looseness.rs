//! markup-carve/carve#1705: a lone block image is a recognized sub-block
//! opener at any authored base at or beyond a list item's content column.
//!
//! PART 9 §17 decides an item's looseness from the BLANK LINE and from what the
//! line under it spells: a blank followed by a plain paragraph loosens (L1), a
//! blank followed by a sub-block opener keeps the item tight (L2). A block
//! image therefore takes L2 and keeps the item tight, including when it is
//! captioned or written past the canonical content column. PART 11 §1c still
//! removes the image paragraph wrapper independently.

use carve::ast::BlockNode;
use carve::{parse, to_html};

fn only_list_tight(src: &str) -> bool {
    let doc = parse(src);
    match doc.children.first() {
        Some(BlockNode::List(list)) => list.tight,
        other => panic!("expected a list at the top level, found {other:?}"),
    }
}

/// The item's looseness reaching the page: a loose item wraps its lead in `<p>`.
fn html_wraps_the_lead(html: &str) -> bool {
    html.contains("<p>t</p>")
}

/// The §1c collapse still happening: the image renders BARE.
fn image_is_bare(html: &str) -> bool {
    html.contains(r#"<img src="a.jpg" alt="A">"#) && !html.contains("<p><img")
}

#[test]
fn a_lone_image_at_the_content_column_keeps_the_item_tight() {
    let src = "- t\n\n  ![A](a.jpg)\n";
    let html = to_html(src);

    assert!(
        only_list_tight(src),
        "a block image is an L2 sub-block opener:\n{html}"
    );
    assert!(
        !html_wraps_the_lead(&html),
        "a tight item does not wrap its lead paragraph:\n{html}"
    );
    // The collapse control. Restoring looseness by re-wrapping the image would
    // satisfy the two assertions above and regress corpus 411.
    assert!(
        image_is_bare(&html),
        "PART 11 section 1c still takes the image's wrapper:\n{html}"
    );
}

#[test]
fn the_same_shape_is_tight_for_every_list_kind() {
    for src in [
        "- t\n\n  ![A](a.jpg)\n",     // unordered
        "1. t\n\n   ![A](a.jpg)\n",   // ordered
        "- [ ] t\n\n  ![A](a.jpg)\n", // task
        "* t\n\n  ![A](a.jpg)\n",     // the other bullet
    ] {
        let html = to_html(src);
        assert!(only_list_tight(src), "loose for {src:?}:\n{html}");
        assert!(!html_wraps_the_lead(&html), "lead <p> for {src:?}:\n{html}");
        assert!(
            image_is_bare(&html),
            "image re-wrapped for {src:?}:\n{html}"
        );
    }
}

#[test]
fn a_captioned_image_at_the_content_column_is_tight_too() {
    // The neighbour the reported shape does not name. An image with a `^ `
    // caption is a `Figure`, which is no more a `Paragraph` than a `BlockImage`
    // is - so it carried the identical defect, and a fix aimed only at the
    // block image would have left it.
    let src = "- t\n\n  ![A](a.jpg)\n  ^ cap\n";
    let html = to_html(src);

    assert!(only_list_tight(src), "captioned image loosened:\n{html}");
    assert!(!html_wraps_the_lead(&html), "lead <p>:\n{html}");
    assert!(
        html.contains("<figcaption>cap</figcaption>") && image_is_bare(&html),
        "the figure is still built, and its image is still bare:\n{html}"
    );
}

#[test]
fn an_over_indented_lone_image_uses_its_authored_base_and_stays_tight() {
    // The same recognized opener one column further in establishes that column
    // as its authored base and keeps the same L2 classification.
    let src = "- t\n\n   ![A](a.jpg)\n";
    let html = to_html(src);

    assert!(only_list_tight(src), "indented image went loose:\n{html}");
    assert!(!html_wraps_the_lead(&html), "lead <p>:\n{html}");
    assert!(image_is_bare(&html), "image re-wrapped:\n{html}");
}

#[test]
fn a_sub_block_opener_under_the_blank_still_keeps_the_item_tight() {
    // The near-miss set: §17 L2 is what the predicate must NOT swallow. Each of
    // these reaches the same test as a non-`Paragraph` first block, and each is
    // `tight: true` on carve-js and carve-php as well - a predicate that
    // loosened on every non-paragraph block would flip all four.
    for (label, src) in [
        ("captioned fence", "- t\n\n  ```\n  code\n  ```\n  ^ cap\n"),
        ("captioned table", "- t\n\n  | a |\n  ^ cap\n"),
        ("captioned quote", "- t\n\n  > q\n  ^ cap\n"),
        (
            "figure container",
            "- t\n\n  ::: figure\n  ![A](a.jpg)\n  :::\n",
        ),
        ("plain fence", "- t\n\n  ```\n  code\n  ```\n"),
        ("heading", "- t\n\n  # h\n"),
        ("sub-list", "- t\n\n  - n\n"),
    ] {
        let html = to_html(src);
        assert!(
            only_list_tight(src),
            "{label} loosened the item, but a blank before a sub-block opener \
             keeps it tight (section 17 L2):\n{html}"
        );
        assert!(
            !html_wraps_the_lead(&html),
            "{label}: a tight item does not wrap its lead:\n{html}"
        );
    }
}

#[test]
fn a_plain_paragraph_under_the_blank_still_loosens() {
    // The other side of the same predicate, so a fix that stopped loosening
    // altogether cannot pass this file.
    let src = "- t\n\n  x\n";
    let html = to_html(src);
    assert!(
        !only_list_tight(src),
        "a second paragraph must loosen:\n{html}"
    );
    assert!(html_wraps_the_lead(&html), "no lead <p>:\n{html}");
}

#[test]
fn an_invisible_block_does_not_change_the_visible_blocks_looseness_class() {
    // markup-carve/carve#630: a comment in front of the second block is skipped,
    // not counted. The predicate is asked of the first VISIBLE block, and that
    // has to survive this change.
    //
    // The image row is the two rules meeting: the comment remains invisible and
    // the block image behind it remains an L2 sub-block opener.
    for (label, src, tight) in [
        ("paragraph", "- t\n\n  %% n\n  x\n", false),
        ("lone image", "- t\n\n  %% n\n  ![A](a.jpg)\n", true),
    ] {
        let html = to_html(src);
        assert!(
            only_list_tight(src) == tight,
            "{label}: the comment changed the visible block's class:\n{html}"
        );
        assert_eq!(html_wraps_the_lead(&html), !tight, "{label}: {html}");
    }
}
