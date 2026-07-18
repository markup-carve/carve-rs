//! djot.js PR137 / djot-php #264 parity: trailing backslash at EOF is a hard
//! break, and a bare same-level `#` continues a heading.

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
fn bare_same_level_hash_continues_heading() {
    assert_eq!(
        to_html("# h\n#\n# x\n"),
        "<section id=\"h-x\">\n  <h1>h\nx</h1>\n</section>"
    );
}
