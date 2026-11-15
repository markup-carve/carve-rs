//! The URL policy (C2-H-128, C2-H-159 to C2-H-162, C2-H-173): the scheme
//! probe, the destination test and the authored attribute filter.

use crate::options::UrlPolicy;
use crate::tables::{
    DEFAULT_DENIED_SCHEMES, DROPPED_ATTRIBUTES, PROBE_REMOVES, STYLE_PATTERNS, URL_LIST_ATTRIBUTES,
};

fn probe_removed(c: char) -> bool {
    PROBE_REMOVES
        .iter()
        .any(|(lo, hi)| (*lo..=*hi).contains(&c))
}

/// The scheme of a destination, lowered, after the probe removals of
/// C2-H-159; `None` for a destination with no scheme.
#[must_use]
pub fn scheme(dest: &str) -> Option<String> {
    let mut out = String::new();
    for c in dest.chars().filter(|c| !probe_removed(*c)) {
        if c == ':' {
            return (!out.is_empty()).then_some(out);
        }
        let ok = if out.is_empty() {
            c.is_ascii_alphabetic()
        } else {
            c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
        };
        if !ok {
            return None;
        }
        out.push(c.to_ascii_lowercase());
    }
    None
}

/// True when `dest` passes `policy` (C2-H-160).
#[must_use]
pub fn allowed(dest: &str, policy: &UrlPolicy) -> bool {
    let Some(s) = scheme(dest) else { return true };
    match policy {
        UrlPolicy::Deny(list) => !list.iter().any(|d| d.eq_ignore_ascii_case(&s)),
        UrlPolicy::Allow(list) => list.iter().any(|d| d.eq_ignore_ascii_case(&s)),
    }
}

/// An attribute name that is never written (C2-H-161).
#[must_use]
pub fn dropped_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    DROPPED_ATTRIBUTES.iter().any(|(n, how)| {
        if *how == "prefix" {
            lower.starts_with(n)
        } else {
            lower == *n
        }
    })
}

/// C2-H-161: the text before the first colon names a scheme of the default
/// deny list, whatever `urlPolicy` holds.
#[must_use]
pub fn value_fails(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    let head: String = value
        .get(..colon)
        .unwrap_or("")
        .chars()
        .filter(|c| !probe_removed(*c))
        .map(|c| c.to_ascii_lowercase())
        .collect();
    DEFAULT_DENIED_SCHEMES.contains(&head.as_str())
}

/// The value an authored attribute is written with: emptied when it fails
/// C2-H-161 or C2-H-162.
#[must_use]
pub fn filter_value(name: &str, value: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut fails = value_fails(value);
    if let Some((_, commas)) = URL_LIST_ATTRIBUTES.iter().find(|(n, _)| *n == lower) {
        let split = value.split(|c: char| c.is_ascii_whitespace() || (*commas && c == ','));
        fails = fails || split.filter(|t| !t.is_empty()).any(value_fails);
    }
    if lower == "style" {
        fails = fails || style_fails(value);
    }
    if fails {
        String::new()
    } else {
        value.to_string()
    }
}

/// C2-H-162: a style value that holds a pattern after decoding.
#[must_use]
pub fn style_fails(value: &str) -> bool {
    let decoded: String = decode_css(value)
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    STYLE_PATTERNS.iter().any(|p| decoded.contains(p))
}

fn decode_css(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut it = value.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 && it.peek().is_some_and(char::is_ascii_hexdigit) {
            if let Some(h) = it.next() {
                hex.push(h);
            }
        }
        if !hex.is_empty() {
            let cp = u32::from_str_radix(&hex, 16)
                .ok()
                .and_then(char::from_u32)
                .unwrap_or('\u{FFFD}');
            out.push(cp);
            if it.peek().is_some_and(|c| c.is_whitespace()) {
                it.next();
            }
            continue;
        }
        match it.next() {
            Some('\n') | None => {}
            Some(other) => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c2_h_159_probe_removes_controls() {
        assert_eq!(scheme("java\u{1}script:x").as_deref(), Some("javascript"));
        assert_eq!(scheme("/a#b"), None);
        assert!(!allowed("JavaScript:x", &UrlPolicy::default()));
    }

    #[test]
    fn c2_h_161_value_before_colon() {
        assert_eq!(filter_value("title", "Shell: bash"), "");
        assert_eq!(filter_value("ping", "java script:x"), "");
        assert_eq!(filter_value("title", "ok: yes"), "ok: yes");
        assert!(dropped_name("onClick"));
    }

    #[test]
    fn c2_h_162_style_escapes_decode() {
        assert!(style_fails("color: u\\72l(a)"));
        assert!(!style_fails("color: red"));
    }
}
