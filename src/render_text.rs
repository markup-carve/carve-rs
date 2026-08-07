/// Drop C0/C1 control characters (keeping tab and newline) from author content
/// so attacker `ESC` / OSC sequences cannot inject into terminal output (the
/// ANSI and plain-text renderers). The renderers' own styling escapes are added
/// separately and are not affected.
pub(crate) fn strip_controls(input: &str) -> String {
    input
        .chars()
        .filter(|c| *c == '\t' || *c == '\n' || !c.is_control())
        .collect()
}

/// A renderer's whitespace terminal: U+0020 and U+0009, and NOTHING ELSE.
///
/// `blank_line = {whitespace}` takes a space or a tab (PART 1, carve#890), and
/// PART 2's NO TRAILING WHITESPACE drops the same two (carve#926). Every other
/// character is CONTENT and has to reach the output, however invisible - a
/// no-break space, an OGHAM SPACE MARK, an EN QUAD, a THIN SPACE, a NARROW
/// NO-BREAK SPACE, a MEDIUM MATHEMATICAL SPACE, an IDEOGRAPHIC SPACE, a
/// zero-width space, a FORM FEED and a VERTICAL TAB.
///
/// The two LINE TERMINATORS are in the set as well, and that is a different
/// job: these helpers also trim the newlines around a rendered block, which is
/// layout rather than line content. A form feed is NOT a terminator here - it
/// is content that PART 2 keeps - so the set is named rather than spelled
/// `is_ascii_whitespace`, which would take it.
///
/// This was the Unicode whitespace PROPERTY with U+00A0 carved out by hand,
/// which is the shape the rule keeps being written in wrongly: the one
/// character anyone thinks of survived and the other eight did not, so a line
/// holding one of them was written back EMPTY and reparsed as a blank - which
/// split its paragraph in two and lost the character. Naming the two terminal
/// characters removes the exception along with the defect.
///
/// It lives HERE, shared, because `str::trim` is the default reach for "drop
/// the layout around this rendered fragment", and `str::trim` takes
/// `char::is_whitespace`, U+00A0 included. The canonical writer learned that
/// once and kept its own copy; the plain-text, Markdown and ANSI writers each
/// went on calling `.trim()` on footnote-definition bodies, table cells and a
/// caption, so the same character was preserved on one target and deleted on
/// three by the same engine (carve-rs#614 fixed the Markdown heading case
/// alone). One spelling, in the module the presentation renderers already
/// share for exactly this.
pub(crate) fn trim_non_nbsp(text: &str) -> &str {
    text.trim_matches([' ', '\t', '\n', '\r'])
}

/// `trim_non_nbsp`, at the end only.
pub(crate) fn trim_end_non_nbsp(text: &str) -> &str {
    text.trim_end_matches([' ', '\t', '\n', '\r'])
}
