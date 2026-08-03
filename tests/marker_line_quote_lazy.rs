//! A flush-left line after a list item whose FIRST BLOCK is a quote written on
//! the marker line folds into that quote's open paragraph.
//!
//! This engine ended the list instead - and disagreed with itself doing it: the
//! same quote written on the NEXT line folds, and so does the same document at
//! the top level. PART 1 S4 folds a lazy continuation into the innermost OPEN
//! PARAGRAPH, and the quote's paragraph is one (carve#572).
//!
//! The open-paragraph condition is the whole rule: an EMPTY quote has no
//! paragraph, and neither does the item, so nothing is open and the line ends
//! the item. carve-js and carve-php fold there too; they are wrong, and the
//! second test pins this engine's answer so a later fix cannot quietly adopt
//! theirs.

fn html(source: &str) -> String {
    carve::to_html(source)
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_lazy_line_folds_into_a_marker_line_quote() {
    assert_eq!(
        html("- > q\nlazy\n"),
        "<ul><li><blockquote><p>q lazy</p></blockquote></li></ul>",
    );
}

#[test]
fn the_same_quote_on_the_next_line_folds_the_same_way() {
    // The shape this engine already agreed with everyone about, kept here so
    // the two cannot drift apart again.
    assert_eq!(
        html("- item\n  > q\nlazy\n"),
        "<ul><li>item <blockquote><p>q lazy</p></blockquote></li></ul>",
    );
}

#[test]
fn an_empty_marker_line_quote_has_nothing_to_fold_into() {
    assert_eq!(
        html("- >\nlazy\n"),
        "<ul><li><blockquote></blockquote></li></ul><p>lazy</p>",
    );
}

#[test]
fn a_block_opener_still_interrupts() {
    assert_eq!(
        html("- > q\n| a |\n"),
        "<ul><li><blockquote><p>q</p></blockquote></li></ul><table><tbody><tr><td>a</td></tr></tbody></table>",
    );
}

#[test]
fn a_blank_line_still_ends_the_item() {
    assert_eq!(
        html("- > q\n\nafter\n"),
        "<ul><li><blockquote><p>q</p></blockquote></li></ul><p>after</p>",
    );
}
