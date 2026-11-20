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
fn zs(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\u{a0}' | '\u{1680}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    ) || ('\u{2000}'..='\u{200a}').contains(&ch)
}
fn edge_space(ch: char) -> bool {
    zs(ch) || matches!(ch, '\t' | '\n' | '\r' | '\u{c}' | '\u{b}')
}
fn closer_space(ch: char) -> bool {
    zs(ch) || matches!(ch, '\t' | '\n' | '\r')
}
fn can_open(text: &str, mark: &str, id: &str, at: usize) -> bool {
    if !id.starts_with("deprecated-bare-") {
        return true;
    }
    if joined(text, mark, id, at) || escaped(text, at) {
        return false;
    }
    if id.starts_with("deprecated-bare-") && before(text, at) == mark.chars().next() {
        return false;
    }
    let Some(after) = next(text, at + mark.len()) else {
        return false;
    };
    if edge_space(after) || mark.starts_with(after) {
        return false;
    }
    mark.len() == 2 || !before(text, at).is_some_and(char::is_alphanumeric)
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
        if !can_open(text, mark, id, start) {
            at = start + mark.len();
            continue;
        }
        let after_opener = start + mark.len();
        let mut search = after_opener + next(text, after_opener).map_or(0, char::len_utf8);
        let end = loop {
            let Some(candidate) = text
                .get(search..)
                .and_then(|tail| tail.find(mark))
                .map(|p| p + search)
            else {
                return ranges;
            };
            if before(text, candidate).is_some_and(closer_space)
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

#[cfg(test)]
mod tests {
    use super::pairs;

    #[test]
    fn only_catalog_edge_spaces_exclude_bare_pairs() {
        assert_eq!(
            pairs("~a\u{85}~", "~", "deprecated-bare-strike"),
            vec![(0, 5)]
        );
        assert!(pairs("~a\u{a0}~", "~", "deprecated-bare-strike").is_empty());
    }
}
