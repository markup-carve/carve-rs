//! CARVE-P11-079 (a) collects footnote definitions from a container to the
//! document tail, preserving their source order even when two share a line.

use carve::{to_carve, to_html};

#[test]
fn two_notes_on_one_line_are_collected_in_order() {
    let source = "- item\n  [^c]: [^d]: para\n          more\n   ~~~\n\n  See[^c][^d]\n";
    let written = to_carve(source);
    assert_eq!(
        written,
        "- item\n  ```\n\n  See[^c][^d]\n  ```\n\n[^c]: {empty}\n\n[^d]: para\n  more\n"
    );
    assert_eq!(to_html(&written), to_html(source));
}

#[test]
fn notes_on_two_lines_of_one_gap_both_go_back() {
    let source = "- a\n  [^c]: x\n  [^d]: y\n\n  > more [^c] [^d]\n";
    let written = to_carve(source);
    assert_eq!(written, "- a\n\n  > more [^c] [^d]\n\n[^c]: x\n\n[^d]: y\n");
    assert_eq!(to_html(&written), to_html(source));
}
