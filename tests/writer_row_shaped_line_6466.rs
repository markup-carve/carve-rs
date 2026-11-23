//! carve#6466: a row-shaped paragraph line has the line kind `|` (C2-W-088), so only the pipe at the start of
//! the line takes a backslash. The expected bytes are carve-php's.

use carve::api::{Carve, Options, WriteOptions};

fn check(source: &str, expected: &str) {
    check_paths(source, expected, expected);
}

/// `plain` is the form written from the tree without the sidecar of the parse.
fn check_paths(source: &str, expected: &str, plain: &str) {
    let carve = Carve::new();
    let options = WriteOptions::default();
    let written = carve.format(source, &options).unwrap();
    assert_eq!(written, expected, "{source:?}");
    let tree = carve.parse(source, &Options::default()).unwrap();
    assert_eq!(carve.write(&tree, &options).unwrap(), plain, "{source:?}");
    assert_eq!(carve.format(&written, &options).unwrap(), written);
}

#[test]
fn only_the_pipe_at_the_line_start_takes_a_backslash() {
    check("| a \\|\n", "\\| a |\n");
    check("| a |\\|\n", "\\| a ||\n");
    check("|a\\|\n", "\\|a|\n");
    check("|= a \\|\n", "\\|= a |\n");
    check("| a | b \\|\n", "\\| a | b |\n");
}

#[test]
fn a_row_shaped_line_behind_a_marker_escapes_only_its_first_pipe() {
    check("- | a \\|\n", "- \\| a |\n");
    check("> | a | b \\|\n", "> \\| a | b |\n");
    check_paths("1. |= a \\|\n", "1. \\|= a |\n", ". \\|= a |\n");
    check("p line\n| a \\|\n| r |\n", "p line\n\\| a |\n\n| r |\n");
}
