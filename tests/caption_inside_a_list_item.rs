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
fn one_blank_before_table_caption_keeps_list_tight() {
    let html = to_html("- | a |\n\n  ^ cap\n- c\n");
    assert!(html.contains("<caption>cap</caption>"), "{html}");
    assert!(html.contains("<li>c</li>"), "{html}");
}

#[test]
fn two_blanks_detach_table_caption_and_loosen_list() {
    let html = to_html("- | a |\n\n\n  ^ cap\n- c\n");
    assert!(!html.contains("<caption>"), "{html}");
    assert!(html.contains("<li><p>c</p></li>"), "{html}");
}

#[test]
fn one_blank_before_table_continuation_caption_keeps_list_tight() {
    let html = to_html("- | a |\n  + b |\n\n  ^ cap\n- c\n");
    assert!(html.contains("<caption>cap</caption>"), "{html}");
    assert!(html.contains("<li>c</li>"), "{html}");
}

#[test]
fn one_blank_before_code_caption_keeps_list_tight() {
    let html = to_html("- ```\n  x\n  ```\n\n  ^ cap\n- c\n");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
    assert!(html.contains("<li>c</li>"), "{html}");
}

#[test]
fn two_blanks_after_code_block_detach_caption() {
    let html = to_html("- ```\n  x\n  ```\n\n\n  ^ cap\n- c\n");
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("<li><p>c</p></li>"), "{html}");
}

#[test]
fn one_blank_keeps_each_captionable_host_tight() {
    for host in [
        "![a](b.png)",
        "> q",
        "$$`x`",
        "::: figure\n  ![a](b.png)\n  :::",
    ] {
        let html = to_html(&format!("- {host}\n\n  ^ cap\n- c\n"));
        assert!(html.contains(">cap</"), "{host}: {html}");
        assert!(html.contains("<li>c</li>"), "{host}: {html}");
    }
}

#[test]
fn a_blank_before_a_captioned_equation_loosens_the_list() {
    for gap in ["", "\n"] {
        let html = to_html(&format!("- a\n\n  $$`x`\n{gap}  ^ cap\n- c\n"));
        assert!(html.contains("<li><p>a</p>"), "{html}");
        assert!(html.contains("<li><p>c</p></li>"), "{html}");
    }
}

#[test]
fn a_lazy_quote_line_before_its_caption_keeps_the_list_tight() {
    let html = to_html("- > q\n  text\n\n  ^ cap\n- c\n");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
    assert!(html.contains("<li>c</li>"), "{html}");
}

#[test]
fn an_image_or_equation_after_attributes_keeps_its_caption_tight() {
    for host in ["![a](b.png)", "$$`x`"] {
        let html = to_html(&format!("- {{.x}}\n  {host}\n\n  ^ cap\n- c\n"));
        assert!(
            html.contains("<figcaption>cap</figcaption>"),
            "{host}: {html}"
        );
        assert!(html.contains("<li>c</li>"), "{host}: {html}");
    }
}

#[test]
fn an_image_after_a_heading_keeps_its_caption_tight() {
    let html = to_html("- # h\n  ![a](b.png)\n\n  ^ cap\n- c\n");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
    assert!(html.contains("<li>c</li>"), "{html}");
}

#[test]
fn an_adjacent_image_caption_does_not_absorb_the_following_fence() {
    let html = to_html("- ![a](i)\n  ^ cap\n  ```\n  b\n y\n  ```\n");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
    assert!(html.contains("<pre><code>b\n</code></pre>"), "{html}");
}

#[test]
fn two_blanks_detach_figure_group_caption() {
    let html = to_html("- ::: figure\n  ![a](b.png)\n  :::\n\n\n  ^ cap\n- c\n");
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("<p>^ cap</p>"), "{html}");
    assert!(html.contains("<li><p>c</p></li>"), "{html}");
}

#[test]
fn a_blank_after_an_empty_quote_does_not_attach_a_caption() {
    let html = to_html("- >\n\n  ^ cap\n- c\n");
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("<p>^ cap</p>"), "{html}");
    assert!(html.contains("<li><p>c</p></li>"), "{html}");
}

#[test]
fn an_image_line_inside_a_paragraph_does_not_attach_a_caption() {
    let html = to_html("- para\n  ![a](b.png)\n\n  ^ cap\n- c\n");
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("<p>^ cap</p>"), "{html}");
    assert!(html.contains("<li><p>c</p></li>"), "{html}");
}

#[test]
fn blank_before_orphan_caption_loosens_list() {
    let html = to_html("- text\n\n  ^ cap\n- c\n");
    assert!(html.contains("<p>^ cap</p>"), "{html}");
    assert!(html.contains("<li><p>c</p></li>"), "{html}");
}

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
fn an_image_indented_past_the_content_column_uses_its_authored_base() {
    let html = to_html(&format!("- text\n\n    {IMAGE}\n    ^ cap here\n"));
    assert!(html.contains("<figcaption>cap here</figcaption>"), "{html}");
}

#[test]
fn a_lazy_caption_at_column_zero_does_not_attach() {
    // The caption folds into the item paragraph but cannot turn its image into
    // a figure from below the item's content column.
    let html = to_html(&format!("- {IMAGE}\n^ lazy cap\n"));
    assert!(!html.contains("<figcaption>"), "{html}");
    assert!(html.contains("^ lazy cap"), "{html}");
}
