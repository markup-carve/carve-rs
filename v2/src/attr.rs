//! The attribute block grammar (C2-I-028 to C2-I-042, `attribute-block.json`),
//! shared by attribute lines, marker attributes, cell prefixes and inline hosts.

use crate::ast::Attrs;

/// One parsed brace block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttrBlock {
    pub attrs: Attrs,
    /// The language part (C2-I-029 `:tag`), last one written wins.
    pub lang: Option<String>,
    /// The block holds no part (`{}` or braces around spaces, C2-I-040).
    pub empty: bool,
}

impl AttrBlock {
    /// Merges a later glued block (C2-I-039).
    pub fn merge(&mut self, later: Self) {
        self.attrs.merge(later.attrs);
        if later.lang.is_some() {
            self.lang = later.lang;
        }
        self.empty = self.empty && later.empty;
    }
}

const fn name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Parses a brace block at byte `at` of `s` (which must hold `{` there).
/// Returns the block and the offset behind its `}`. A block never crosses a
/// line break (C2-I-028).
#[must_use]
pub fn parse_block(s: &str, at: usize) -> Option<(AttrBlock, usize)> {
    let b = s.as_bytes();
    if b.get(at) != Some(&b'{') {
        return None;
    }
    let mut i = at + 1;
    let mut block = AttrBlock::default();
    let mut parts = 0usize;
    loop {
        let before = i;
        while b.get(i) == Some(&b' ') {
            i += 1;
        }
        match b.get(i) {
            Some(b'}') => {
                block.empty = parts == 0;
                return Some((block, i + 1));
            }
            None | Some(b'\n') => return None,
            _ => {}
        }
        if parts > 0 && before == i {
            return None;
        }
        i = parse_part(s, i, &mut block)?;
        parts += 1;
    }
}

fn parse_part(s: &str, i: usize, block: &mut AttrBlock) -> Option<usize> {
    let b = s.as_bytes();
    match b.get(i)? {
        b'#' => {
            let end = run(b, i + 1, name_char);
            (end > i + 1).then(|| {
                block.attrs.id = Some(s[i + 1..end].to_string());
                end
            })
        }
        b'.' => {
            let end = run(b, i + 1, name_char);
            (end > i + 1).then(|| {
                block.attrs.classes.push(s[i + 1..end].to_string());
                end
            })
        }
        b':' => {
            let end = run(b, i + 1, |c| c.is_ascii_alphanumeric() || c == b'-');
            let tag = &s[i + 1..end];
            let ok = tag.is_empty() || (!tag.starts_with('-') && !tag.ends_with('-') && !tag.contains("--"));
            ok.then(|| {
                block.lang = Some(tag.to_string());
                end
            })
        }
        _ => parse_key(s, i, block),
    }
}

fn run(b: &[u8], mut i: usize, f: impl Fn(u8) -> bool) -> usize {
    while b.get(i).is_some_and(|&c| f(c)) {
        i += 1;
    }
    i
}

fn parse_key(s: &str, i: usize, block: &mut AttrBlock) -> Option<usize> {
    let b = s.as_bytes();
    let first = *b.get(i)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let end = run(b, i + 1, name_char);
    let key = &s[i..end];
    if b.get(end) != Some(&b'=') {
        // C2-I-031: a bare key does not start with `_`.
        if first == b'_' {
            return None;
        }
        add_key(block, key, String::new());
        return Some(end);
    }
    let (value, after) = parse_value(s, end + 1)?;
    add_key(block, key, value);
    Some(after)
}

fn parse_value(s: &str, i: usize) -> Option<(String, usize)> {
    let b = s.as_bytes();
    match b.get(i)? {
        q @ (b'"' | b'\'') => {
            let q = *q;
            let mut out = String::new();
            let mut j = i + 1;
            loop {
                let c = s.get(j..)?.chars().next()?;
                match c {
                    '\n' => return None,
                    '\\' => {
                        let next = s.get(j + 1..)?.chars().next()?;
                        if next == '\n' {
                            return None;
                        }
                        if !next.is_ascii_punctuation() {
                            out.push('\\');
                        }
                        out.push(next);
                        j += 1 + next.len_utf8();
                    }
                    c if c as u32 == u32::from(q) => return Some((out, j + 1)),
                    c => {
                        out.push(c);
                        j += c.len_utf8();
                    }
                }
            }
        }
        _ => {
            let end = run(b, i, |c| !matches!(c, b' ' | b'\t' | b'"' | b'\'' | b'\\' | b'|' | b'}' | b'\n'));
            (end > i).then(|| (s[i..end].to_string(), end))
        }
    }
}

/// C2-I-042: `id` and `class` keys fold, any ASCII case; other keys merge by
/// name without case, the later value at the first place (C2-A-035).
fn add_key(block: &mut AttrBlock, key: &str, value: String) {
    if key.eq_ignore_ascii_case("id") {
        block.attrs.id = Some(value);
    } else if key.eq_ignore_ascii_case("class") {
        block.attrs.classes.extend(value.split([' ', '\t']).filter(|c| !c.is_empty()).map(String::from));
    } else if let Some(slot) = block.attrs.key_values.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        slot.1 = value;
    } else {
        block.attrs.key_values.push((key.to_string(), value));
    }
}

/// Parses a run of glued brace blocks at `at`. Returns the merged block, the
/// end of the run, and whether any block of the run was the empty one.
#[must_use]
pub fn parse_glued(s: &str, at: usize) -> Option<(AttrBlock, usize)> {
    let (mut block, mut end) = parse_block(s, at)?;
    while let Some((next, e)) = parse_block(s, end) {
        block.merge(next);
        end = e;
    }
    Some((block, end))
}
