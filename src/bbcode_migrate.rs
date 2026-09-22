//! BBCode-to-Carve migration.
//!
//! BBCode is a tag language, so tags are protected while ordinary text is
//! escaped with the shared `plain` profile, then rewritten in a deliberate
//! order. Literal/code runs are stashed until every other rewrite is complete.

use crate::djot_migrate::{escape_plain_carve_syntax, HandledDelimiters};
use regex::{Captures, Regex};
use std::fmt;

pub const BBCODE_MAX_INPUT_LENGTH: usize = 262_144;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BbcodeImportError {
    InputTooLarge { bytes: usize, maximum: usize },
    SentinelSpaceExhausted,
}

impl fmt::Display for BbcodeImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { bytes, maximum } => write!(
                f,
                "BBCode input exceeds maximum length of {maximum} bytes (got {bytes})"
            ),
            Self::SentinelSpaceExhausted => f.write_str(
                "BBCode input occupies every private-use sentinel available to the importer",
            ),
        }
    }
}

impl std::error::Error for BbcodeImportError {}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("the BBCode importer owns valid regexes")
}

fn replace(text: String, pattern: &str, replacement: &str) -> String {
    re(pattern).replace_all(&text, replacement).into_owned()
}

fn pick_pair(source: &str, first: u32) -> Result<(char, char), BbcodeImportError> {
    for code in first..=0xf8fd {
        let Some(open) = char::from_u32(code) else {
            continue;
        };
        let Some(close) = char::from_u32(code + 1) else {
            continue;
        };
        if !source.contains(open) && !source.contains(close) {
            return Ok((open, close));
        }
    }
    Err(BbcodeImportError::SentinelSpaceExhausted)
}

fn stash_matches(
    mut text: String,
    patterns: &[&str],
    open: char,
    close: char,
    stash: &mut Vec<String>,
) -> String {
    for pattern in patterns {
        text = re(pattern)
            .replace_all(&text, |caps: &Captures<'_>| {
                let index = stash.len();
                stash.push(caps[0].to_string());
                format!("{open}{index}{close}")
            })
            .into_owned();
    }
    text
}

fn restore(mut text: String, open: char, close: char, stash: &[String]) -> String {
    let pattern = re(&format!(
        r"{}([0-9]+){}",
        regex::escape(&open.to_string()),
        regex::escape(&close.to_string())
    ));
    loop {
        let next = pattern
            .replace_all(&text, |caps: &Captures<'_>| {
                caps[1]
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| stash.get(index))
                    .cloned()
                    .unwrap_or_default()
            })
            .into_owned();
        if next == text {
            return text;
        }
        text = next;
    }
}

fn escape_text(source: &str) -> Result<String, BbcodeImportError> {
    let (open, close) = pick_pair(source, 0xe001)?;
    let mut stash = Vec::new();
    let mut text = stash_matches(
        source.to_string(),
        &[
            r"(?is)\[code(?:=[^\]]*)?\].*?\[/code\]",
            r"(?is)\[(?:c|icode)\].*?\[/(?:c|icode)\]",
            r"(?is)\[url\].*?\[/url\]",
            r"(?is)\[img(?:=[^\]]*)?\].*?\[/img\]",
            r"(?i)\[/?[a-z][a-z0-9]*(?:=[^\]]*)?\]",
            r"\[\*\]",
        ],
        open,
        close,
        &mut stash,
    );
    text = text.replace('\\', "\\\\").replace('`', "\\`");
    text = text.replace("{#", "\\{#").replace("{.", "\\{.");
    text = escape_plain_carve_syntax(&text, HandledDelimiters::PLAIN);
    Ok(restore(text, open, close, &stash))
}

