//! Escaping (C2-H-002, C2-H-003) and the character filters of C2-H-172 and
//! C2-H-176.

/// A character the HTML input stream rejects (C2-H-176).
#[must_use]
pub fn rejected(c: char) -> bool {
    let cp = u32::from(c);
    (cp < 0x20 && !matches!(c, '\t' | '\n' | '\x0C' | '\r'))
        || (0x7F..=0x9F).contains(&cp)
        || (0xFDD0..=0xFDEF).contains(&cp)
        || (cp & 0xFFFE) == 0xFFFE
}

/// A bidirectional control that is never written between tags (C2-H-172).
#[must_use]
pub fn bidi(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Appends text content (C2-H-002).
pub fn text(s: &str, out: &mut String) {
    let mut from = 0;
    for (i, c) in s.char_indices() {
        let rep = match c {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '\u{a0}' => "&nbsp;",
            c if bidi(c) => "",
            c if rejected(c) => "\u{FFFD}",
            _ => continue,
        };
        out.push_str(s.get(from..i).unwrap_or(""));
        out.push_str(rep);
        from = i + c.len_utf8();
    }
    out.push_str(s.get(from..).unwrap_or(""));
}

/// Text content as a new string.
#[must_use]
pub fn text_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    text(s, &mut out);
    out
}

/// Appends an attribute value (C2-H-003): a no-break space and the
/// bidirectional controls stay.
pub fn attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if rejected(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c2_h_002_text_escapes_and_nbsp() {
        assert_eq!(text_str("a&<>\u{a0}\"'"), "a&amp;&lt;&gt;&nbsp;\"'");
    }

    #[test]
    fn c2_h_172_bidi_controls_leave_text() {
        assert_eq!(text_str("a\u{202E}b\u{2066}c"), "abc");
    }

    #[test]
    fn c2_h_176_rejected_characters_become_replacement() {
        assert_eq!(
            text_str("a\u{1}b\u{85}c\u{FFFF}"),
            "a\u{FFFD}b\u{FFFD}c\u{FFFD}"
        );
    }

    #[test]
    fn c2_h_003_attribute_escapes_quotes() {
        let mut s = String::new();
        attr("\"'\u{a0}", &mut s);
        assert_eq!(s, "&quot;&apos;\u{a0}");
    }
}
