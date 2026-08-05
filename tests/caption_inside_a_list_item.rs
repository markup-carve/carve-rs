//! A `^ ` caption attaches to the block above it wherever that block sits.
//!
//! It worked at the top level, inside a block quote and inside a div, and NOT
//! inside a list item - there the caption line rendered as literal text, where the
//! executable spec, carve-js and carve-php all built a `<figure>` (carve-rs#610).
//!
//! The parse was never wrong. The promotion in `promote_block_images` already
//! recursed into list items and already accepted a direct image; it was gated on
//! the paragraph's `at_content_column`, and the item's LEAD paragraph was built
//! with `..Default::default()`, so that flag was false for every list item in
//! every document. The gate could not pass.
//!
//! That is worth stating as a shape rather than a line number: the flag is set in
//! `parse_paragraph` by looking at the line, and the item lead path builds its
//! paragraph by hand instead of calling it. A hand-built path that fills in some
//! of a struct's fields keeps missing the ones that carry meaning - the same
//! reason the executable spec dropped `battrs` (carve#626) and then `caption`
//! (carve#693) at its own hand-built item path.
//!
//! So these assertions go per CONTAINER and per TARGET, not per field.

use carve::to_html;

const IMAGE: &str = "![alt](/i.png)";

#[test]
fn a_caption_attaches_inside_a_list_item() {
    let html = to_html(&format!("- {IMAGE}\n  ^ Figure 1: caption\n"));
    assert!(
        html.contains("<figcaption>Figure 1: caption</figcaption>"),
        "{html}"
    );
    // The caption text must not ALSO survive as literal item text.
    assert!(!html.contains("^ Figure 1: caption"), "{html}");
}

#[test]
fn and_inside_an_ordered_item() {
    let html = to_html(&format!("1. {IMAGE}\n   ^ ord cap\n"));
    assert!(html.contains("<figcaption>ord cap</figcaption>"), "{html}");
}

#[test]
fn and_inside_a_nested_item() {
    let html = to_html(&format!("- outer\n  - {IMAGE}\n    ^ nested cap\n"));
    assert!(
        html.contains("<figcaption>nested cap</figcaption>"),
        "{html}"
    );
}

#[test]
fn and_for_the_other_captionable_targets_in_an_item() {
    // A caption is not image-only. These passed before the fix as well, and are
    // pinned here so a narrowed change cannot trade one target for another.
    let quote = to_html("- > quoted\n  ^ quote cap\n");
    assert!(
        quote.contains("<figcaption>quote cap</figcaption>"),
        "{quote}"
    );

    let code = to_html("- ```\n  code\n  ```\n  ^ code cap\n");
    assert!(code.contains("<figcaption>code cap</figcaption>"), "{code}");
}

#[test]
fn and_still_at_the_top_level_and_in_a_div() {
    // The positions that already worked. If the fix had moved the gate rather
    // than setting the flag, these are what would break.
    let top = to_html(&format!("{IMAGE}\n^ top cap\n"));
    assert!(top.contains("<figcaption>top cap</figcaption>"), "{top}");

    let div = to_html(&format!(":::\n{IMAGE}\n^ div cap\n:::\n"));
    assert!(div.contains("<figcaption>div cap</figcaption>"), "{div}");
}

#[test]
fn an_orphan_caption_line_in_an_item_stays_text() {
    // §4: a `^ ` line with nothing captionable above it is ordinary content.
    // Setting the flag must not start promoting these.
    let html = to_html("- text\n  ^ not a caption\n");
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("^ not a caption"), "{html}");
}

#[test]
fn an_image_indented_past_the_content_column_stays_literal() {
    // The strict column-0 rule, and the reason the gate exists at all: an image
    // ABOVE the item's content column is literal paragraph text, caption and all.
    // All four implementations agree, so the flag must not be set for this path.
    let html = to_html(&format!("- text\n\n    {IMAGE}\n    ^ cap here\n"));
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("^ cap here"), "{html}");
}

#[test]
fn a_lazy_caption_at_column_zero_does_not_attach() {
    // A caption BELOW the content column ends the item instead of folding in, so
    // it never reaches the item's paragraph. Pinned because the fix's premise is
    // that the item lead paragraph always starts AT the content column - if a
    // lazy line could start it, the flag would be a lie.
    let html = to_html(&format!("- {IMAGE}\n^ lazy cap\n"));
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("^ lazy cap"), "{html}");
}
