//! Terminal helpers of the ANSI export: display width and the Unicode forms
//! of superscript and subscript.

use super::common::strip_bidi;

/// Columns `text` takes in a terminal; bidirectional controls are dropped
/// from the output, so they take none.
pub fn display_width(text: &str) -> usize {
    crate::write::display_width(&strip_bidi(text))
}

fn superscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        _ => return None,
    })
}

fn subscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'o' => 'ₒ',
        'x' => 'ₓ',
        _ => return None,
    })
}

/// Maps each character that has a form, leaving SGR sequences of styled children intact.
fn map_outside_sgr(text: &str, map: fn(char) -> Option<char>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let sgr = rest.find("\x1b[").and_then(|at| {
            let after = &rest[at + 2..];
            let end = after.find(|c: char| !(c.is_ascii_digit() || c == ';'))?;
            after[end..]
                .starts_with('m')
                .then_some((at, at + 2 + end + 1))
        });
        let (plain, seq) = match sgr {
            Some((start, end)) => (&rest[..start], Some(&rest[start..end])),
            None => (rest, None),
        };
        out.extend(plain.chars().map(|c| map(c).unwrap_or(c)));
        match seq {
            Some(seq) => {
                out.push_str(seq);
                rest = &rest[plain.len() + seq.len()..];
            }
            None => return out,
        }
    }
}

pub fn to_superscript(text: &str) -> String {
    map_outside_sgr(text, superscript)
}

pub fn to_subscript(text: &str) -> String {
    map_outside_sgr(text, subscript)
}
