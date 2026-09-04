//! A LONE `|` LINE IS NOT A TABLE ROW, AND MUST NOT PANIC
//! (markup-carve/carve-rs#1553).
//!
//! `layout_pipe_cells` tested the first and last byte independently, and on a
//! one-character line the same `|` answered both - so it sliced `line[1..0]` and
//! panicked with "byte range starts at 1 but ends at 0". `carve::to_html("|")`
//! took the process down, which for an embedder rendering untrusted input is an
//! unauthenticated DoS in one byte rather than a rendering bug.
//!
//! ORACLE: the executable spec at carve `8898a1a5` (spec main). All ten
//! documents below match it exactly.

use carve::to_html;

/// The one-character document. THE REPORTED CASE.
#[test]
fn a_document_that_is_one_pipe_renders_as_text() {
    assert_eq!(to_html("|\n"), "<p>|</p>");
}

/// Without the trailing newline too, since the panic was in a trim'd slice.
#[test]
fn the_same_without_a_trailing_newline() {
    assert_eq!(to_html("|"), "<p>|</p>");
}

/// It panicked with almost ANY follower, so the follower is not what decides it.
#[test]
fn a_lone_pipe_followed_by_other_blocks_renders() {
    assert_eq!(
        to_html("|\n# h\n"),
        "<p>|</p>\n<section id=\"h\">\n  <h1>h</h1>\n</section>"
    );
    assert_eq!(
        to_html("|\n> q\n"),
        "<p>|</p>\n<blockquote><p>q</p></blockquote>"
    );
    assert_eq!(to_html("|\n- x\n"), "<p>|\n- x</p>");
    assert_eq!(to_html("|\nx\n"), "<p>|\nx</p>");
    assert_eq!(to_html("|\n```\n"), "<p>|\n<code></code></p>");
}

/// CONTROLS. Each of these already answered correctly, and each is a neighbor of
/// the guard: `||` is the two-character line the guard now lets through
/// unchanged, `| a |` is a real row, and an indented or preceded `|` never
/// reached the layout pass at all.
#[test]
fn the_neighboring_shapes_are_unchanged() {
    assert_eq!(to_html("||\n"), "<p>||</p>");
    assert_eq!(
        to_html("| a |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>"
    );
    assert_eq!(to_html(" |\n"), "<p>|</p>");
    assert_eq!(to_html("a\n|\nx\n"), "<p>a\n|\nx</p>");
}
