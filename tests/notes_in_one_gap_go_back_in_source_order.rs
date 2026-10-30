//! Every footnote definition written in a gap between two blocks goes back
//! into that gap in source order, also when two notes share a line
//! (carve-js#3992, corpus 763).

use carve::{to_carve, to_html};

#[test]
fn two_notes_on_one_line_keep_their_place_and_order() {
    let source = "- item\n  [^c]: [^d]: para\n          more\n   ~~~\n\n  See[^c][^d]\n";
    let written = to_carve(source);
    assert_eq!(
        written,
        "- item\n  [^c]: {empty}\n  [^d]: para\n    more\n  ```\n\n  See[^c][^d]\n  ```\n"
    );
    assert_eq!(to_html(&written), to_html(source));
}

#[test]
fn notes_on_two_lines_of_one_gap_both_go_back() {
    let source = "- a\n  [^c]: x\n  [^d]: y\n\n  > more [^c] [^d]\n";
    let written = to_carve(source);
    assert_eq!(written, "- a\n  [^c]: x\n  [^d]: y\n  > more [^c] [^d]\n");
    assert_eq!(to_html(&written), to_html(source));
}
