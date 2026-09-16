//! PART 11 section 8a M1f as amended in markup-carve/carve#2056: on a line
//! emitted as a heading, a trailing hash run after a space or tab is the ATX
//! closing sequence to a CommonMark reader, so its first hash is escaped.

use carve::{parse, render_markdown};

fn md(src: &str) -> String {
    render_markdown(&parse(src)).expect("the document writes")
}

#[test]
fn a_trailing_hash_run_on_a_heading_line_is_escaped() {
    for (src, want) in [
        ("# a ##\n", "# a \\##\n"),
        ("# a #\n", "# a \\#\n"),
        ("# a ###\n", "# a \\###\n"),
        ("# a ######\n", "# a \\######\n"),
        ("# a #######\n", "# a \\#######\n"),
        ("## a ##\n", "## a \\##\n"),
    ] {
        assert_eq!(md(src), want, "{src:?}");
    }
}

#[test]
fn only_the_run_that_ends_the_line_is_escaped() {
    // The escape goes on the run's FIRST hash, and a run with text after it is
    // not a closing sequence at all.
    assert_eq!(md("# a ### b ###\n"), "# a ### b \\###\n");
}

#[test]
fn a_tab_before_the_run_is_escaped_too() {
    // The two readers disagree about the bare form here - markdown-it-py drops
    // the run, pulldown-cmark keeps it - and both keep the escaped one, which
    // is why the clause names the tab beside the space.
    assert_eq!(md("# a\t##\n"), "# a\t\\##\n");
}

#[test]
fn a_heading_inside_a_container_is_escaped_the_same_way() {
    assert_eq!(md("\n- # a ##\n"), "- # a \\##\n");
    assert_eq!(md("\n1. # a ##\n"), "1. # a \\##\n");
    assert_eq!(md("\n> - # a ##\n"), "> - # a \\##\n");
}

#[test]
fn a_run_that_is_not_a_closing_sequence_stays_bare() {
    // CONTROLS. No space in front, a following word, and a hash on a line that
    // is not a heading at all.
    assert_eq!(md("# a## \n"), "# a##\n");
    assert_eq!(md("a # b\n"), "a # b\n");
    assert_eq!(md("# a\n"), "# a\n");
    // And the line that is NOT a heading: no closing sequence exists there, so
    // the run is ordinary text.
    assert_eq!(md("a ##\n"), "a ##\n");
    assert_eq!(md("\n- a ##\n"), "- a ##\n");
    assert_eq!(md("> a ##\n"), "> a ##\n");
}
