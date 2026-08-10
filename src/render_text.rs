use crate::escape::is_bidi_control;

/// Drop EVERY control character (keeping tab and newline) from author content,
/// so an attacker's `ESC` / OSC sequence cannot inject into terminal output.
///
/// THE TERMINAL TARGET ONLY (PART 9 §29 T4). It is the one target whose consumer
/// ACTS on the character: a form feed feeds or clears, and U+001B introduces a
/// sequence that can move the cursor, rewrite earlier output or reach the
/// clipboard. That is a property of the DEVICE, so it reaches this target and no
/// other. The breadth is deliberate and T4 says so in as many words: §25
/// NON-HTML TARGETS requires DEL (U+007F) and the C1 controls to go too, because
/// CSI (U+009B) and OSC (U+009D) are single-character forms of the very
/// sequences the requirement exists to stop. Narrowing this to C0 would be a
/// security regression.
///
/// The Markdown and plain-text targets use [`strip_high_controls`] instead.
pub(crate) fn strip_terminal_controls(input: &str) -> String {
    input
        .chars()
        .filter(|c| (*c == '\t' || *c == '\n' || !c.is_control()) && !is_bidi_control(*c))
        .collect()
}

/// Drop DEL and the C1 controls, and NOTHING BELOW U+007F, from author content.
///
/// What the Markdown and plain-text targets strip (PART 9 §29 T2, T3). After
/// markup-carve/carve#963 the whitespace of this language is exactly U+0020,
/// U+0009, U+000A and U+000D; every other C0 control - U+0000..U+0008, U+000B,
/// U+000C, U+000E..U+001F - is ordinary CONTENT that parses as content, survives
/// into the AST, and satisfies no whitespace slot. §29 then says what each target
/// does with that content, and for these two the answer is EMIT: a target that
/// silently removes content is lossy in exactly the way markup-carve/carve#817
/// rejected for the wire, and the reason first offered for the strip - that a
/// Markdown reader would reclassify these as whitespace - was measured against
/// four readers and did not hold.
///
/// DEL AND THE C1 CONTROLS ARE NOT PART OF THAT, and stay stripped here. §29 T5
/// puts them outside the section explicitly and leaves them to a ticket of their
/// own; removing them from this filter as well would have made this change
/// introduce that defect rather than leave it where it is
/// (markup-carve/carve-rs#812).
///
/// NEITHER IS U+000D, and for the opposite reason: carve#963 makes it
/// WHITESPACE, so §29's class - "every OTHER C0 control" - excludes it and this
/// section rules on it not at all. The parser never lets one through (a CRLF is
/// normalized before any block is read), so it can only arrive on a tree built
/// through the API or read by `from_json`, where it is a LINE TERMINATOR inside
/// a leaf the writer is laying out in lines of its own: a Markdown reader may
/// take it as a line boundary, and on a terminal it returns the cursor over what
/// was already printed. The previous filter dropped it and so does this one -
/// leaving a character §29 does not govern exactly where it was (raised by
/// `codex review`).
pub(crate) fn strip_high_controls(input: &str) -> String {
    if !input.chars().any(is_not_emitted) {
        return input.to_string();
    }
    input.chars().filter(|c| !is_not_emitted(*c)).collect()
}

/// DEL (U+007F), the C1 controls (U+0080..U+009F), the carriage return, and
/// §26's bidi override/isolate controls on every presentation target.
fn is_not_emitted(c: char) -> bool {
    matches!(c, '\u{7f}'..='\u{9f}' | '\r') || is_bidi_control(c)
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
