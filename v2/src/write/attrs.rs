//! The brace block of a node (C2-W-047 to C2-W-050), the fence runs of C2-W-038
//! and the list markers of C2-W-044 and C2-W-045.

use super::node::{attrs, classes, key_values, s, ty};
use super::state::Writer;
use serde_json::Value;

/// C2-I-030: a name is one or more ASCII letters, digits, `_` and `-`.
pub fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// C2-I-030: a key is an ASCII letter or `_`, then name characters.
pub fn is_key(text: &str) -> bool {
    is_name(text)
        && text
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
}

/// C2-I-032: an unquoted value has no space, tab, quote, backslash, pipe or closing brace.
fn is_unquoted(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|c| " \t\"'\\|}".contains(c))
}

/// `value` in double quotes (C2-I-033): a quote takes a backslash, and so does
/// a backslash before ASCII punctuation or the closing quote.
pub fn quoted(value: &str, pipes: bool) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    let mut it = value.chars().peekable();
    while let Some(c) = it.next() {
        let next = it.peek().copied().unwrap_or('"');
        if c == '"' || (c == '\\' && next.is_ascii_punctuation()) || (pipes && c == '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn key_part(key: &str, value: &str, pipes: bool) -> String {
    if value.is_empty() {
        // C2-I-031: a bare key does not start with `_`.
        return if key.starts_with('_') {
            format!("{key}=\"\"")
        } else {
            key.to_string()
        };
    }
    if is_unquoted(value) {
        format!("{key}={value}")
    } else {
        format!("{key}={}", quoted(value, pipes))
    }
}

#[derive(Debug, Default, Clone)]
pub struct BlockParts {
    /// The alignment mark of a cell, before the id (C2-W-062).
    pub mark: String,
    /// Keys that the tree holds in other fields, written before the keys of `attrs`.
    pub keys: Vec<(String, String)>,
    /// Classes the opener stands for (C2-W-039, C2-W-081).
    pub skip_classes: Vec<usize>,
    /// Keys of `attrs` that `keys` writes from other fields (C2-W-136).
    pub skip_keys: Vec<String>,
    /// C2-B-166, C2-B-169: the block stands in a table row.
    pub pipes: bool,
}

/// C2-W-048: the brace block of `node`, or the empty string when it has nothing to write.
pub fn brace_block(node: &Value, extra: &BlockParts) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !extra.mark.is_empty() {
        parts.push(extra.mark.clone());
    }
    if let Some(id) = attrs(node)
        .and_then(|a| a.get("id"))
        .and_then(Value::as_str)
    {
        parts.push(if is_name(id) {
            format!("#{id}")
        } else {
            format!("id={}", quoted(id, extra.pipes))
        });
    }
    for (at, name) in classes(node).into_iter().enumerate() {
        if extra.skip_classes.contains(&at) {
            continue;
        }
        // C2-W-048: a class the `.name` form cannot hold is a quoted `class` key at its place.
        parts.push(if is_name(name) {
            format!(".{name}")
        } else {
            format!("class={}", quoted(name, extra.pipes))
        });
    }
    if let Some(lang) = s(node, "lang") {
        parts.push(format!(":{lang}"));
    }
    for (k, v) in &extra.keys {
        parts.push(key_part(k, v, extra.pipes));
    }
    for (k, v) in key_values(node) {
        if is_key(k) && !extra.skip_keys.iter().any(|x| x == k) {
            parts.push(key_part(k, v, extra.pipes));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", parts.join(" "))
    }
}

pub fn plain_block(node: &Value) -> String {
    brace_block(node, &BlockParts::default())
}

/// C2-W-038: the longest bare run of `ch` on a line, allowing outer spaces and tabs.
pub fn longest_bare_run<'a>(lines: impl IntoIterator<Item = &'a str>, ch: u8) -> usize {
    let mut longest = 0;
    for line in lines {
        let t = line.trim_start_matches([' ', '\t']);
        let run = t.bytes().take_while(|&b| b == ch).count();
        if t.get(run..)
            .is_some_and(|r| r.trim_matches([' ', '\t']).is_empty())
        {
            longest = longest.max(run);
        }
    }
    longest
}

const MAX_DECIMAL: i64 = 999_999_999;
const MAX_ROMAN: i64 = 3999;
const ROMAN_LETTERS: [i64; 7] = [1, 5, 10, 50, 100, 500, 1000];
const ROMAN_PLACES: [(i64, &str); 13] = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

pub struct ListMarkers {
    pub markers: Vec<String>,
    /// C2-B-045: the start value that no first marker can carry.
    pub start: Option<String>,
}

fn roman(value: i64) -> String {
    let mut out = String::new();
    let mut rest = value;
    for (amount, letters) in ROMAN_PLACES {
        while rest >= amount {
            out.push_str(letters);
            rest -= amount;
        }
    }
    out
}

