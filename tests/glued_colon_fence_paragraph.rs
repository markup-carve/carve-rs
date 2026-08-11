//! A glued colon fence (`:::note`, `:::]`) is paragraph text, and it holds back
//! exactly one thing: a following BARE fence (`:::`), which is closer-shaped and
//! would close a container the paragraph never opened. A real opener - a type
//! word, `::: |`, `::: \` - still interrupts the paragraph (carve-rs#496).
//!
//! carve-js and carve-php both split these; carve-rs used to let the first glued
//! line disable colon-fence interruption for the whole paragraph.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn a_line_block_opener_interrupts_after_a_glued_fence() {
    assert_eq!(
        html(":::]\n\n::: |\n\n:::\n"),
        "<p>:::]</p>\n<div class=\"line-block\">\n</div>"
    );
}

#[test]
fn a_container_opener_interrupts_after_a_glued_fence() {
    assert_eq!(
        html(":::note\n\n::: warn\n\n:::\n"),
        "<p>:::note</p>\n<div class=\"warn\">\n\n</div>"
    );
}

#[test]
fn a_bare_fence_still_folds_after_a_glued_fence() {
    assert_eq!(html(":::note\ntext\n:::\n"), "<p>:::note\ntext\n:::</p>");
}

#[test]
fn a_bare_fence_interrupts_a_paragraph_with_no_glued_fence() {
    assert_eq!(html("text\n\n:::\n\n:::\n"), "<p>text</p>\n<div>\n</div>");
}

#[test]
fn the_split_holds_inside_a_block_quote() {
    assert_eq!(
        html("> :::]\n>\n> ::: |\n>\n> :::\n"),
        "<blockquote>\n  <p>:::]</p>\n  <div class=\"line-block\">\n  </div>\n</blockquote>"
    );
}

#[test]
fn the_split_holds_inside_a_container() {
    assert_eq!(
        html(":::\n\\:::\\]\n\n:::: |\n\\::::\n::::\n:::\n"),
        "<div>\n  <p>:::]</p>\n  <div class=\"line-block\">\n    <p>::::</p>\n  </div>\n</div>"
    );
}

#[test]
fn the_split_holds_in_an_item_lead_paragraph() {
    assert_eq!(
        html("- :::]\n+\n::: |\n\n:::\n"),
        "<ul>\n  <li>:::]\n    <div class=\"line-block\">\n    </div>\n  </li>\n</ul>"
    );
}

#[test]
fn an_item_lead_paragraph_still_folds_a_bare_fence() {
    assert_eq!(html("- :::]\n  :::\n"), "<ul>\n  <li>:::]\n:::</li>\n</ul>");
}
