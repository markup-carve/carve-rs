//! Trailing whitespace on a CONTENT LINE is dropped - on every line of a
//! paragraph, not only its last (PART 2 NO TRAILING WHITESPACE, carve#926).
//!
//! The rule used to be written here as "the very END of a paragraph's final
//! line", with whitespace before a MID-paragraph newline untouched. That was a
//! correct reading of PART 12 §7 at the time, which claimed `a` + SPACE +
//! newline + `b` renders `<p>a \nb</p>`; §7 has since been corrected, because
//! the executable spec does not render it that way.
//!
//! A backslash HARD BREAK is unaffected: the line ends in a backslash, which is
//! content, so it does not end in whitespace at all. Carve has no two-space
//! hard break for the strip to eat.

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
    // The interior newline (a soft break) is preserved; only the whitespace
    // goes.
    assert_eq!(carve::to_html("foo\nbar  "), "<p>foo\nbar</p>");
}

// --- hard / soft breaks must be preserved ---

#[test]
fn a_run_before_a_soft_break_is_dropped_like_any_other() {
    // These two documents are the same document, which is the whole of
    // carve#926's first half. The run used to survive here because the strip
    // acted on the joined buffer's END rather than on each line.
    assert_eq!(carve::to_html("a  \nb"), "<p>a\nb</p>");
    assert_eq!(carve::to_html("a \nb"), "<p>a\nb</p>");
    assert_eq!(carve::to_html("a\t\nb"), "<p>a\nb</p>");
    assert_eq!(carve::to_html("a\nb"), "<p>a\nb</p>");
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
