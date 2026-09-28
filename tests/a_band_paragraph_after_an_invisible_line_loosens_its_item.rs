//! A second paragraph that reaches its item from the BAND loosens the item, the
//! same as one written at the content column (PART 9 §17 L1/L1b).
//!
//! markup-carve/carve-rs#2142 reported the opposite: that this crate reads
//!
//!     - t
//!
//!       %% c
//!      z
//!
//! LOOSE where the oracle, carve-js and carve-php read it tight, and asked for
//! the tight reading. Measured at current heads (oracle `b940924c`, carve-js
//! `e8508fb5`, carve-php `1074a795`) the three-way reading is as reported. The
//! reading it asks for is not: the ticket's argument rests on `z` folding into
//! the lead paragraph, so that the item holds ONE paragraph and has no
//! separation to sit in. No engine folds it. Every one of the four, the oracle
//! included, builds the item as paragraph, comment, paragraph - and a tight
//! item renders all of its paragraphs bare, so the HTML the ticket compared
//! cannot tell one paragraph from two.
//!
//! GIVEN THAT TREE, L1b DECIDES IT. L1 asks whether the item holds a
//! blank-line-separated second PARAGRAPH; L1b (NORMATIVE, CARVE-P9-030) says an
//! invisible line is not a separator, so the blank's separation survives the
//! comment. `t` and `z` are two paragraphs with a blank line between them. The
//! item is LOOSE.
//!
//! THE COLUMN IS NOT A TERM IN THE RULE, which is what the tests below pin.
//! Move `z` one column right, to the content column, and the item tree does not
//! change at all - only the authored column does - yet the oracle, carve-js and
//! carve-php flip to loose there. That spelling is corpus
//! `186-an-invisible-line-does-not-cancel-a-blank-line-separation` and it is
//! pinned loose. §17 states tightness over the item's blocks and the blank lines
//! between them and names no column, so the two spellings must agree, and the
//! pinned one says which way.
//!
//! markup-carve/carve#1803 is the precedent and the same argument: the oracle
//! read a comment-then-blank item tight, L1b's reasoning does not mention the
//! line ORDER either, and the oracle was the side that changed. Here the
//! variable is the column instead of the order.
//!
//! WHY THIS CRATE ANSWERS BOTH SPELLINGS ALIKE: the band line folds into the
//! same collected continuation chunk as the comment, so both spellings reach the
//! one looseness test rather than two. The other three run their test only in
//! the branch for a line at or above the content column, so the band spelling
//! never reaches it - the tight answer is where the check is, not a decision.
//!
//! No corpus document carries the band spelling, at any marker width or either
//! comment spelling, which is why this file exists.

fn tight(source: &str) -> bool {
    let carve::BlockNode::List(list) = &carve::parse(source).children[0] else {
        panic!("expected a list");
    };
    list.tight
}

fn item_kinds(source: &str) -> Vec<&'static str> {
    let carve::BlockNode::List(list) = &carve::parse(source).children[0] else {
        panic!("expected a list");
    };
    list.items[0]
        .children
        .iter()
        .map(|block| match block {
            carve::BlockNode::Paragraph(_) => "paragraph",
            carve::BlockNode::Comment(_) => "comment",
            _ => "other",
        })
        .collect()
}

/// The ticket's document. The premise it was argued from is the part to check:
/// the item holds TWO paragraphs, so L1 has a second paragraph to ask about.
#[test]
fn the_band_paragraph_is_the_item_s_second_paragraph() {
    let source = "- t\n\n  %% c\n z\n";
    assert_eq!(item_kinds(source), ["paragraph", "comment", "paragraph"]);
    assert!(!tight(source));
}

/// The pinned spelling, one column over: corpus
/// `186-an-invisible-line-does-not-cancel-a-blank-line-separation`. It fixes
/// which way the pair agrees.
#[test]
fn the_content_column_spelling_is_loose() {
    let source = "- t\n\n  %% c\n  z\n";
    assert_eq!(item_kinds(source), ["paragraph", "comment", "paragraph"]);
    assert!(!tight(source));
}

/// THE INVARIANT. One item tree, two authored columns for its second paragraph,
/// one answer. This is the assertion the other three fail.
#[test]
fn the_follower_s_column_does_not_move_the_item_s_tightness() {
    let band = "- t\n\n  %% c\n z\n";
    let body = "- t\n\n  %% c\n  z\n";
    assert_eq!(item_kinds(band), item_kinds(body));
    assert_eq!(tight(band), tight(body));
}

/// THE DISCRIMINATOR: the blank line is what loosens, not the comment and not
/// the band. Delete the blank and both spellings are tight, in all four readers.
#[test]
fn without_the_blank_line_both_spellings_are_tight() {
    assert!(tight("- t\n  %% c\n z\n"));
    assert!(tight("- t\n  %% c\n  z\n"));
}

/// A tight item renders every paragraph bare, so the HTML of the no-blank
/// spelling is the tight reading of the ticket's document, character for
/// character. Kept because it is the evidence the ticket compared, and it shows
/// why that comparison could not decide the question.
#[test]
fn the_tight_html_cannot_show_the_paragraph_split() {
    let no_blank = carve::render_html(&carve::parse("- t\n  %% c\n z\n")).unwrap();
    assert!(!no_blank.contains("<p>"), "{no_blank}");
    let with_blank = carve::render_html(&carve::parse("- t\n\n  %% c\n z\n")).unwrap();
    assert!(with_blank.contains("<p>t</p>"), "{with_blank}");
}

/// Every marker width and both spellings of the comment, with the comment at or
/// above the content column and the second paragraph anywhere in the band. The
/// ticket's own sweeps put the family at 16 of 920 and 90 of 1944 documents; the
/// axes are these.
#[test]
fn the_whole_family_answers_the_way_its_content_column_twin_does() {
    for (marker, content_col) in [("- ", 2), ("* ", 2), ("1. ", 3), ("10. ", 4)] {
        for comment in ["%% c", "%%%"] {
            for comment_col in content_col..content_col + 3 {
                for band_col in 1..content_col {
                    for blanks in 1..=2 {
                        let band = format!(
                            "{marker}t\n{}{}{comment}\n{}z\n",
                            "\n".repeat(blanks),
                            " ".repeat(comment_col),
                            " ".repeat(band_col),
                        );
                        let body = format!(
                            "{marker}t\n{}{}{comment}\n{}z\n",
                            "\n".repeat(blanks),
                            " ".repeat(comment_col),
                            " ".repeat(content_col),
                        );
                        assert_eq!(
                            item_kinds(&band),
                            ["paragraph", "comment", "paragraph"],
                            "{band:?}"
                        );
                        assert!(!tight(&band), "{band:?} should be loose");
                        assert_eq!(tight(&band), tight(&body), "{band:?} vs {body:?}");
                    }
                }
            }
        }
    }
}
