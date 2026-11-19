//! Heading slugs (C2-H-143 to C2-H-148) and the id reservation of C2-H-024.

use crate::chars::is_number;
use crate::options::HeadingIds;
use crate::tables::{SLUG_REMOVED, SLUG_REVERSED};
use std::collections::{HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

/// The slug of a heading's plain text, before any collision suffix.
#[must_use]
pub fn slug(plain: &str, mode: HeadingIds) -> String {
    let mut s = hyphenate(&reverse(plain));
    match mode {
        HeadingIds::Lower => s = lower(&s),
        HeadingIds::Ascii | HeadingIds::AsciiLower => {
            s = hyphenate(&transliterate(&s));
            if mode == HeadingIds::AsciiLower {
                s = lower(&s);
            }
        }
        HeadingIds::AsciiStrict | HeadingIds::AsciiStrictLower => {
            let ascii = transliterate(&s)
                .chars()
                .map(|c| if c.is_ascii() { c } else { ' ' })
                .collect::<String>();
            s = hyphenate(&ascii);
            if mode == HeadingIds::AsciiStrictLower {
                s = lower(&s);
            }
        }
        HeadingIds::KeepCase | HeadingIds::Off => {}
    }
    prefix(s)
}

/// C2-H-144 and C2-H-145: NFC, removed characters, reversed glyphs.
fn reverse(plain: &str) -> String {
    // ASCII is its own NFC and holds no removed or reversed character.
    if plain.is_ascii() {
        return plain.to_string();
    }
    let mut out = String::with_capacity(plain.len());
    for c in plain.nfc() {
        if SLUG_REMOVED.contains(&c) {
            continue;
        }
        match SLUG_REVERSED.iter().find(|(g, _)| *g == c) {
            Some((_, ascii)) => out.push_str(ascii),
            None => out.push(c),
        }
    }
    out
}

/// C2-H-146: each run of other ASCII characters becomes one hyphen, and the
/// hyphens at the ends go.
fn hyphenate(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || !c.is_ascii() {
            if pending && !out.is_empty() {
                out.push('-');
            }
            pending = false;
            out.push(c);
        } else {
            pending = true;
        }
    }
    out
}

/// C2-H-147: each character lowered on its own, with no context.
fn lower(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).collect()
}

/// C2-H-147 leaves the transliteration to the host: this host decomposes and
/// keeps the ASCII part (gap HTML-006).
fn transliterate(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let ascii: String = c.to_string().nfkd().filter(char::is_ascii).collect();
        if ascii.is_empty() {
            out.push(c);
        } else {
            out.push_str(&ascii);
        }
    }
    out
}

/// C2-H-148: a leading number takes `s-`, an empty slug is `s`.
fn prefix(s: String) -> String {
    match s.chars().next() {
        None => "s".to_string(),
        Some(c) if is_number(c) => format!("s-{s}"),
        Some(_) => s,
    }
}

/// The ids already taken, and the suffix search of C2-H-024.
#[derive(Default)]
pub struct Taken {
    set: HashSet<String>,
    /// The next suffix to try per base: ids are never released, so the
    /// search resumes where it stopped (linear time for repeated headings).
    next: HashMap<String, usize>,
}

impl Taken {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Reserves an id; false when it was taken already.
    pub fn reserve(&mut self, id: &str) -> bool {
        self.set.insert(id.to_string())
    }

    /// True when `id` is taken.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.set.contains(id)
    }

    /// The first free id of `base`, `base-2`, `base-3`, ..., reserved.
    pub fn unique(&mut self, base: &str) -> String {
        if self.reserve(base) {
            return base.to_string();
        }
        let mut k = self.next.get(base).copied().unwrap_or(2);
        loop {
            let id = format!("{base}-{k}");
            k += 1;
            if self.reserve(&id) {
                self.next.insert(base.to_string(), k);
                return id;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c2_h_146_runs_become_one_hyphen() {
        assert_eq!(slug("Hello, World!", HeadingIds::KeepCase), "Hello-World");
        assert_eq!(
            slug("Don’t stop – go", HeadingIds::KeepCase),
            "Don-t-stop-go"
        );
    }

    #[test]
    fn c2_h_148_number_prefix_and_empty() {
        assert_eq!(slug("2nd try", HeadingIds::KeepCase), "s-2nd-try");
        assert_eq!(slug("***", HeadingIds::KeepCase), "s");
    }

    #[test]
    fn c2_h_147_lower_has_no_context() {
        assert_eq!(slug("Ärger mit ΣΑΣ", HeadingIds::Lower), "ärger-mit-σασ");
        assert_eq!(slug("İstanbul", HeadingIds::Lower), "i\u{307}stanbul");
    }

    #[test]
    fn c2_h_024_suffix_follows_the_prefix() {
        let mut t = Taken::default();
        assert_eq!(t.unique("s-1"), "s-1");
        assert_eq!(t.unique("s-1"), "s-1-2");
    }
}
