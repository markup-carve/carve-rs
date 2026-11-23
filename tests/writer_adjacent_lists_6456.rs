//! C2-W-028: two adjacent ordered lists of different styles get no blank line unless the second could join the first
//! (carve#6456, CC-1215).
use carve::api::{Carve, WriteOptions};

fn fmt(input: &str) -> String {
    Carve::new()
        .format(input, &WriteOptions::default())
        .unwrap()
}

#[test]
fn ordered_lists_of_different_styles_are_written_without_a_blank_line() {
    let cases = [
        ("1. a\ni. z\n", "1. a\ni. z\n"),
        ("1. a\n\na. z\n", "1. a\na. z\n"),
        ("a. a\n\n1. z\n", "a. a\n1. z\n"),
        ("i. a\n1. z\n", "i. a\n1. z\n"),
        ("c.  x\n\nCI.  y\n", "c. x\nCI.  y\n"),
        ("1) a\n\nb) z\n", "1) a\nb) z\n"),
        ("C) a\n\n1) z\n", "C) a\n1) z\n"),
        ("> 1. a\n>\n> i. z\n", "> 1. a\n> i. z\n"),
        ("a. first\nii. second\n", "a. first\nii. second\n"),
    ];
    for (input, expected) in cases {
        assert_eq!(fmt(input), expected, "{input:?}");
        assert_eq!(fmt(expected), expected, "{expected:?}");
    }
}

#[test]
fn other_adjacent_lists_keep_their_separation() {
    // CC-1213: a marker the first list can read takes three blank lines.
    assert_eq!(fmt("a. x\n\n\n\nc. z\n"), "a. x\n\n\n\nc. z\n");
    assert_eq!(fmt("- a\n\n* z\n"), "- a\n\n* z\n");
    assert_eq!(fmt("1. a\n\n1) z\n"), "1. a\n\n1) z\n");
}
