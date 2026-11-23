//! The pair scan of a text and a mark (C2-W-147), as the bare pair rules of the lint model read it.

use regex::Regex;
use std::sync::OnceLock;

/// The named groups of one pair as byte spans of the text: `open`, `close`, `inner`, `before`, `backslashes`,
/// `after`.
pub(super) type Pair = [(&'static str, usize, usize); 6];

fn class(re: &'static OnceLock<Option<Regex>>, pattern: &str, c: char) -> bool {
    let mut buf = [0; 4];
    re.get_or_init(|| Regex::new(pattern).ok())
        .as_ref()
        .is_some_and(|re| re.is_match(c.encode_utf8(&mut buf)))
}

fn letter_or_number(c: char) -> bool {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    class(&RE, r"\A[\p{L}\p{N}]\z", c)
}

fn space(c: char) -> bool {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    matches!(c, '\t' | '\n' | '\r') || class(&RE, r"\A\p{Zs}\z", c)
}

/// A combining mark, a variation selector or a zero-width joiner (C2-I-050).
fn joins(c: char) -> bool {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    matches!(c, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}' | '\u{200D}')
        || class(&RE, r"\A\p{M}\z", c)
}

fn before(text: &str, at: usize) -> Option<char> {
    text.get(..at)?.chars().next_back()
}

fn after(text: &str, at: usize) -> Option<char> {
    text.get(at..)?.chars().next()
}

fn escaped(text: &str, at: usize) -> bool {
    text.as_bytes()
        .get(..at)
        .unwrap_or_default()
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}

fn can_open(text: &str, at: usize, mark: &str) -> bool {
    let own = mark.chars().next();
    let end = at + mark.len();
    let behind = after(text, end);
    let ahead = before(text, at);
    !(behind.is_none_or(|c| joins(c) || space(c) || matches!(c, '\x0C' | '\x0B') || Some(c) == own)
        || escaped(text, at)
        || ahead.is_some_and(|c| Some(c) == own)
        || (mark.chars().count() == 1 && ahead.is_some_and(letter_or_number)))
}

fn can_close(text: &str, at: usize, mark: &str, strict: bool) -> bool {
    !(before(text, at).is_some_and(space)
        || escaped(text, at)
        || (strict && after(text, at + mark.len()).is_some_and(joins)))
}

/// The pairs of the scan, left to right; a strict scan passes openers that cannot open a bare pair.
pub(super) fn scan(text: &str, mark: &str, strict: bool) -> Vec<Pair> {
    let mut out = Vec::new();
    let mut from = 0;
    if mark.is_empty() {
        return out;
    }
    while let Some(found) = text.get(from..).and_then(|rest| rest.find(mark)) {
        let open = from + found;
        let open_end = open + mark.len();
        if strict && !can_open(text, open, mark) {
            from = open_end;
            continue;
        }
        // The closer starts at least one character after the opener ends.
        let Some(first) = after(text, open_end) else {
            break;
        };
        let mut search = open_end + first.len_utf8();
        let close = loop {
            let Some(found) = text.get(search..).and_then(|rest| rest.find(mark)) else {
                break None;
            };
            let at = search + found;
            if can_close(text, at, mark, strict) {
                break Some(at);
            }
            search = at + after(text, at).map_or(1, char::len_utf8);
        };
        let Some(close) = close else {
            break;
        };
        let close_end = close + mark.len();
        let before_start = before(text, open).map_or(open, |c| open - c.len_utf8());
        let after_end = after(text, close_end).map_or(close_end, |c| close_end + c.len_utf8());
        let run = text
            .as_bytes()
            .get(..open)
            .unwrap_or_default()
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count();
        out.push([
            ("open", open, open_end),
            ("close", close, close_end),
            ("inner", open_end, close),
            ("before", before_start, open),
            ("backslashes", open - run, open),
            ("after", close_end, after_end),
        ]);
        from = close_end;
    }
    out
}
