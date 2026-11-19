//! What the Markdown, plain-text and ANSI exports share: the resolved copy
//! of the tree, the loss record and the text of the nodes whose output does
//! not depend on the target.

use super::{ExportOptions, Target};
use crate::api::{Entry, Error, Range};
use crate::options::UrlPolicy;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The bound of a recursive walk, above the parser's own nesting limit, for supplied trees.
pub const MAX_EXPORT_DEPTH: usize = 1000;

pub fn ty(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or("")
}

pub fn sv<'v>(node: &'v Value, key: &str) -> Option<&'v str> {
    node.get(key).and_then(Value::as_str)
}

pub fn list<'v>(node: &'v Value, key: &str) -> &'v [Value] {
    node.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub fn children(node: &Value) -> &[Value] {
    list(node, "children")
}

pub fn flag(node: &Value, key: &str) -> bool {
    node.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn int(node: &Value, key: &str) -> Option<i64> {
    node.get(key).and_then(Value::as_i64)
}

/// A node slot that may be absent.
pub fn slot<'v>(node: &'v Value, key: &str) -> Option<&'v Value> {
    node.get(key).filter(|v| v.is_object())
}

struct Loss {
    code: &'static str,
    severity: &'static str,
    message: String,
    node: *const Value,
    range: Option<Range>,
}

/// The state of one export run.
pub struct Exporter<'v> {
    pub target: Target,
    pub document: &'v Value,
    /// The text of each cross-reference target, by id (C2-H-096).
    pub xref_text: HashMap<String, String>,
    /// The tooltip of each top-level link reference definition, first of each key.
    pub definitions: HashMap<String, Option<String>>,
    pub smart_typography: bool,
    pub url_policy: UrlPolicy,
    losses: Vec<Loss>,
    /// Bytes of derived text still allowed in this run.
    pub derived_left: usize,
    pub derived_reported: bool,
    pub depth: usize,
    pub link_depth: usize,
}

impl<'v> Exporter<'v> {
    pub fn new(
        target: Target,
        document: &'v Value,
        xref_text: HashMap<String, String>,
        options: &ExportOptions,
    ) -> Self {
        let mut definitions = HashMap::new();
        for child in children(document) {
            if ty(child) == "link_reference_definition"
                && let Some(key) = sv(child, "key")
            {
                definitions
                    .entry(key.to_string())
                    .or_insert_with(|| sv(child, "tooltip").map(str::to_string));
            }
        }
        Self {
            target,
            document,
            xref_text,
            definitions,
            smart_typography: options.smart_typography,
            url_policy: options.url_policy.clone(),
            losses: Vec::new(),
            derived_left: crate::tables::DERIVED_TEXT_BYTES,
            derived_reported: false,
            depth: 0,
            link_depth: 0,
        }
    }

    pub fn loss(&mut self, node: &Value, code: &'static str, message: impl Into<String>) {
        self.push(node, code, "warning", message.into());
    }

    fn push(&mut self, node: &Value, code: &'static str, severity: &'static str, message: String) {
        self.losses.push(Loss {
            code,
            severity,
            message,
            node: std::ptr::from_ref(node),
            range: node.get("pos").and_then(|pos| {
                Some(Range {
                    start_offset: usize::try_from(pos.get("startOffset")?.as_u64()?).ok()?,
                    end_offset: usize::try_from(pos.get("endOffset")?.as_u64()?).ok()?,
                })
            }),
        });
    }

    pub fn loss_count(&self) -> usize {
        self.losses.len()
    }

    pub fn truncate_losses(&mut self, len: usize) {
        self.losses.truncate(len);
    }

    /// Descends one level of the tree, refusing a supplied tree nested past [`MAX_EXPORT_DEPTH`].
    pub fn enter(&mut self) -> Result<(), Error> {
        if self.depth >= MAX_EXPORT_DEPTH {
            return Err(Error::option(
                "document",
                format!("nested deeper than {MAX_EXPORT_DEPTH} levels"),
            ));
        }
        self.depth += 1;
        Ok(())
    }

    pub fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Takes `text` from the budget of derived text (C2-H-201). Past the
    /// budget the caller writes the plain form, and the first such
    /// occurrence is reported.
    pub fn draw_derived(&mut self, node: &Value, text: &str) -> bool {
        let cost = text.len();
        if cost <= self.derived_left {
            self.derived_left -= cost;
            return true;
        }
        self.derived_left = 0;
        if !self.derived_reported {
            self.derived_reported = true;
            self.loss(
                node,
                "derived-text-budget",
                format!(
                    "Wrote an abbreviation or a cross-reference in its plain form: the derived text of this render reached its budget of {} bytes",
                    crate::tables::DERIVED_TEXT_BYTES
                ),
            );
        }
        false
    }

    /// The destination of a link or an image, empty where the URL policy
    /// denies its scheme (reported on `node`).
    pub fn allowed_destination(&mut self, url: &str, node: &Value) -> String {
        if crate::html::url::allowed(url, &self.url_policy) {
            return url.to_string();
        }
        let message = if ty(node) == "image" {
            "Blanked a denied image source"
        } else {
            "Blanked a denied destination scheme"
        };
        self.push(node, "destination-denied", "error", message.into());
        String::new()
    }

