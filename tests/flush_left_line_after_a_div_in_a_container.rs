//! A REAL DIV IN A CONTAINER, AND THE FLUSH-LEFT LINE AFTER IT (carve#939).
//!
//! PART 1 S4 folds a flush-left line into the innermost OPEN PARAGRAPH, and
//! folds nothing when there is none. A REAL `::: ` div - one whose opener
//! passes PART 7's separator test, so it is a block and not absorbed paragraph
//! text - makes that clause decide two ways depending on what the div holds
//! when the line arrives.
//!
//! No engine ticket was filed for these three documents, because nothing
//! diverged when the ruling was measured. This engine diverged on the first of
//! them: it read a div as a complete block with no open paragraph whether or
//! not a `:::` line closed it, so the flush-left line ended the item.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn an_unterminated_div_holding_a_paragraph_takes_the_flush_left_line() {
    assert_eq!(
        squash(&to_html("- item\n+\n::: note\nbody\ntail\n:::\n")),
        squash(
            "<ul>\n  <li>item\n    <aside class=\"admonition note\">\n      \
             <p>body\ntail</p>\n    </aside>\n  </li>\n</ul>"
        )
    );
}

#[test]
fn an_unterminated_div_holding_nothing_does_not() {
    // The item's own paragraph was closed by the div that followed it, and the
    // div itself is empty - so nothing in the stack is open, the containers
    // close, and the line is a top-level paragraph.
    let html = squash(&to_html("- item\n+\n::: note\n\n:::\n\ntail\n"));
    assert!(html.ends_with("</ul> <p>tail</p>"), "{html}");
}

#[test]
fn control_closing_the_div_inverts_the_first_answer() {
    // The reason the rule is about an UNTERMINATED div: the `:::` line closes
    // the paragraph inside it, so the first document's answer inverts on the
    // strength of one line. This pins behavior that does not change.
    let html = squash(&to_html("- item\n+\n::: note\nbody\n:::\n\ntail\n"));
    assert!(html.contains("<p>body</p>"), "{html}");
    assert!(html.ends_with("</ul> <p>tail</p>"), "{html}");
}

#[test]
fn control_the_same_shape_in_a_block_quote_is_unchanged() {
    let html = squash(&to_html("> item\n>\n> ::: note\n> body\n> tail\n> :::\n"));
    assert!(html.contains("<p>body tail</p>"), "{html}");
}

#[test]
fn control_a_flush_left_line_after_a_closed_block_still_ends_the_item() {
    // A code block and a table have no open paragraph either, and the div rule
    // must not have widened to them.
    let html = squash(&to_html("- item\n+\n```\nx\n```\n\ntail\n"));
    assert!(html.ends_with("</ul> <p>tail</p>"), "{html}");
}
