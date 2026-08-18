//! Prose written after an item's first block reopens the item's paragraph, and
//! a following flush-left line folds into it (markup-carve/carve-rs#1098, spec
//! PART 1 S4 as markup-carve/carve#1370 clarifies it, corpus category `361`).
//!
//! PART 1 S4 asks whether any container in the open stack holds an OPEN
//! paragraph. It does NOT ask whether that paragraph is the container's first
//! block. An item whose first block is a table, a fence or a heading and whose
//! next line is prose holds an open paragraph exactly as an item that began
//! with prose does; the blocks before it are spent, having answered S4 while
//! they were the item's last block and stopped answering it when prose reopened
//! one.
//!
//! This engine rendered `b` AS the item's prose and then declined to treat its
//! paragraph as open, which answers one line two ways in a single parse.
//!
//! Only the MARKER-LINE spelling was wrong. `- x` / `  | a |` / `  b` / `tail`
//! already folded, so the two spellings of one item disagreed about the same
//! question depending on which line the first block was written on.
//!
//! Every expectation here was measured against carve-js `7cd66e0` and the
//! executable spec oracle at spec `662e861`, which agree on all of it.

use carve::to_html;

/// Does the flush-left line stay inside the item?
fn folds(src: &str) -> bool {
    let html = to_html(src);
    !html.contains("<p>tail</p>")
}

/// The three container heads this reaches, with the content column each opens.
const HEADS: &[(&str, &str)] = &[("- ", "  "), ("* ", "  "), ("1. ", "   "), (". ", "  ")];

#[test]
fn a_block_on_the_marker_line_then_prose_keeps_the_item_open() {
    // The reported shape and its fence and heading spellings, at every head.
    for (head, col) in HEADS {
        for block in [vec!["| a |"], vec!["```", "c", "```"], vec!["# h"]] {
            let mut src = format!("{head}{}\n", block[0]);
            for line in &block[1..] {
                src.push_str(&format!("{col}{line}\n"));
            }
            src.push_str(&format!("{col}b\ntail\n"));
            assert!(folds(&src), "{src}\n{}", to_html(&src));
        }
    }
}

#[test]
fn the_indented_spelling_did_not_move() {
    // THE CONTROL THAT NAMES THE DEFECT. Writing the same first block one line
    // down always folded, so the fix is the two spellings agreeing rather than
    // a new behavior. It must still fold.
    for (head, col) in HEADS {
        for block in ["| a |", "# h"] {
            let src = format!("{head}x\n{col}{block}\n{col}b\ntail\n");
            assert!(folds(&src), "{src}\n{}", to_html(&src));
        }
    }
}

#[test]
fn a_blank_line_still_closes_the_paragraph() {
    // INTENDED SURVIVOR. The blank closes the paragraph the prose opened, so
    // there is nothing left for `tail` to fold into and it is a document
    // sibling. Corpus `361-...-4` pins it.
    for (head, col) in HEADS {
        let src = format!("{head}| a |\n{col}b\n\ntail\n");
        assert!(!folds(&src), "{src}\n{}", to_html(&src));
    }
}

#[test]
fn a_row_at_the_content_column_opens_no_paragraph() {
    // INTENDED SURVIVOR, and the one a fix that folds after a table
    // unconditionally answers wrong. `| b |` extends the item's own table, so
    // the item's last block is still a table and no paragraph is open. Corpus
    // `361-...-5` pins it.
    for (head, col) in HEADS {
        let src = format!("{head}| a |\n{col}| b |\ntail\n");
        assert!(!folds(&src), "{src}\n{}", to_html(&src));
    }
}

#[test]
fn a_marker_line_block_with_no_prose_under_it_still_ends_the_item() {
    // INTENDED SURVIVOR (markup-carve/carve#1280). With nothing after it the
    // marker line's block IS the item's last block, and a heading, a thematic
    // break or a table leaves no open paragraph - so the flush-left line ends
    // the item exactly as `> # h` / `tail` ends a quote.
    for (head, _) in HEADS {
        for block in ["| a |", "---", "%% c", "[r]: /u"] {
            let src = format!("{head}{block}\ntail\n");
            assert!(!folds(&src), "{src}\n{}", to_html(&src));
        }
    }
}

#[test]
fn a_spent_first_block_no_longer_answers_for_the_item() {
    // The half of the fix that is NOT the ticket's fifteen shapes, found in the
    // same sweep. A thematic break, a comment or a link-reference definition on
    // the marker line used to make the item fold WHATEVER came after it - the
    // marker line's own kind was consulted instead of the item's last block. So
    // an item whose last block is a table folded a flush-left line in, which is
    // the opposite of the rule this file is about. carve-js and the executable
    // spec both end the item on all three.
    for (head, col) in HEADS {
        for block in ["---", "%% c", "[r]: /u"] {
            let src = format!("{head}{block}\n{col}| b |\ntail\n");
            assert!(
                !folds(&src),
                "a spent first block still answered for the item:\n{src}\n{}",
                to_html(&src)
            );
        }
    }
}

#[test]
fn a_heading_last_in_the_body_still_folds() {
    // INTENDED SURVIVOR. A heading takes a flush-left line as its own
    // continuation (PART 2, carve#326), which is a different question from
    // whether a paragraph is open - so the heading term stays beside S4's
    // question rather than under it.
    for (head, col) in HEADS {
        let src = format!("{head}# h\n{col}# i\ntail\n");
        assert!(folds(&src), "{src}\n{}", to_html(&src));
    }
}

#[test]
fn an_open_fence_still_closes_the_item() {
    // INTENDED SURVIVOR (markup-carve/carve#950). A FENCED BODY IS NOT A
    // PARAGRAPH, so an item holding an unterminated fence has nothing open and
    // the flush-left line ends it.
    for (head, col) in HEADS {
        let src = format!("{head}```\n{col}c\ntail\n");
        assert!(!folds(&src), "{src}\n{}", to_html(&src));
    }
}

#[test]
fn the_definition_list_head_did_not_move() {
    // The `dd` path was already correct, which is what told the ticket this is
    // not a whole-family rewrite. It must still answer the same way.
    let src = ":: t\n:  | a |\n   b\ntail\n";
    assert!(folds(src), "{}", to_html(src));
}