fn stash_literals(source: String) -> Result<(String, char, char, Vec<String>), BbcodeImportError> {
    let (open, close) = pick_pair(&source, 0xe010)?;
    let mut stash = Vec::new();
    let mut text = source;
    for (kind, pattern) in [
        r"(?is)(\[code(?:=[^\]]*)?\])(.*?)(\[/code\])",
        r"(?is)(\[(?:c|icode)\])(.*?)(\[/(?:c|icode)\])",
    ]
    .into_iter()
    .enumerate()
    {
        text = re(pattern)
            .replace_all(&text, |caps: &Captures<'_>| {
                let index = stash.len();
                stash.push(if kind == 0 {
                    caps[2].trim().to_string()
                } else {
                    caps[2].to_string()
                });
                format!("{}{open}{index}{close}{}", &caps[1], &caps[3])
            })
            .into_owned();
    }
    text = re(r"(?is)\[noparse\](.*?)\[/noparse\]")
        .replace_all(&text, |caps: &Captures<'_>| {
            let index = stash.len();
            stash.push(caps[1].to_string());
            format!("{open}{index}{close}")
        })
        .into_owned();
    Ok((text, open, close, stash))
}

fn convert_pairs(mut text: String) -> String {
    for (pattern, replacement) in [
        (r"(?is)\[url=([^\]]+)\](.*?)\[/url\]", "[$2]($1)"),
        (r"(?is)\[url\](.*?)\[/url\]", "<$1>"),
        (r"(?is)\[email\](.*?)\[/email\]", "<mailto:$1>"),
        (r"(?is)\[img(?:=[^\]]*)?\](.*?)\[/img\]", "![]($1)"),
    ] {
        text = replace(text, pattern, replacement);
    }
    text = convert_marks(&text);
    for (pattern, replacement) in [
        (
            r"(?is)\[(?:size|color|font)=[^\]]*\](.*?)\[/(?:size|color|font)\]",
            "$1",
        ),
        (
            r"(?is)\[(?:center|left|right)\](.*?)\[/(?:center|left|right)\]",
            "$1",
        ),
        (r"(?is)\[(?:c|icode)\](.*?)\[/(?:c|icode)\]", "`$1`"),
        (
            r"(?i)\[youtube\]([a-z0-9_-]+)\[/youtube\]",
            "![YouTube Video](https://www.youtube.com/watch?v=$1)",
        ),
    ] {
        text = replace(text, pattern, replacement);
    }
    // Forced brace form: a sup/sub is often intraword (E=mc[sup]2[/sup]). An
    // empty one has no spelling and goes (ruling markup-carve/carve-rs#1719).
    for (pattern, open, close) in [
        (r"(?is)\[sup\](.*?)\[/sup\]", "{^", "^}"),
        (r"(?is)\[sub\](.*?)\[/sub\]", "{,", ",}"),
    ] {
        text = re(pattern)
            .replace_all(&text, |caps: &Captures<'_>| {
                if caps[1].is_empty() {
                    String::new()
                } else {
                    format!("{open}{}{close}", &caps[1])
                }
            })
            .into_owned();
    }
    text
}

/// One piece of a formatting-tag tree: text, or a tag by its arena index.
enum MarkItem {
    Text(String),
    Mark(usize),
}

struct ParsedMark {
    kind: u8,
    open: String,
    children: Vec<MarkItem>,
    unclosed: bool,
}

fn mark_delim(kind: u8) -> char {
    match kind {
        b'b' => '*',
        b'i' => '/',
        b'u' => '_',
        _ => '~',
    }
}

