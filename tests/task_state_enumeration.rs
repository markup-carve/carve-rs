//! PART 9 enumerates the task states exhaustively:
//!
//! ```text
//! task_marker = '[', task_state, ']', space ;
//! task_state = ' ' | 'x' | 'X' | '-' | '_' | '>' | '?' ;
//! ```
//!
//! `detect_task` checked the brackets and the trailing space but never the
//! state byte, so ANY single character opened a task item - and the bracket
//! text was dropped rather than rendered (carve-rs#471).
//!
//! carve-js implements the enumeration. carve-php shared this defect
//! (carve-php#657). The majority was on the wrong side, so these assert
//! against the grammar rather than against the other engines.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn every_enumerated_state_opens_a_task_item() {
    for state in [' ', 'x', 'X', '-', '_', '>', '?'] {
        let html = to_html(&format!("- [{state}] item\n"));
        assert!(
            html.contains("type=\"checkbox\""),
            "[{state}] should be a task marker, got {html}"
        );
    }
}

#[test]
fn only_x_is_checked() {
    assert!(to_html("- [x] item\n").contains("checked"));
    assert!(to_html("- [X] item\n").contains("checked"));
    assert!(!to_html("- [?] item\n").contains("checked"));
    assert!(!to_html("- [ ] item\n").contains("checked"));
}

#[test]
fn a_character_outside_the_enumeration_stays_literal() {
    for state in ['d', '1', '#', '*', '!', '~'] {
        assert_eq!(
            squash(&to_html(&format!("- [{state}] item\n"))),
            format!("<ul> <li>[{state}] item</li> </ul>"),
            "[{state}] is not a task state and must stay literal text"
        );
    }
}

#[test]
fn the_bracket_text_survives() {
    // The sharp end: the marker was not reinterpreted, it was deleted.
    let html = to_html("- [!] urgent\n");

    assert!(html.contains("[!]"), "the bracket text vanished: {html}");
    assert!(!html.contains("checkbox"));
}

#[test]
fn two_characters_were_already_rejected() {
    assert_eq!(
        squash(&to_html("- [ab] item\n")),
        "<ul> <li>[ab] item</li> </ul>"
    );
}
