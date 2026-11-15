//! Abbreviations (C2-I-098 to C2-I-100): each whole-word occurrence of a
//! defined term in plain text is an `abbreviation` node.

use crate::chars::is_word;

/// A match: byte range in the text and the index of the term.
pub type Hit = (usize, usize, usize);

/// Finds the terms in `s`. `before` and `after` are the flow characters
/// around `s`; an element edge is no boundary of its own (C2-I-099).
#[must_use]
pub fn find(
    s: &str,
    before: Option<char>,
    after: Option<char>,
    terms: &[(String, String)],
) -> Vec<Hit> {
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

fn longest(
    s: &str,
    p: usize,
    after: Option<char>,
    terms: &[(String, String)],
) -> Option<(usize, usize)> {
    let rest = s.get(p..)?;
    let mut best: Option<(usize, usize)> = None;
    for (idx, (term, _)) in terms.iter().enumerate() {
        if term.is_empty()
            || !rest.starts_with(term.as_str())
            || best.is_some_and(|(l, _)| l >= term.len())
        {
            continue;
        }
        let next =
            rest.get(term.len()..)
                .and_then(|x| x.chars().next())
                .or(if term.len() == rest.len() {
                    after
                } else {
                    None
                });
        if next.is_none_or(|n| !is_word(n)) {
            best = Some((term.len(), idx));
        }
    }
    best
}