    pub fn raw_dropped(&mut self, node: &Value, block: bool) {
        let message = format!(
            "Dropped {} raw format \"{}\" while rendering {}",
            if block { "block" } else { "inline" },
            sv(node, "format").unwrap_or(""),
            self.target.name()
        );
        self.loss(node, "raw-format-dropped", message);
    }

    pub fn smart_text<'n>(&self, node: &'n Value) -> &'n str {
        if self.smart_typography {
            sv(node, "glyph")
                .or_else(|| sv(node, "source"))
                .unwrap_or("")
        } else {
            sv(node, "source").unwrap_or("")
        }
    }

    /// The text a cross-reference stands for, or its source where it found no target.
    pub fn xref_text(&mut self, node: &Value) -> (String, bool) {
        let key = sv(node, "key").unwrap_or("");
        let text = sv(node, "href")
            .and_then(|href| self.xref_text.get(href.strip_prefix('#').unwrap_or(href)))
            .cloned();
        let Some(text) = text else {
            return (format!("</#{key}>"), false);
        };
        if self.draw_derived(node, &text) {
            (text, true)
        } else {
            (key.to_string(), true)
        }
    }

    /// C2-H-133: an include that was not expanded is written as its source line, and reported.
    pub fn unexpanded_include(&mut self, node: &Value) -> String {
        let path = sv(node, "path").unwrap_or("");
        self.loss(
            node,
            "include-unresolved",
            format!("Include \"{path}\" could not be resolved."),
        );
        crate::write::include_spelling(node)
    }

    /// The losses of the run with their positions (C2-A-173): the pointer of
    /// each node, its byte range and the file an include around it names.
    pub fn diagnostics(&self) -> Vec<Entry> {
        if self.losses.is_empty() {
            return Vec::new();
        }
        let wanted: HashSet<*const Value> = self.losses.iter().map(|l| l.node).collect();
        let mut found: HashMap<*const Value, (String, Option<String>)> = HashMap::new();
        locate(self.document, &mut String::new(), None, &wanted, &mut found);
        self.losses
            .iter()
            .map(|l| {
                let place = found.get(&l.node);
                let range = place.and(l.range);
                Entry {
                    code: l.code.to_string(),
                    severity: l.severity.to_string(),
                    channel: "render-loss".into(),
                    message: l.message.clone(),
                    path: Some(place.map(|p| p.0.clone()).unwrap_or_default()),
                    range,
                    file: place.and_then(|p| p.1.clone()),
                    foreign_type: None,
                }
            })
            .collect()
    }
}

fn locate(
    node: &Value,
    pointer: &mut String,
    file: Option<&str>,
    wanted: &HashSet<*const Value>,
    found: &mut HashMap<*const Value, (String, Option<String>)>,
) {
    if found.len() == wanted.len() {
        return;
    }
    let address = std::ptr::from_ref(node);
    if wanted.contains(&address) {
        found
            .entry(address)
            .or_insert_with(|| (pointer.clone(), file.map(str::to_string)));
    }
    let Some(map) = node.as_object() else {
        if let Some(items) = node.as_array() {
            for (at, item) in items.iter().enumerate() {
                let len = pointer.len();
                pointer.push('/');
                pointer.push_str(&at.to_string());
                locate(item, pointer, file, wanted, found);
                pointer.truncate(len);
            }
        }
        return;
    };
    let inner = if ty(node) == "include" && node.get("children").is_some() {
        sv(node, "file").or(file)
    } else {
        file
    };
    for (key, value) in map {
        if key == "pos" || key == "attrs" || !(value.is_object() || value.is_array()) {
            continue;
        }
        let len = pointer.len();
        pointer.push('/');
        pointer.push_str(key);
        locate(
            value,
            pointer,
            if key == "children" { inner } else { file },
            wanted,
            found,
        );
        pointer.truncate(len);
    }
}

/// C2-H-086: the label part of an unresolved reference, as typed.
pub fn reference_label(node: &Value, key: &str) -> String {
    match sv(node, "refForm") {
        Some("collapsed") => "[]".into(),
        Some("shortcut") => String::new(),
        _ => format!("[{key}]"),
    }
}

/// Whether a link or an image is a reference that found no definition.
pub fn is_unresolved(node: &Value) -> bool {
    node.get("key").is_some()
        && if ty(node) == "link" {
            node.get("href").is_none()
        } else {
            node.get("src").is_none()
        }
}

fn is_blank(value: &str) -> bool {
    value.bytes().all(|b| b == b' ' || b == b'\t')
}

/// Whether the line that starts beside `from` and runs in the direction
/// `step` holds only comments and spaces.
fn comment_only_line(
    nodes: &[&Value],
    from: isize,
    step: isize,
    at_end: bool,
    nested: bool,
) -> bool {
    let mut comment = false;
    let mut at = from;
    while let Some(node) = usize::try_from(at).ok().and_then(|i| nodes.get(i)) {
        match ty(node) {
            "comment" => comment = true,
            "soft_break" | "hard_break" => return comment && !at_end,
            "text" if is_blank(sv(node, "value").unwrap_or("")) => {}
            _ => return false,
        }
        at += step;
    }
    comment && !nested
}