/// The formatting tags as a tree, kept in an arena so neither building nor
/// dropping it recurses on the author's nesting depth. A close tag that does
/// not match the innermost open one stays literal text, and so does an open
/// tag never closed (marked here, unwound by `flatten_same_kind`).
fn parse_marks(text: &str) -> (Vec<ParsedMark>, Vec<MarkItem>) {
    let mut arena: Vec<ParsedMark> = Vec::new();
    let mut root: Vec<MarkItem> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut from = 0;
    for caps in re(r"(?i)\[(/?)(b|i|u|s)\]").captures_iter(text) {
        let whole = caps.get(0).expect("group 0 always matches");
        let kind = caps[2].to_ascii_lowercase().as_bytes()[0];
        let lead = MarkItem::Text(text[from..whole.start()].to_string());
        match stack.last() {
            Some(&top) => arena[top].children.push(lead),
            None => root.push(lead),
        }
        from = whole.end();
        if caps[1].is_empty() {
            arena.push(ParsedMark {
                kind,
                open: whole.as_str().to_string(),
                children: Vec::new(),
                unclosed: false,
            });
            let id = arena.len() - 1;
            match stack.last() {
                Some(&top) => arena[top].children.push(MarkItem::Mark(id)),
                None => root.push(MarkItem::Mark(id)),
            }
            stack.push(id);
        } else if stack.last().is_some_and(|&top| arena[top].kind == kind) {
            stack.pop();
        } else {
            let literal = MarkItem::Text(whole.as_str().to_string());
            match stack.last() {
                Some(&top) => arena[top].children.push(literal),
                None => root.push(literal),
            }
        }
    }
    let tail = MarkItem::Text(text[from..].to_string());
    match stack.last() {
        Some(&top) => arena[top].children.push(tail),
        None => root.push(tail),
    }
    for id in stack {
        arena[id].unclosed = true;
    }
    (arena, root)
}

/// A tag after flattening: its kind and the arena slot holding its children.
struct FlatMark {
    kind: u8,
    slot: usize,
}

enum FlatItem {
    Text(String),
    Mark(FlatMark),
}

/// An unclosed tag becomes its literal text followed by its content. Carve has
/// no second level of one kind (E3), so a tag inside an open tag of its own
/// kind adds nothing and is replaced by its content. Iterative, and a
/// transparent tag writes straight into its parent's slot, so this is linear;
/// with four kinds the tree left behind is at most four deep, which keeps
/// `write_marks` recursion bounded.
fn flatten_same_kind(mut arena: Vec<ParsedMark>, root: Vec<MarkItem>) -> Vec<Vec<FlatItem>> {
    let mut slots: Vec<Vec<FlatItem>> = vec![Vec::new()];
    // (items still to place, slot they go to, kinds open around them)
    let mut frames: Vec<(std::vec::IntoIter<MarkItem>, usize, [bool; 4])> =
        vec![(root.into_iter(), 0, [false; 4])];
    let kind_index = |kind: u8| match kind {
        b'b' => 0,
        b'i' => 1,
        b'u' => 2,
        _ => 3,
    };
    while let Some((items, slot, open)) = frames.last_mut() {
        let Some(item) = items.next() else {
            frames.pop();
            continue;
        };
        let (slot, open) = (*slot, *open);
        match item {
            MarkItem::Text(text) => slots[slot].push(FlatItem::Text(text)),
            MarkItem::Mark(id) => {
                let children = std::mem::take(&mut arena[id].children);
                let kind = arena[id].kind;
                if arena[id].unclosed {
                    slots[slot].push(FlatItem::Text(std::mem::take(&mut arena[id].open)));
                    frames.push((children.into_iter(), slot, open));
                } else if open[kind_index(kind)] {
                    frames.push((children.into_iter(), slot, open));
                } else {
                    slots.push(Vec::new());
                    let child_slot = slots.len() - 1;
                    slots[slot].push(FlatItem::Mark(FlatMark {
                        kind,
                        slot: child_slot,
                    }));
                    let mut inner = open;
                    inner[kind_index(kind)] = true;
                    frames.push((children.into_iter(), child_slot, inner));
                }
            }
        }
    }
    slots
}

// The tree is at most four deep, so this recursion is bounded.
fn slot_has_content(slots: &[Vec<FlatItem>], slot: usize) -> bool {
    slots[slot].iter().any(|item| match item {
        FlatItem::Text(text) => !text.is_empty(),
        FlatItem::Mark(mark) => slot_has_content(slots, mark.slot),
    })
}

