//! Character classes the clauses name.

use unicode_general_category::{GeneralCategory as G, get_general_category};

/// A word character: a Unicode letter or number, category L or N (inline preliminaries).
#[must_use]
pub fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric();
    }
    matches!(
        get_general_category(c),
        G::UppercaseLetter
            | G::LowercaseLetter
            | G::TitlecaseLetter
            | G::ModifierLetter
            | G::OtherLetter
            | G::DecimalNumber
            | G::LetterNumber
            | G::OtherNumber
    )
}

/// A Unicode number, category N (C2-H-148).
#[must_use]
pub fn is_number(c: char) -> bool {
    matches!(
        get_general_category(c),
        G::DecimalNumber | G::LetterNumber | G::OtherNumber
    )
}

/// Whitespace for the tests of the character before or after a position:
/// space, tab, line break, and the no-break space (inline preliminaries).
#[must_use]
pub const fn is_flank_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{a0}')
}

/// A combining mark, a variation selector or a zero-width joiner (C2-I-050).
#[must_use]
pub fn is_joining(c: char) -> bool {
    matches!(c, '\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
        || matches!(
            get_general_category(c),
            G::NonspacingMark | G::SpacingMark | G::EnclosingMark
        )
}

/// A control (Cc) or format (Cf) character (C2-I-122).
#[must_use]
pub fn is_control_or_format(c: char) -> bool {
    matches!(get_general_category(c), G::Control | G::Format)
}

/// Space or tab.
#[must_use]
pub const fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// A line that holds only spaces and tabs.
#[must_use]
pub fn is_blank_line(s: &str) -> bool {
    s.bytes().all(|b| b == b' ' || b == b'\t')
}

/// The lookup key of a label (C2-I-079): runs of spaces, tabs and line breaks
/// fold to one space, the ends are trimmed, case is kept.
#[must_use]
pub fn fold_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}