/// C2-H-069: a comment-only line takes one line break with it, the one
/// behind it, or the one before it where the content ends with the comment.
#[allow(clippy::cast_possible_wrap)]
pub fn is_skipped_break(nodes: &[&Value], at: usize, nested: bool) -> bool {
    let at = at as isize;
    comment_only_line(nodes, at - 1, -1, false, nested)
        || comment_only_line(nodes, at + 1, 1, true, nested)
}

/// The footnote definitions of the document in source order, the first of each key only.
pub fn footnote_definitions(document: &Value) -> Vec<&Value> {
    fn visit<'v>(list: &'v [Value], keys: &mut HashSet<&'v str>, found: &mut Vec<&'v Value>) {
        for child in list {
            if ty(child) == "include" {
                visit(children(child), keys, found);
            }
            if ty(child) != "footnote" {
                continue;
            }
            let key = sv(child, "key").unwrap_or("");
            if keys.insert(key) {
                found.push(child);
            }
        }
    }
    let mut found = Vec::new();
    visit(children(document), &mut HashSet::new(), &mut found);
    found
}

/// Bidirectional controls the exports never write (U+202A to U+202E, U+2066 to U+2069).
pub fn strip_bidi(text: &str) -> String {
    if !text.contains([
        '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}',
        '\u{2068}', '\u{2069}',
    ]) {
        return text.to_string();
    }
    text.chars()
        .filter(|c| !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .collect()
}

/// Every node of the tree below `root`, its slots included.
pub fn each_node<'v>(root: &'v Value, visit: &mut impl FnMut(&'v Value)) {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.get("type").is_some() {
            visit(node);
        }
        if let Some(map) = node.as_object() {
            for (key, value) in map {
                if key == "pos" || key == "attrs" {
                    continue;
                }
                match value {
                    Value::Array(items) => pending.extend(items.iter().filter(|i| i.is_object())),
                    Value::Object(_) => pending.push(value),
                    _ => {}
                }
            }
        }
    }
}

/// The text of inline content with no styling, for a measure or a slug.
pub fn plain_inline_text(x: &Exporter<'_>, nodes: &[Value]) -> String {
    let mut out = String::new();
    plain_into(x, nodes, &mut out);
    out
}

fn plain_into(x: &Exporter<'_>, nodes: &[Value], out: &mut String) {
    for node in nodes {
        if let Some(text) = plain_atom(x, node).or_else(|| plain_reference(x, node)) {
            out.push_str(&text);
            continue;
        }
        match ty(node) {
            "comment" | "raw_inline" => {}
            "ruby" => {
                for pair in list(node, "pairs") {
                    plain_into(x, list(pair, "base"), out);
                }
            }
            _ => plain_into(x, children(node), out),
        }
    }
}

/// The text of an inline that holds it in a field of its own.
fn plain_atom(x: &Exporter<'_>, node: &Value) -> Option<String> {
    let text = match ty(node) {
        "text" | "code" | "literal_inline" | "math" | "critic_comment" => sv(node, "value"),
        "smart_punctuation" => Some(x.smart_text(node)),
        "soft_break" | "hard_break" | "non_breaking_space" => Some(" "),
        "autolink" => sv(node, "text").or_else(|| sv(node, "href")),
        "image" => sv(node, "alt"),
        "abbreviation" => sv(node, "key"),
        "citation_group" => sv(node, "raw"),
        _ => return None,
    };
    Some(text.unwrap_or("").to_string())
}

/// The text of a symbol, mention, tag, cross-reference or caption number.
fn plain_reference(x: &Exporter<'_>, node: &Value) -> Option<String> {
    Some(match ty(node) {
        "symbol" => format!(":{}:", sv(node, "name").unwrap_or("")),
        "mention" => sv(node, "label").map_or_else(
            || format!("@{}", sv(node, "target").unwrap_or("")),
            str::to_string,
        ),
        "tag" => format!("#{}", sv(node, "name").unwrap_or("")),
        "xref" => sv(node, "href")
            .and_then(|href| x.xref_text.get(href.strip_prefix('#').unwrap_or(href)))
            .cloned()
            .unwrap_or_else(|| format!("</#{}>", sv(node, "key").unwrap_or(""))),
        "caption_number" => int(node, "n").map_or_else(|| "#".into(), |n| n.to_string()),
        _ => return None,
    })
}

/// The value of the `abbr` key of a span's attributes.
pub fn key_value<'v>(node: &'v Value, name: &str) -> Option<&'v str> {
    node.get("attrs")
        .and_then(|a| a.get("keyValues"))
        .and_then(Value::as_array)?
        .iter()
        .find(|pair| pair.get(0).and_then(Value::as_str) == Some(name))
        .and_then(|pair| pair.get(1).and_then(Value::as_str))
}