/// For each item, the first character written after it, or `None` when nothing
/// is. One pass from the right, so a run of empty tags costs nothing per tag.
fn next_chars(slots: &[Vec<FlatItem>], slot: usize) -> Vec<Option<char>> {
    let items = &slots[slot];
    let mut next = vec![None; items.len()];
    let mut after = None;
    for (k, item) in items.iter().enumerate().rev() {
        next[k] = after;
        match item {
            FlatItem::Text(text) => {
                if let Some(first) = text.chars().next() {
                    after = Some(first);
                }
            }
            FlatItem::Mark(mark) => {
                if slot_has_content(slots, mark.slot) {
                    after = Some('{');
                }
            }
        }
    }
    next
}

fn is_word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Write the tree the way the Carve writer would. An empty tag holds nothing a
/// reader sees and has no spelling, so it goes (ruling markup-carve/carve-rs#1719).
/// A bare pair the CARVE-P3-013 guards would not read back, or that the writer
/// would brace, takes the braced form, as does a `/` around content that would
/// read back as bold-italic.
fn write_marks(
    slots: &[Vec<FlatItem>],
    slot: usize,
    outer_next: Option<char>,
    prev: Option<char>,
) -> String {
    // Chunks rather than one growing string, so escaping the character before
    // an opener rewrites only the text chunk holding it.
    let mut parts: Vec<String> = Vec::new();
    let mut last = prev;
    let mut text_part: Option<usize> = None;
    let mut escape_brace = false;
    let nexts = next_chars(slots, slot);
    for (index, item) in slots[slot].iter().enumerate() {
        match item {
            FlatItem::Text(text) => {
                if text.is_empty() {
                    continue;
                }
                // A `{` before a bare opener and a `}` after its closer would read
                // as the braced form, eating both braces; the writer escapes the `}`.
                let chunk = if escape_brace && text.starts_with('}') {
                    format!("\\{text}")
                } else {
                    text.clone()
                };
                last = chunk.chars().next_back();
                parts.push(chunk);
                escape_brace = false;
                text_part = Some(parts.len() - 1);
            }
            FlatItem::Mark(mark) => {
                let delim = mark_delim(mark.kind);
                // The parent's own delimiters are not text: a child at its edge
                // has no neighbor there, which is how the writer spells `/_x_/`.
                let body = write_marks(slots, mark.slot, None, None);
                if body.is_empty() {
                    continue;
                }
                // The post's own text was escaped while the tags were still tags,
                // so a literal delimiter now touching an opener was never seen.
                // Already escaped only behind an ODD run of backslashes.
                if text_part.is_some()
                    && text_part == parts.len().checked_sub(1)
                    && last.is_some_and(|c| "*/_~=".contains(c))
                {
                    let chunk = &parts[parts.len() - 1];
                    let run = chunk[..chunk.len() - 1]
                        .bytes()
                        .rev()
                        .take_while(|b| *b == b'\\')
                        .count();
                    if run % 2 == 0 {
                        let chunk = parts.last_mut().expect("text_part is the last chunk");
                        chunk.insert(chunk.len() - 1, '\\');
                    }
                }
                let before = last;
                let next = nexts[index].or(outer_next);
                let ws = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
                let braced = body.starts_with(ws)
                    || body.ends_with(ws)
                    || is_word(before)
                    || before == Some(delim)
                    || (before == Some('/') && (delim == '/' || delim == '_'))
                    || is_word(next)
                    || body.starts_with(delim)
                    || body.ends_with(delim)
                    || (delim == '/' && body.starts_with('*') && body.ends_with('*'));
                let chunk = if braced {
                    format!("{{{delim}{body}{delim}}}")
                } else {
                    format!("{delim}{body}{delim}")
                };
                last = chunk.chars().next_back();
                parts.push(chunk);
                escape_brace = !braced && before == Some('{');
            }
        }
    }
    parts.concat()
}

fn convert_marks(text: &str) -> String {
    let (arena, root) = parse_marks(text);
    let slots = flatten_same_kind(arena, root);
    write_marks(&slots, 0, None, None)
}

