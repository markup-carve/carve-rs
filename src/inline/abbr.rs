//! Abbreviations (C2-I-098 to C2-I-100): each whole-word occurrence of a
//! defined term in plain text is an `abbreviation` node.

use super::Terms;
use crate::chars::is_word;

/// A match: byte range in the text and the index of the term.
pub type Hit = (usize, usize, usize);

/// Finds the terms in `s`. `before` and `after` are the flow characters
/// around `s`; an element edge is no boundary of its own (C2-I-099).
#[must_use]
pub fn find(s: &str, before: Option<char>, after: Option<char>, terms: &Terms<'_>) -> Vec<Hit> {
    let mut hits = Vec::new();
    if terms.is_empty() {
        return hits;
    }
    let mut prev = before;
    let mut p = 0;
    while p < s.len() {
        let Some(c) = s.get(p..).and_then(|x| x.chars().next()) else {
            break;
        };
        if prev.is_none_or(|b| !is_word(b))
            && let Some((len, idx)) = longest(s, p, after, terms)
        {
            hits.push((p, p + len, idx));
            prev = s.get(..p + len).and_then(|x| x.chars().next_back());
            p += len;
            continue;
        }
        prev = Some(c);
        p += c.len_utf8();
    }
    hits
}

/// C2-B-084: a term is ASCII letters and digits, and C2-I-099 wants no word
/// character after it, so a hit is exactly the ASCII run at `p`: one binary
/// search in `terms` (linear time).
fn longest(s: &str, p: usize, after: Option<char>, terms: &Terms<'_>) -> Option<(usize, usize)> {
    let rest = s.get(p..)?;
    let len = rest
        .bytes()
        .position(|b| !b.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    let run = rest.get(..len).filter(|r| !r.is_empty())?;
    let idx = terms.find(run)?;
    let next = rest
        .get(len..)
        .and_then(|x| x.chars().next())
        .or(if len == rest.len() { after } else { None });
    next.is_none_or(|n| !is_word(n)).then_some((len, idx))
}
