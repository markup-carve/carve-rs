//! Trailing whitespace at the very END of a paragraph's final line is not
//! significant and is stripped (CommonMark "final spaces are stripped"; matches
//! carve-php and Djot). Whitespace before a MID-paragraph newline is untouched,
//! so a two-space soft break and a backslash hard break are both preserved.

#[test]
fn final_trailing_space_is_stripped() {
    assert_eq!(carve::to_html("abc "), "<p>abc</p>");
}

#[test]
fn final_trailing_tab_is_stripped() {
    assert_eq!(carve::to_html("abc\t"), "<p>abc</p>");
}

#[test]
fn multiple_final_trailing_spaces_are_stripped() {
    assert_eq!(carve::to_html("abc  "), "<p>abc</p>");
}

#[test]
fn lone_hash_with_trailing_space_is_a_paragraph_without_the_space() {
    // `# ` is not a heading (no heading text) -- it falls back to a paragraph,
    // and the trailing space is stripped.
    assert_eq!(carve::to_html("# "), "<p>#</p>");
}

#[test]
fn plain_paragraph_is_unchanged() {
    assert_eq!(carve::to_html("abc"), "<p>abc</p>");
}

#[test]
fn final_trailing_whitespace_after_a_multi_line_paragraph_is_stripped() {
    // Only the very last line's trailing whitespace is removed; the interior
    // newline (a soft break) is preserved.
    assert_eq!(carve::to_html("foo\nbar  "), "<p>foo\nbar</p>");
}

// --- hard / soft breaks must be preserved ---

#[test]
fn mid_paragraph_two_space_softbreak_is_preserved() {
    // carve treats two trailing spaces before a mid-paragraph newline as a soft
    // break that keeps the literal spaces (only the backslash form is a hard
    // break). The trailing-whitespace strip MUST NOT touch this.
    assert_eq!(carve::to_html("a  \nb"), "<p>a  \nb</p>");
}

#[test]
fn backslash_hard_break_is_preserved() {
    assert_eq!(carve::to_html("a\\\nb"), "<p>a<br>\nb</p>");
}

#[test]
fn backslash_hard_break_survives_trailing_whitespace_on_final_line() {
    // A hard break mid-paragraph stays a `<br>`; the final line's trailing
    // whitespace is still stripped.
    assert_eq!(carve::to_html("a\\\nb  "), "<p>a<br>\nb</p>");
}