fn letter_marker(style: &str, value: i64) -> Option<String> {
    let lower = style.to_lowercase();
    let text = if lower == "a" {
        u8::try_from(value)
            .ok()
            .filter(|v| (1..=26).contains(v))
            .map(|v| char::from(b'a' + v - 1).to_string())?
    } else if (1..=MAX_ROMAN).contains(&value) {
        roman(value)
    } else {
        return None;
    };
    if text.len() > 9 {
        return None;
    }
    Some(if style == lower {
        text
    } else {
        text.to_uppercase()
    })
}

fn reads_as_other_style(style: &str, value: i64) -> bool {
    if style.eq_ignore_ascii_case("a") {
        value == 9
    } else {
        value != 1 && ROMAN_LETTERS.contains(&value)
    }
}

fn letter_markers(style: &str, delimiter: &str, start: i64, count: usize) -> ListMarkers {
    let first = letter_marker(style, start);
    let moved = first.is_none() || reads_as_other_style(style, start);
    let from = if moved { 1 } else { start };
    let mut markers = Vec::with_capacity(count);
    for at in 0..count {
        let value = from.saturating_add(i64::try_from(at).unwrap_or(i64::MAX));
        let text = letter_marker(style, value)
            .or_else(|| letter_marker(style, 1))
            .unwrap_or_default();
        markers.push(format!("{text}{delimiter}"));
    }
    ListMarkers {
        markers,
        start: moved.then(|| start.to_string()),
    }
}

fn numbered(delimiter: &str, start: i64, count: usize) -> ListMarkers {
    let markers = (0..count)
        .map(|at| {
            let value = start.saturating_add(i64::try_from(at).unwrap_or(i64::MAX));
            let value = if value > MAX_DECIMAL { 1 } else { value };
            format!("{value}{delimiter}")
        })
        .collect();
    ListMarkers {
        markers,
        start: None,
    }
}

/// The markers of the items of the list `node`, and the start value for its attribute line.
pub fn list_markers(w: &Writer<'_>, node: &Value) -> ListMarkers {
    let count = super::node::children(node).len();
    let marker = node.get("marker").cloned().unwrap_or_default();
    let kind = s(&marker, "kind").unwrap_or("bullet");
    if kind != "ordered" {
        let c = s(&marker, "char").unwrap_or("-").to_string();
        return ListMarkers {
            markers: vec![c; count],
            start: None,
        };
    }
    let style = s(&marker, "style").unwrap_or("1");
    let delimiter = s(&marker, "delimiter").unwrap_or(".");
    let start = marker.get("start").and_then(Value::as_i64).unwrap_or(1);
    if style != "1" {
        return letter_markers(style, delimiter, start, count);
    }
    // C2-W-044, C2-B-219: a start of more than nine digits is no marker and no start value.
    let start = if start > MAX_DECIMAL { 1 } else { start };
    // C2-L-031: a bare `)` is no marker. C2-L-030: with no fact the period takes the bare dot.
    let bare =
        delimiter == "." && !w.bare_dot_lead && w.fact(node).and_then(|f| f.bare).unwrap_or(true);
    if !bare {
        return numbered(delimiter, start, count);
    }
    ListMarkers {
        markers: vec![".".to_string(); count],
        start: (start != 1).then(|| start.to_string()),
    }
}

fn marker_field<'a>(list: &'a Value, f: &str, default: &'a str) -> &'a str {
    list.get("marker").and_then(|m| s(m, f)).unwrap_or(default)
}

fn same_ordered_kind(a: &Value, b: &Value) -> bool {
    let (first, second) = (marker_field(a, "style", "1"), marker_field(b, "style", "1"));
    if marker_field(a, "delimiter", ".") != marker_field(b, "delimiter", ".") {
        return false;
    }
    let upper = |s: &str| s != s.to_lowercase();
    // C2-B-038, C2-B-039: a single letter is a sibling in a list of the other letter style.
    first == second || (first != "1" && second != "1" && upper(first) == upper(second))
}

/// C2-B-049, C2-B-053: whether the list `b` directly after the list `a` would continue it.
pub fn same_list_kind(a: &Value, b: &Value) -> bool {
    if ty(a) == "definition_list" {
        return ty(b) == "definition_list";
    }
    if ty(a) != "list" || ty(b) != "list" {
        return false;
    }
    let (ka, kb) = (marker_field(a, "kind", ""), marker_field(b, "kind", ""));
    if ka != kb {
        return false;
    }
    if ka == "ordered" {
        same_ordered_kind(a, b)
    } else {
        marker_field(a, "char", "") == marker_field(b, "char", "")
    }
}
