//! Escaping (C2-H-002, C2-H-003) and the character filters of C2-H-172 and
//! C2-H-176.

use crate::chars::ByteSet;

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

/// The C0 controls: the rejected ones and the four whitespace bytes.
const CONTROLS: &[u8] = &[
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
/// The bytes the text escape looks at: all but plain printable ASCII.
static TEXT_BYTES: ByteSet = ByteSet::new(b"&<>", CONTROLS, Some(0x7F));
/// The same for attribute values, quotes included.
static ATTR_BYTES: ByteSet = ByteSet::new(b"&<>\"'", CONTROLS, Some(0x7F));

/// Writes `s` with each character `rep` maps replaced; only the characters
/// that start at a byte of `look` are tested.
fn escape(s: &str, out: &mut String, look: &ByteSet, rep: impl Fn(char) -> Option<&'static str>) {
    let b = s.as_bytes();
    let mut from = 0;
    let mut i = 0;
    while let Some(k) = b.get(i..).and_then(|r| look.find(r)) {
        i += k;
        let Some(c) = s.get(i..).and_then(|r| r.chars().next()) else {
            break;
        };
        let len = c.len_utf8();
        if let Some(r) = rep(c) {
            out.push_str(s.get(from..i).unwrap_or(""));
            out.push_str(r);
            from = i + len;
        }
        i += len;
    }
    out.push_str(s.get(from..).unwrap_or(""));
}

fn text_rep(c: char) -> Option<&'static str> {
    Some(match c {
        '&' => "&amp;",
        '<' => "&lt;",
        '>' => "&gt;",
        '\u{a0}' => "&nbsp;",
        c if bidi(c) => "",
        c if rejected(c) => "\u{FFFD}",
        _ => return None,
    })
}

fn attr_rep(c: char) -> Option<&'static str> {
    Some(match c {
        '&' => "&amp;",
        '<' => "&lt;",
        '>' => "&gt;",
        '"' => "&quot;",
        '\'' => "&apos;",
        c if rejected(c) => "\u{FFFD}",
        _ => return None,
    })
}

/// Appends text content (C2-H-002).
pub fn text(s: &str, out: &mut String) {
    escape(s, out, &TEXT_BYTES, text_rep);
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
    escape(s, out, &ATTR_BYTES, attr_rep);
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
    fn escapes_match_a_char_by_char_reference() {
        let reference = |s: &str, rep: fn(char) -> Option<&'static str>| {
            s.chars().fold(String::new(), |mut o, c| {
                match rep(c) {
                    Some(r) => o.push_str(r),
                    None => o.push(c),
                }
                o
            })
        };
        let all: String = (0..0x3_0000).filter_map(char::from_u32).collect();
        for s in [
            all.as_str(),
            "plain ascii",
            "é&ü<\u{202A}>\u{FFFE}\"'\u{a0}\u{85}x",
        ] {
            let mut t = String::new();
            text(s, &mut t);
            assert_eq!(t, reference(s, text_rep));
            let mut a = String::new();
            attr(s, &mut a);
            assert_eq!(a, reference(s, attr_rep));
        }
    }

    #[test]
    fn c2_h_003_attribute_escapes_quotes() {
        let mut s = String::new();
        attr("\"'\u{a0}", &mut s);
        assert_eq!(s, "&quot;&apos;\u{a0}");
    }
}
