//! markup-carve/carve-rs#1358: a lone image at a list item's content column
//! must not make the list tight.
//!
//! PART 9 §17 decides an item's looseness from the BLANK LINE and from what the
//! line under it spells: a blank followed by a plain paragraph loosens (L1), a
//! blank followed by a sub-block opener keeps the item tight (L2). That
//! decision is settled before anything is known about what the second block
//! renders as.
//!
//! PART 11 §1c takes the `<p>` WRAPPER off a block whose whole content is one
//! node that spells it away, and this engine implements the image half of that
//! in the TREE: a lone image at its container's content column is a
//! `BlockImage`, and one carrying a `^ ` caption is a `Figure`
//! (markup-carve/carve#1660, markup-carve/carve#1677). The looseness test asked
//! `is the first block a Paragraph`, so the collapse silently answered a
//! question that belongs to the blank line above it - the clause takes the
//! wrapper, never the item's looseness.
//!
//! WHAT MAKES THESE FIXTURES ABLE TO FAIL. Each shape is asserted on BOTH faces
//! of the one defect, because either alone passes while the other stays wrong:
//! `list.tight` on the tree, and the `<p>` on the rendered HTML. Every shape
//! also carries the COLLAPSE CONTROL - the image must still render bare, with
//! no `<p>` around it - so a "fix" that restored looseness by undoing the §1c
//! collapse fails here rather than passing and regressing corpus 411.
//!
//! Measured against carve-js 99b28ab and carve-php 38de559, which publish
//! `tight: false` and `<p>t</p>` for every LOOSE shape below and `tight: true`
//! for every TIGHT one.

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
fn a_lone_image_at_the_content_column_leaves_the_item_loose() {
    let src = "- t\n\n  ![A](a.jpg)\n";
    let html = to_html(src);

    assert!(
        !only_list_tight(src),
        "the blank line loosened the item; the image below it cannot re-tighten \
         the list. tree said tight, HTML was:\n{html}"
    );
    assert!(
        html_wraps_the_lead(&html),
        "a loose item wraps its lead paragraph:\n{html}"
    );
    // The collapse control. Restoring looseness by re-wrapping the image would
    // satisfy the two assertions above and regress corpus 411.
    assert!(
        image_is_bare(&html),
        "PART 11 section 1c still takes the image's wrapper:\n{html}"
    );
}

#[test]
fn the_same_shape_is_loose_for_every_list_kind() {
    for src in [
        "- t\n\n  ![A](a.jpg)\n",     // unordered
        "1. t\n\n   ![A](a.jpg)\n",   // ordered
        "- [ ] t\n\n  ![A](a.jpg)\n", // task
        "* t\n\n  ![A](a.jpg)\n",     // the other bullet
    ] {
        let html = to_html(src);
        assert!(!only_list_tight(src), "still tight for {src:?}:\n{html}");
        assert!(
            html_wraps_the_lead(&html),
            "no lead <p> for {src:?}:\n{html}"
        );
        assert!(
            image_is_bare(&html),
            "image re-wrapped for {src:?}:\n{html}"
        );
    }
}

#[test]
fn a_captioned_image_at_the_content_column_is_loose_too() {
    // The neighbour the reported shape does not name. An image with a `^ `
    // caption is a `Figure`, which is no more a `Paragraph` than a `BlockImage`
    // is - so it carried the identical defect, and a fix aimed only at the
    // block image would have left it.
    let src = "- t\n\n  ![A](a.jpg)\n  ^ cap\n";
    let html = to_html(src);

    assert!(
        !only_list_tight(src),
        "captioned image still tight:\n{html}"
    );
    assert!(html_wraps_the_lead(&html), "no lead <p>:\n{html}");
    assert!(
        html.contains("<figcaption>cap</figcaption>") && image_is_bare(&html),
        "the figure is still built, and its image is still bare:\n{html}"
    );
}

#[test]
fn an_indented_lone_image_was_already_loose_and_stays_loose() {
    // The tell that found the defect: the SAME image one column further in kept
    // the item loose all along, because an indented lone image stays a
    // paragraph in the tree (markup-carve/carve#1660). Two columns of the same
    // document cannot disagree about a blank line.
    let src = "- t\n\n   ![A](a.jpg)\n";
    let html = to_html(src);

    assert!(!only_list_tight(src), "indented image went tight:\n{html}");
    assert!(html_wraps_the_lead(&html), "no lead <p>:\n{html}");
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
fn an_invisible_block_under_the_blank_does_not_cancel_it() {
    // markup-carve/carve#630: a comment in front of the second block is skipped,
    // not counted. The predicate is asked of the first VISIBLE block, and that
    // has to survive this change.
    //
    // The IMAGE row is the two rules meeting, and it was broken too: the comment
    // was skipped correctly and the block behind it was a `BlockImage`, so the
    // item went tight for the same reason the bare shape did. carve-js and
    // carve-php publish `tight: false` for both.
    for (label, src) in [
        ("paragraph", "- t\n\n  %% n\n  x\n"),
        ("lone image", "- t\n\n  %% n\n  ![A](a.jpg)\n"),
    ] {
        let html = to_html(src);
        assert!(
            !only_list_tight(src),
            "{label}: a comment in front of the second block must not \
             re-tighten:\n{html}"
        );
        assert!(html_wraps_the_lead(&html), "{label}: no lead <p>:\n{html}");
    }
}
