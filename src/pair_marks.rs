//! The five bare-pair lint readings shared with the canonical writer.
use unicode_normalization::char::is_combining_mark;

fn next(text: &str, at: usize) -> Option<char> {
    text.get(at..)?.chars().next()
}
fn before(text: &str, at: usize) -> Option<char> {
    text.get(..at)?.chars().next_back()
}
fn escaped(text: &str, at: usize) -> bool {
    text.get(..at)
        .unwrap_or("")
        .bytes()
        .rev()
        .take_while(|ch| *ch == b'\\')
        .count()
        % 2
        == 1
}
fn joined(text: &str, mark: &str, id: &str, at: usize) -> bool {
    id.starts_with("deprecated-bare-")
        && next(text, at + mark.len()).is_some_and(|ch| {
            is_combining_mark(ch)
                || ch == '\u{200d}'
                || ('\u{fe00}'..='\u{fe0f}').contains(&ch)
                || ('\u{e0100}'..='\u{e01ef}').contains(&ch)
        })
}
fn edge_space(ch: char) -> bool {
    ch.is_whitespace() && ch != '\u{feff}'
}
fn valid(text: &str, mark: &str, start: usize, end: usize) -> bool {
    let Some(inside) = text.get(start + mark.len()..end) else {
        return false;
    };
    let first = inside.chars().next();
    let last = inside.chars().next_back();
    if escaped(text, start)
        || inside.is_empty()
        || first.is_some_and(edge_space)
        || last.is_some_and(edge_space)
        || first == mark.chars().next()
        || last == mark.chars().next()
        || (mark == "**" && inside.contains('*'))
    {
        return false;
    }
    mark.len() == 2
        || (!before(text, start).is_some_and(char::is_alphanumeric)
            && !next(text, end + mark.len()).is_some_and(char::is_alphanumeric))
}
/// Source byte ranges of bare pairs. The closer search ignores joined, escaped and space-prefixed candidates.
pub(crate) fn pairs(text: &str, mark: &str, id: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut at = 0;
    while let Some(start) = text
        .get(at..)
        .and_then(|tail| tail.find(mark))
        .map(|p| p + at)
    {
        if joined(text, mark, id, start) {
            at = start + mark.len();
            continue;
        }
        let mut search = start + mark.len() + 1;
        let end = loop {
            let Some(candidate) = text
                .get(search..)
                .and_then(|tail| tail.find(mark))
                .map(|p| p + search)
            else {
                return ranges;
            };
            if before(text, candidate).is_some_and(|ch| ch.is_whitespace() && ch != '\u{feff}')
                || escaped(text, candidate)
                || joined(text, mark, id, candidate)
            {
                search = candidate + mark.len();
                continue;
            }
            break candidate;
        };
        if valid(text, mark, start, end) {
            ranges.push((start, end + mark.len()));
        }
        at = end + mark.len();
    }
    ranges
}
pub(crate) const RULES: [(&str, &str); 5] = [
    ("_", "deprecated-bare-underline"),
    ("~", "deprecated-bare-strike"),
    ("=", "deprecated-bare-highlight"),
    ("~~", "markdown-strikethrough-double-tilde"),
    ("**", "markdown-strong-double-star"),
];