fn convert_code(mut text: String) -> String {
    text = re(r"(?is)\[code=([^\]]+)\](.*?)\[/code\]")
        .replace_all(&text, |caps: &Captures<'_>| {
            let language = caps[1].trim().to_ascii_lowercase();
            let language = language.trim_start_matches('=').trim_start();
            format!("\n\n```{language}\n{}\n```\n\n", caps[2].trim())
        })
        .into_owned();
    text = re(r"(?is)\[code\](.*?)\[/code\]")
        .replace_all(&text, |caps: &Captures<'_>| {
            format!("\n\n```\n{}\n```\n\n", caps[1].trim())
        })
        .into_owned();
    text
}

fn convert_other(mut text: String) -> String {
    text = re(r"(?is)\[spoiler(?:=([^\]]+))?\](.*?)\[/spoiler\]")
        .replace_all(&text, |caps: &Captures<'_>| {
            let title = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let title = title.replace('\\', "\\\\").replace('"', "\\\"");
            let attr = if title.is_empty() {
                String::new()
            } else {
                format!("{{title=\"{title}\"}}\n")
            };
            format!("{attr}::: spoiler\n{}\n:::\n", caps[2].trim())
        })
        .into_owned();

    let table = re(r"(?is)\[table\](.*?)\[/table\]");
    text = table
        .replace_all(&text, |caps: &Captures<'_>| {
            let rows = re(r"(?is)\[tr\](.*?)\[/tr\]");
            let mut output = Vec::new();
            for row in rows.captures_iter(&caps[1]) {
                let body = &row[1];
                let headers = re(r"(?is)\[th\](.*?)\[/th\]")
                    .captures_iter(body)
                    .map(|cell| format!("|= {}", cell[1].trim()))
                    .collect::<Vec<_>>();
                if !headers.is_empty() {
                    output.push(format!("{} |", headers.join(" ")));
                    continue;
                }
                let cells = re(r"(?is)\[td\](.*?)\[/td\]")
                    .captures_iter(body)
                    .map(|cell| cell[1].trim().to_string())
                    .collect::<Vec<_>>();
                if !cells.is_empty() {
                    output.push(format!("| {} |", cells.join(" | ")));
                }
            }
            format!("\n\n{}\n\n", output.join("\n"))
        })
        .into_owned();
    text
}

fn quote_block(content: &str, author: Option<&str>) -> String {
    let quoted = content
        .trim()
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let attribution = format_attribution(author.unwrap_or(""));
    if attribution.is_empty() {
        format!("\n\n{quoted}\n\n")
    } else {
        format!("\n\n{quoted}\n^ {attribution}\n\n")
    }
}

