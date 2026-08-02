//! djot.js PR137 / djot-php #264 parity: a trailing backslash at EOF is a hard
//! break. A bare same-level `#` used to continue a heading; under SINGLE-LINE
//! HEADINGS (PART 2) it does not, which is pinned here as the guard.

use carve::to_html;

#[test]
fn trailing_backslash_at_eof_is_hard_break() {
    assert_eq!(to_html("para\\\n"), "<p>para<br>\n</p>");
}

#[test]
fn normal_hard_break_unchanged() {
    assert_eq!(to_html("a\\\nb\n"), "<p>a<br>\nb</p>");
}

#[test]
fn trailing_escaped_punctuation_unchanged() {
    assert_eq!(to_html("a\\*\n"), "<p>a*</p>");
}

#[test]
fn bare_same_level_hash_does_not_continue_a_heading() {
    // This used to join `h` and `x` into one title with the id `h-x`. Each `#`
    // line now stands alone, and the content-less one is not a heading at all.
    assert_eq!(
        to_html("# h\n#\n# x\n"),
        "<section id=\"h\">\n  <h1>h</h1>\n  <p>#</p>\n</section>\n<section id=\"x\">\n  <h1>x</h1>\n</section>"
    );
}