fn take_named(remaining: &mut String, name: &str) -> Option<String> {
    let pattern = re(&format!(r#"(?i)\b{name}=["']([^"']+)["']"#));
    let caps = pattern.captures(remaining)?;
    let value = caps.get(1)?.as_str().to_string();
    *remaining = pattern.replace(remaining, "").into_owned();
    Some(value)
}

fn format_attribution(source: &str) -> String {
    let mut remaining = source.trim().to_string();
    let leading_id = re(r#"^["']([0-9]+)["']"#);
    let id = if let Some(caps) = leading_id.captures(&remaining) {
        let value = caps[1].to_string();
        remaining = remaining[caps.get(0).expect("leading id").end()..]
            .trim()
            .to_string();
        Some(value)
    } else {
        let named = re(r#"(?i)\bid=["']?([0-9]+)["']?"#);
        let value = named.captures(&remaining).map(|caps| caps[1].to_string());
        remaining = named.replace(&remaining, "").into_owned();
        value
    };
    let name = take_named(&mut remaining, "name");
    let date = take_named(&mut remaining, "date");
    let time = take_named(&mut remaining, "time");
    let fallback = remaining.trim();
    let mut output = name.unwrap_or_else(|| fallback.to_string());
    let datetime = match (date, time) {
        (Some(date), Some(time)) => Some(format!("{date} {time}")),
        (Some(date), None) => Some(date),
        (None, Some(time)) => Some(time),
        (None, None) => None,
    };
    if let Some(datetime) = datetime {
        output.push_str(&format!(" ({datetime})"));
    }
    if let Some(id) = id {
        output.push_str(&format!(" #{id}"));
    }
    output.trim().to_string()
}

fn convert_quotes(text: String) -> String {
    let open = re(r"(?i)^\[quote(?:[= ]([^\]]*))?\]");
    let close = re(r"(?i)^\[/quote\]");
    let mut contents = vec![String::new()];
    let mut authors: Vec<Option<String>> = vec![None];
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if let Some(caps) = open.captures(rest) {
            let whole = caps.get(0).expect("whole quote opener");
            contents.push(String::new());
            authors.push(caps.get(1).map(|m| m.as_str().to_string()));
            i += whole.end();
            continue;
        }
        if let Some(found) = close.find(rest) {
            i += found.end();
            if contents.len() > 1 {
                let content = contents.pop().expect("nested quote content");
                let author = authors.pop().expect("nested quote author");
                let block = quote_block(&content, author.as_deref());
                contents
                    .last_mut()
                    .expect("root quote buffer")
                    .push_str(&block);
            }
            continue;
        }
        let ch = rest.chars().next().expect("non-empty remainder");
        contents.last_mut().expect("quote buffer").push(ch);
        i += ch.len_utf8();
    }
    while contents.len() > 1 {
        let content = contents.pop().expect("nested quote content");
        let author = authors.pop().expect("nested quote author");
        let block = quote_block(&content, author.as_deref());
        contents
            .last_mut()
            .expect("root quote buffer")
            .push_str(&block);
    }
    contents.pop().expect("root quote buffer")
}

#[derive(Debug)]
struct ListFrame {
    ordered: bool,
    marker: char,
    delimiter: char,
    lead: String,
    items: Vec<String>,
    current: Option<String>,
    sibling_index: usize,
}

fn trim_item(body: &str) -> String {
    body.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn render_list(frame: &ListFrame) -> String {
    frame
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if frame.ordered {
                format!("{}{} ", index + 1, frame.delimiter)
            } else {
                format!("{} ", frame.marker)
            };
            let indent = " ".repeat(marker.len());
            trim_item(item)
                .lines()
                .enumerate()
                .map(|(line_index, line)| {
                    if line_index == 0 {
                        format!("{marker}{line}")
                    } else if line.is_empty() {
                        String::new()
                    } else {
                        format!("{indent}{line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn convert_lists(text: String) -> String {
    let open = re(r"(?i)^\[list(?:=([^\]]*))?\]");
    let item = re(r"(?i)^\[\*\]");
    let close = re(r"(?i)^\[/list\]");
    let mut frames: Vec<ListFrame> = Vec::new();
    let mut output = String::new();
    let mut root_sibling = 0usize;
    let mut i = 0usize;

    let append = |frames: &mut Vec<ListFrame>, output: &mut String, value: &str| {
        if let Some(frame) = frames.last_mut() {
            if let Some(current) = frame.current.as_mut() {
                current.push_str(value);
            } else {
                frame.lead.push_str(value);
            }
        } else {
            output.push_str(value);
        }
    };

    while i < text.len() {
        let rest = &text[i..];
        if let Some(caps) = open.captures(rest) {
            let value = caps.get(1).map(|m| m.as_str().trim());
            if value.is_none() || value == Some("1") {
                let axis = if let Some(parent) = frames.last_mut() {
                    let axis = parent.sibling_index;
                    parent.sibling_index += 1;
                    axis
                } else {
                    let axis = root_sibling;
                    root_sibling += 1;
                    axis
                };
                frames.push(ListFrame {
                    ordered: value.is_some(),
                    marker: if axis % 2 == 0 { '-' } else { '*' },
                    delimiter: if axis % 2 == 0 { '.' } else { ')' },
                    lead: String::new(),
                    items: Vec::new(),
                    current: None,
                    sibling_index: 0,
                });
                i += caps.get(0).expect("list opener").end();
                continue;
            }
        }
        if !frames.is_empty() && item.is_match(rest) {
            let frame = frames.last_mut().expect("open list frame");
            if let Some(current) = frame.current.take() {
                frame.items.push(current);
            }
            frame.current = Some(String::new());
            frame.sibling_index = 0;
            i += 3;
            continue;
        }
        if !frames.is_empty() && close.is_match(rest) {
            i += "[/list]".len();
            close_list_frame(&mut frames, &mut output, &append);
            continue;
        }
        let ch = rest.chars().next().expect("non-empty remainder");
        let mut encoded = [0; 4];
        append(&mut frames, &mut output, ch.encode_utf8(&mut encoded));
        i += ch.len_utf8();
    }
    while !frames.is_empty() {
        close_list_frame(&mut frames, &mut output, &append);
    }
    output
}

fn close_list_frame<F>(frames: &mut Vec<ListFrame>, output: &mut String, append: &F)
where
    F: Fn(&mut Vec<ListFrame>, &mut String, &str),
{
    let mut frame = frames.pop().expect("open list frame");
    if let Some(current) = frame.current.take() {
        frame.items.push(current);
    }
    let lead = trim_item(&frame.lead);
    let list = render_list(&frame);
    let block = if lead.is_empty() {
        list
    } else {
        format!("{lead}\n{list}")
    };
    if frames.is_empty() {
        output.push_str(&format!("\n\n{}\n\n", block.trim_end()));
    } else {
        let needs_newline = frames
            .last()
            .and_then(|parent| parent.current.as_ref())
            .is_some_and(|current| !current.is_empty() && !current.ends_with('\n'));
        if needs_newline {
            append(frames, output, "\n");
        }
        append(frames, output, block.trim_end());
        append(frames, output, "\n");
    }
}

fn cleanup(mut text: String) -> String {
    text = replace(text, r"(?i)\[hr\]", "\n---\n");
    text = replace(text, r"(?i)\[/[a-z][a-z0-9]*\]", "");
    text = replace(text, r"(?i)\[[a-z][a-z0-9]*=[^\]]*\]", "");
    text = replace(text, r"\n{3,}", "\n\n");
    format!("{}\n", text.trim())
}

/// Convert one bounded BBCode post to canonical Carve source.
pub fn bbcode_to_carve(source: &str) -> Result<String, BbcodeImportError> {
    if source.len() > BBCODE_MAX_INPUT_LENGTH {
        return Err(BbcodeImportError::InputTooLarge {
            bytes: source.len(),
            maximum: BBCODE_MAX_INPUT_LENGTH,
        });
    }
    let normalized = source
        .replace('\0', "\u{fffd}")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let escaped = escape_text(&normalized)?;
    let (mut text, open, close, stash) = stash_literals(escaped)?;
    text = convert_pairs(text);
    text = convert_code(text);
    text = convert_quotes(text);
    text = convert_lists(text);
    text = convert_other(text);
    text = cleanup(text);
    Ok(restore(text, open, close, &stash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_tags_convert() {
        assert_eq!(bbcode_to_carve("[b]b[/b] [i]i[/i]").unwrap(), "*b* /i/\n");
        assert_eq!(
            bbcode_to_carve("[url=https://e.test]x[/url]").unwrap(),
            "[x](https://e.test)\n"
        );
        assert_eq!(
            bbcode_to_carve("[code][b]x[/b][/code]").unwrap(),
            "```\n[b]x[/b]\n```\n"
        );
    }

    #[test]
    fn plain_carve_syntax_stays_literal() {
        assert_eq!(
            bbcode_to_carve("literal *stars* and @name").unwrap(),
            "literal \\*stars* and \\@name\n"
        );
    }
}
