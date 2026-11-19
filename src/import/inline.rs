//! Inline HTML to inline nodes (C2-M-014 to C2-M-020, C2-M-044 to C2-M-051),
//! with the elements and carriers of a 0.2 render (C2-H-072 to C2-H-101).

use super::attrs::{self, Mapped, Treatment, class_list, is_key, is_raw_format, is_raw_span};
use super::dom::Id;
use super::notes;
use super::{Code, Cx};
use crate::chars::fold_label;
use serde_json::{Map, Value, json};

pub(crate) fn ty(v: &Value) -> &str {
    v["type"].as_str().unwrap_or("")
}

pub(crate) fn node(kind: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("type".into(), kind.into());
    m
}

pub(crate) fn with(kind: &str, children: Vec<Value>, mapped: &Mapped) -> Value {
    let mut m = node(kind);
    m.insert("children".into(), Value::Array(children));
    mapped.apply(&mut m);
    Value::Object(m)
}

pub(crate) fn text(value: &str) -> Value {
    json!({ "type": "text", "value": value.replace('\0', "\u{fffd}") })
}

pub(crate) fn soft_text(value: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = value.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let b = bytes.get(at).copied().unwrap_or(0);
        if b != b'\n' && b != b'\r' {
            at += 1;
            continue;
        }
        if at > start {
            out.push(text(value.get(start..at).unwrap_or("")));
        }
        out.push(json!({ "type": "soft_break" }));
        if b == b'\r' && bytes.get(at + 1) == Some(&b'\n') {
            at += 1;
        }
        at += 1;
        start = at;
    }
    if start < bytes.len() {
        out.push(text(value.get(start..).unwrap_or("")));
    }
    out
}

fn str_of(v: &Value) -> &str {
    v["value"].as_str().unwrap_or("")
}

fn set_str(v: &mut Value, s: String) {
    if let Some(m) = v.as_object_mut() {
        m.insert("value".into(), s.into());
    }
}

pub(crate) fn coalesce(children: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(children.len());
    for child in children {
        if ty(&child) == "text"
            && let Some(prev) = out.last_mut()
            && ty(prev) == "text"
        {
            let joined = format!("{}{}", str_of(prev), str_of(&child));
            set_str(prev, joined);
            continue;
        }
        out.push(child);
    }
    out
}

pub(crate) fn paragraph(children: Vec<Value>) -> Value {
    json!({ "type": "paragraph", "children": coalesce(children) })
}

const EDGE_ELEMENTS: &[&str] = &[
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "td",
    "th",
    "dt",
    "caption",
    "figcaption",
];

fn horizontal(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn edge_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0c' | '\r')
}

fn is_break(v: Option<&Value>) -> bool {
    v.is_some_and(|v| matches!(ty(v), "soft_break" | "hard_break"))
}

fn append_soft(out: &mut Vec<Value>, child: Value) -> bool {
    let mut changed = false;
    if let Some(prev) = out.last_mut()
        && ty(prev) == "text"
    {
        let value = str_of(prev).trim_end_matches(horizontal).to_string();
        let empty = value.is_empty();
        if value != str_of(prev) {
            set_str(prev, value);
            changed = true;
        }
        if empty {
            out.pop();
        }
    }
    if is_break(out.last()) {
        return true;
    }
    out.push(child);
    changed
}

/// C2-H-045: a render writes a hard break as `<br>` and a line ending.
fn append_hard(out: &mut Vec<Value>, child: Value) -> bool {
    let dropped = out.last().is_some_and(|v| ty(v) == "soft_break");
    if dropped {
        out.pop();
    }
    out.push(child);
    dropped
}

fn append_text(out: &mut Vec<Value>, mut child: Value) -> bool {
    if !is_break(out.last()) {
        out.push(child);
        return false;
    }
    let value = str_of(&child).trim_start_matches(horizontal).to_string();
    let changed = value != str_of(&child);
    let empty = value.is_empty();
    if !empty {
        set_str(&mut child, value);
        out.push(child);
    }
    changed || empty
}

impl Cx {
    pub(crate) fn normalize_breaks(&mut self, children: Vec<Value>, host: Id) -> Vec<Value> {
        let mut out = Vec::with_capacity(children.len());
        let mut changed = false;
        for child in children {
            changed = match ty(&child) {
                "soft_break" => append_soft(&mut out, child),
                "hard_break" => append_hard(&mut out, child),
                "text" => append_text(&mut out, child),
                _ => {
                    out.push(child);
                    false
                }
            } || changed;
        }
        if changed {
            self.report(
                host,
                Code::WhitespaceNormalized,
                "HTML whitespace around a line ending removed",
            );
        }
        out
    }

    /// C2-M-018, C2-M-019: whitespace and breaks at the edges of a block.
    pub(crate) fn trim_inline_edges(&mut self, mut children: Vec<Value>, host: Id) -> Vec<Value> {
        let leading = trim_leading(&mut children);
        let trailing = trim_trailing(&mut children);
        if leading || trailing {
            let raw: Vec<Id> = if self.dom.is_element(host) {
                self.kids(host)
            } else {
                vec![host]
            };
            let at = if leading { raw.first() } else { raw.last() };
            self.report(
                at.copied().unwrap_or(host),
                Code::WhitespaceNormalized,
                "Whitespace at the edge of a block removed",
            );
        }
        children
    }

    pub(crate) fn inline_children(&mut self, id: Id) -> Vec<Value> {
        let nodes = self.kids(id);
        self.inline_of(id, &nodes)
    }

    pub(crate) fn inline_of(&mut self, id: Id, nodes: &[Id]) -> Vec<Value> {
        let mut all = Vec::new();
        for &n in nodes {
            all.extend(self.inline(n));
        }
        let children = self.normalize_breaks(coalesce(all), id);
        if self.dom.tag(id).is_some_and(|t| EDGE_ELEMENTS.contains(&t)) {
            self.trim_inline_edges(children, id)
        } else {
            children
        }
    }

    pub(crate) fn inline(&mut self, id: Id) -> Vec<Value> {
        if self.dom.is_text(id) {
            return soft_text(self.dom.text(id));
        }
        if self.dom.is_comment(id) {
            return self.raw_inline(id);
        }
        if self.dom.is_element(id) {
            return self.inline_element(id);
        }
        self.report(id, Code::ElementDropped, "Doctype dropped");
        Vec::new()
    }

    /// C2-M-065, C2-M-066: a comment or raw element in inline content.
    fn raw_inline(&mut self, id: Id) -> Vec<Value> {
        if !self.keeps_raw(id) {
            return Vec::new();
        }
        vec![json!({ "type": "raw_inline", "format": "html", "value": self.raw_value(id) })]
    }

    /// C2-M-065, C2-M-067: the outer HTML, without the reserved names.
    pub(crate) fn raw_value(&self, id: Id) -> String {
        self.dom.outer_html(id, &attrs::is_carrier)
    }

    /// C2-M-066: with `rawHtml` off, raw content is dropped and reported.
    pub(crate) fn keeps_raw(&mut self, id: Id) -> bool {
        if self.raw_html {
            return true;
        }
        self.report(
            id,
            Code::ElementDropped,
            "Raw content dropped with rawHtml off",
        );
        false
    }

    /// C2-M-057: an element of the treatment `drop` is reported, but `rp`.
    pub(crate) fn drop_element(&mut self, id: Id) {
        if !self.dom.is(id, "rp") {
            let tag = self.dom.tag(id).unwrap_or("").to_string();
            self.report(id, Code::ElementDropped, format!("HTML {tag} dropped"));
        }
    }

    fn treated(&mut self, id: Id) -> Option<Vec<Value>> {
        let kind = attrs::treatment(&self.dom, id);
        let tag = self.dom.tag(id).unwrap_or("").to_string();
        if kind == Treatment::Drop {
            self.drop_element(id);
            return Some(Vec::new());
        }
        // C2-M-066: an `<input>` that is no task box is raw.
        if kind == Treatment::Raw || tag == "input" {
            return Some(self.raw_inline(id));
        }
        if kind == Treatment::Unwrap || attrs::is_orphan(&self.dom, id) {
            if tag != "html" && tag != "body" {
                self.report(id, Code::ElementMapped, format!("HTML {tag} unwrapped"));
            }
            return Some(self.inline_children(id));
        }
        // C2-M-014: a block element in inline content is a span.
        if kind == Treatment::Map && !attrs::is_block_element(&self.dom, id) {
            return None;
        }
        self.report(
            id,
            Code::ElementMapped,
            format!("HTML {tag} read as a span"),
        );
        let children = self.inline_children(id);
        let mapped = self.plain_attrs(id);
        Some(self.non_empty(vec![with("span", children, &mapped)], id))
    }

    fn inline_element(&mut self, id: Id) -> Vec<Value> {
        if let Some(own) = self.carve_inline(id) {
            return own;
        }
        if let Some(kept) = self.treated(id) {
            return kept;
        }
        if let Some(leaf) = self.leaf_inline(id) {
            return leaf;
        }
        let children = self.inline_children(id);
        let tag = self.dom.tag(id).unwrap_or("").to_string();
        if let Some(kind) = format_kind(&tag) {
            let mapped = self.plain_attrs(id);
            return self.non_empty(vec![with(kind, children, &mapped)], id);
        }
        match tag.as_str() {
            "a" => {
                let link = self.html_link(id, children);
                self.non_empty(link, id)
            }
            "abbr" => self.html_abbreviation(id, children),
            "time" | "kbd" => vec![self.semantic_span(id, children)],
            _ => {
                let mapped = self.plain_attrs(id);
                self.non_empty(vec![with("span", children, &mapped)], id)
            }
        }
    }

    fn leaf_inline(&mut self, id: Id) -> Option<Vec<Value>> {
        Some(match self.dom.tag(id)? {
            "br" => vec![json!({ "type": "hard_break" })],
            "img" => self.html_image(id),
            "math" => self.html_math(id),
            "ruby" => self.html_ruby(id),
            "code" => {
                let mut m = node("code");
                m.insert("value".into(), self.dom.plain(id).into());
                self.plain_attrs(id).apply(&mut m);
                vec![Value::Object(m)]
            }
            _ => return None,
        })
    }

    /// C2-M-069: an inline node that needs content and has none is dropped,
    /// except a span with attributes; a drop that loses an attribute is reported.
    fn non_empty(&mut self, nodes: Vec<Value>, id: Id) -> Vec<Value> {
        let mut out = Vec::with_capacity(nodes.len());
        for item in nodes {
            let kind = ty(&item).to_string();
            let empty = item
                .get("children")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty);
            if !NEEDS_CONTENT.contains(&kind.as_str()) || !empty {
                out.push(item);
                continue;
            }
            let kept = item.get("attrs").is_some();
            if kind == "span" && (kept || item.get("lang").is_some() || item.get("dir").is_some()) {
                out.push(item);
                continue;
            }
            if kept {
                let tag = self.dom.tag(id).unwrap_or("").to_string();
                self.report(
                    id,
                    Code::StructureRepaired,
                    format!("Empty HTML {tag} with attributes dropped"),
                );
            }
        }
        out
    }

    fn carve_inline(&mut self, id: Id) -> Option<Vec<Value>> {
        match self.dom.tag(id)? {
            "a" => self.footnote_inline(id).or_else(|| self.anchor_carrier(id)),
            "span" => self.span_carrier(id).or_else(|| self.classed_span(id)),
            "sup" => self.note_sup(id),
            _ => None,
        }
    }

    fn is_note_ref(&self, id: Id) -> bool {
        self.dom.is(id, "a")
            && (self.dom.attr(id, "role") == Some("doc-noteref")
                || self.dom.has(id, "data-footnote-ref"))
    }

    /// C2-M-062: a `<sup>` that only holds a note reference, or whose first
    /// class is `footnote-ref`, is that reference alone.
    fn note_sup(&mut self, id: Id) -> Option<Vec<Value>> {
        let nodes = self.kids(id);
        if let [only] = nodes.as_slice()
            && self.is_note_ref(*only)
        {
            return self.footnote_inline(*only);
        }
        if class_list(&self.dom, id).first().map(String::as_str) != Some("footnote-ref") {
            return None;
        }
        let anchor = self
            .dom
            .elements(id)
            .into_iter()
            .find(|&c| self.dom.is(c, "a"))?;
        if self.is_note_ref(anchor) {
            return self.footnote_inline(anchor);
        }
        let key = footnote_key(self.dom.attr(anchor, "href").unwrap_or(""));
        Some(vec![
            json!({ "type": "footnote_ref", "key": key, "namespace": "footnote" }),
        ])
    }

    /// C2-H-088: a note reference; C2-H-114: its key or the inline footnote.
    fn footnote_inline(&mut self, id: Id) -> Option<Vec<Value>> {
        // C2-M-063: a backlink is dropped without a report.
        if notes::is_backlink(&self.dom, id) {
            return Some(Vec::new());
        }
        if !self.is_note_ref(id) {
            return None;
        }
        let href = self.dom.attr(id, "href").unwrap_or("").to_string();
        let target = href.strip_prefix('#').unwrap_or(&href).to_string();
        let note = self.notes.get(&target).cloned();
        // C2-M-062: naming no note, with no fragment or one that is empty once
        // the prefixes are cut, it is a link.
        if note.is_none() {
            match href.find('#') {
                None => return None,
                Some(hash) if footnote_key(href.get(hash + 1..).unwrap_or("")).is_empty() => {
                    return None;
                }
                Some(_) => {}
            }
        }
        let label = self
            .dom
            .attr(id, "data-carve-footnote-label")
            .map(str::to_string);
        let generated = is_generated_ref_id(self.dom.attr(id, "id"), &href, &self.dom.plain(id));
        let mut omit = vec!["href", "role", "data-footnote-ref"];
        if generated {
            omit.push("id");
        }
        let mapped = self.attrs(id, &omit, false);
        if let Some(n) = &note
            && n.inline
            && label.is_none()
        {
            let children = self.note_inline(n.item);
            return Some(vec![with("inline_footnote", children, &mapped)]);
        }
        let key = self.note_key(id, label.as_deref(), note.as_ref(), &href);
        let mut m = node("footnote_ref");
        m.insert("key".into(), key.into());
        m.insert("namespace".into(), "footnote".into());
        mapped.apply(&mut m);
        Some(vec![Value::Object(m)])
    }

    fn note_key(
        &mut self,
        id: Id,
        label: Option<&str>,
        note: Option<&notes::Note>,
        href: &str,
    ) -> String {
        if self.dom.has(id, "data-carve-xref") {
            self.ignored_carrier(id, "data-carve-xref", "the element is a footnote reference");
        }
        let fault = label.and_then(|l| label_fault(&self.dom, id, l, note));
        if let Some(l) = label
            && fault.is_none()
        {
            return l.to_string();
        }
        if let Some(f) = fault {
            self.ignored_carrier(id, "data-carve-footnote-label", f);
        }
        note.and_then(|n| n.label.clone())
            .unwrap_or_else(|| footnote_key(href))
    }

    /// C2-H-090: the body of an inline footnote is one paragraph.
    fn note_inline(&mut self, item: Id) -> Vec<Value> {
        match self
            .dom
            .elements(item)
            .into_iter()
            .find(|&c| self.dom.is(c, "p"))
        {
            Some(p) => self.inline_children(p),
            None => Vec::new(),
        }
    }

    fn anchor_carrier(&mut self, id: Id) -> Option<Vec<Value>> {
        let forms = link_forms(&self.dom, id);
        if forms.len() > 1 {
            for name in forms.iter().filter(|f| **f != "data-carve-ref") {
                self.ignored_carrier(id, name, "the link has one form");
            }
            return None;
        }
        match forms.first().copied() {
            Some("data-carve-autolink") => self.autolink(id),
            Some("data-carve-xref") => self.xref(id),
            _ => None,
        }
    }

    fn autolink(&mut self, id: Id) -> Option<Vec<Value>> {
        let href = self.dom.attr(id, "href").map(str::to_string);
        let Some(href) = href.filter(|_| self.dom.elements(id).is_empty()) else {
            self.ignored_carrier(
                id,
                "data-carve-autolink",
                "an autolink is a destination and its text",
            );
            return None;
        };
        let label = self.dom.plain(id);
        let mut m = node("autolink");
        m.insert("href".into(), href.clone().into());
        if label != href {
            m.insert("text".into(), label.into());
        }
        self.attrs(id, &["href"], false).apply(&mut m);
        Some(vec![Value::Object(m)])
    }

    /// C2-H-096: the text of a cross-reference is generated; its target reads back.
    fn xref(&mut self, id: Id) -> Option<Vec<Value>> {
        let href = self.dom.attr(id, "href").unwrap_or("").to_string();
        let key = href.strip_prefix('#').unwrap_or("");
        // C2-I-096: a cross-reference id holds no `>`, space or line break.
        if !href.starts_with('#') || key.is_empty() || key.contains(['>', ' ', '\n']) {
            self.ignored_carrier(id, "data-carve-xref", "the target is no cross-reference id");
            return None;
        }
        let mut m = node("xref");
        m.insert("key".into(), key.into());
        m.insert("namespace".into(), "heading".into());
        m.insert("href".into(), href.clone().into());
        self.attrs(id, &["href"], false).apply(&mut m);
        Some(vec![Value::Object(m)])
    }

    fn span_carrier(&mut self, id: Id) -> Option<Vec<Value>> {
        if let Some(raw) = self.dom.attr(id, "data-carve-raw").map(str::to_string) {
            return self.raw_span(id, &raw);
        }
        if self.dom.has(id, "data-carve-caption-number") {
            return self.caption_number(id);
        }
        None
    }

    fn in_caption(&self, id: Id) -> bool {
        let mut at = self.dom.parent(id);
        while let Some(p) = at {
            if matches!(self.dom.tag(p), Some("figcaption" | "caption")) {
                return true;
            }
            at = self.dom.parent(p);
        }
        false
    }

    /// C2-H-186: a caption number mark in place of the span.
    fn caption_number(&mut self, id: Id) -> Option<Vec<Value>> {
        if self.in_caption(id)
            && self.dom.attrs(id).len() == 1
            && self.dom.children(id).iter().all(|&c| self.dom.is_text(c))
        {
            return Some(vec![json!({ "type": "caption_number" })]);
        }
        self.ignored_carrier(
            id,
            "data-carve-caption-number",
            "the mark stands outside a caption, holds an element or has attributes",
        );
        None
    }

    /// C2-H-051, C2-H-186: a raw span.
    fn raw_span(&mut self, id: Id, format: &str) -> Option<Vec<Value>> {
        if !is_raw_span(&self.dom, id) {
            let reason = if is_raw_format(format) {
                "the span holds an element or has attributes"
            } else {
                "the value is no raw format name"
            };
            self.ignored_carrier(id, "data-carve-raw", reason);
            return None;
        }
        if !self.keeps_raw(id) {
            return Some(Vec::new());
        }
        Some(vec![
            json!({ "type": "raw_inline", "format": format, "value": self.dom.plain(id) }),
        ])
    }

    /// The spans whose classes a 0.2 render gives the node it writes.
    fn classed_span(&mut self, id: Id) -> Option<Vec<Value>> {
        let classes = class_list(&self.dom, id);
        let first = classes.first()?.clone();
        let second = classes.get(1).map(String::as_str);
        if first == "math" && matches!(second, Some("inline" | "display")) {
            return self.math_span(id, second == Some("display"));
        }
        if first == "critic-comment" {
            let mut m = node("critic_comment");
            m.insert("value".into(), self.dom.plain(id).into());
            self.plain_attrs(id)
                .without_classes(&[&first])
                .apply(&mut m);
            return Some(vec![Value::Object(m)]);
        }
        if first == "mention" || first == "tag" {
            return self.social_span(id, &first);
        }
        // C2-M-035 row `ext`, C2-I-094: NAME has the shape of a key.
        if let Some(name) = first.strip_prefix("ext-")
            && is_key(name)
        {
            let children = self.inline_children(id);
            let mapped = self.plain_attrs(id).without_classes(&[&first]);
            let mut m = node("inline_extension");
            m.insert("name".into(), name.into());
            m.insert("children".into(), Value::Array(children));
            mapped.apply(&mut m);
            return Some(vec![Value::Object(m)]);
        }
        None
    }

    /// C2-H-102: a formula between the delimiters of its display.
    fn math_span(&mut self, id: Id, display: bool) -> Option<Vec<Value>> {
        let value = self.dom.plain(id);
        let (open, close) = if display {
            ("\\[", "\\]")
        } else {
            ("\\(", "\\)")
        };
        if !value.starts_with(open) || !value.ends_with(close) || value.encode_utf16().count() < 4 {
            return None;
        }
        let inner = value.get(2..value.len() - 2).unwrap_or("").to_string();
        let mapped = self
            .plain_attrs(id)
            .without_classes(&["math", "inline", "display"])
            .without_keys(&["role"]);
        let mut m = node("math");
        m.insert("display".into(), display.into());
        m.insert("value".into(), inner.into());
        mapped.apply(&mut m);
        Some(vec![Value::Object(m)])
    }

    /// C2-H-100: an inert mention or tag, its name behind its sigil.
    fn social_span(&mut self, id: Id, kind: &str) -> Option<Vec<Value>> {
        let strong = self.dom.elements(id);
        let name = self.dom.plain(id);
        let sigil = if kind == "mention" { '@' } else { '#' };
        let only_strong = matches!(strong.as_slice(), [s] if self.dom.is(*s, "strong"));
        let rest = name.strip_prefix(sigil)?;
        if !only_strong || rest.is_empty() {
            return None;
        }
        let mapped = self.plain_attrs(id).without_classes(&[kind]);
        let mut m = if kind == "tag" {
            let mut m = node("tag");
            m.insert("name".into(), rest.into());
            m
        } else {
            let mut m = node("mention");
            m.insert("target".into(), rest.into());
            m.insert("kind".into(), "user".into());
            m
        };
        mapped.apply(&mut m);
        Some(vec![Value::Object(m)])
    }

    /// C2-H-098, C2-H-075: an abbreviation a template defines, or a span.
    fn html_abbreviation(&mut self, id: Id, children: Vec<Value>) -> Vec<Value> {
        let expansion = self.dom.attr(id, "title").map(str::to_string);
        let key = self.dom.plain(id);
        let defined = self
            .abbreviations
            .get(&key)
            .is_some_and(|d| d["expansion"].as_str().map(str::to_string) == expansion);
        if defined && self.dom.elements(id).is_empty() {
            let mut m = node("abbreviation");
            m.insert("key".into(), key.into());
            m.insert("namespace".into(), "abbr".into());
            if let Some(e) = expansion {
                m.insert("expansion".into(), e.into());
            }
            self.attrs(id, &["title"], false).apply(&mut m);
            return vec![Value::Object(m)];
        }
        vec![self.semantic_span(id, children)]
    }

    /// C2-H-075: the span whose key named the element.
    fn semantic_span(&mut self, id: Id, children: Vec<Value>) -> Value {
        let tag = self.dom.tag(id).unwrap_or("").to_string();
        let target = match tag.as_str() {
            "abbr" => Some("title"),
            "time" => Some("datetime"),
            _ => None,
        };
        let value = target
            .and_then(|t| self.dom.attr(id, t))
            .unwrap_or("")
            .to_string();
        // C2-A-035: an attribute of the element's own name would be a second key.
        if self.dom.attr(id, &tag).is_some() {
            self.report(
                id,
                Code::AttributeDropped,
                format!("An {tag} attribute beside the key its element gives dropped"),
            );
        }
        let omit: Vec<&str> = match target {
            Some(t) => vec![t, tag.as_str()],
            None => vec![tag.as_str()],
        };
        let mut mapped = self.attrs(id, &omit, false);
        mapped.attrs.key_values.insert(0, (tag.clone(), value));
        with("span", children, &mapped)
    }

    fn html_math(&mut self, id: Id) -> Vec<Value> {
        let annotation = self
            .dom
            .elements(id)
            .into_iter()
            .flat_map(|c| self.dom.elements(c))
            .find(|&c| {
                self.dom.is(c, "annotation")
                    && matches!(
                        self.dom.attr(c, "encoding"),
                        Some("application/x-tex" | "text/x-tex" | "LaTeX")
                    )
            });
        let source = match annotation {
            Some(a) => Some(self.dom.plain(a)),
            None => self.dom.attr(id, "alttext").map(str::to_string),
        };
        if let Some(source) = source {
            let display = self.dom.attr(id, "display") == Some("block");
            return vec![json!({ "type": "math", "display": display, "value": source })];
        }
        self.report(
            id,
            Code::ElementMapped,
            "MathML without a TeX source read as its text",
        );
        vec![text(&self.dom.plain(id))]
    }

    fn ruby_base(&mut self, id: Id) -> Vec<Value> {
        match self.dom.tag(id) {
            Some("rb") => self.inline_children(id),
            // C2-H-197: every render writes the parentheses again.
            Some("rp") => Vec::new(),
            _ => self.inline(id),
        }
    }

    fn html_ruby(&mut self, id: Id) -> Vec<Value> {
        let mut pairs = Vec::new();
        let mut base = Vec::new();
        for child in self.kids(id) {
            if self.dom.is(child, "rt") {
                let annotation = self.inline_children(child);
                pairs.push(json!({
                    "type": "ruby_pair",
                    "base": coalesce(std::mem::take(&mut base)),
                    "annotation": annotation,
                }));
            } else {
                base.extend(self.ruby_base(child));
            }
        }
        if !base.is_empty() {
            self.report(
                id,
                Code::StructureRepaired,
                "Ruby base without annotation read as text",
            );
        }
        if pairs.is_empty() {
            return base;
        }
        let mut m = node("ruby");
        m.insert("pairs".into(), Value::Array(pairs));
        self.plain_attrs(id).apply(&mut m);
        let mut out = vec![Value::Object(m)];
        out.extend(base);
        out
    }

    /// C2-H-114, C2-H-117: the key and form of a reference.
    fn reference(&mut self, id: Id) -> Option<(String, &'static str)> {
        let key = self.dom.attr(id, "data-carve-ref").map(str::to_string);
        let collapsed = self.dom.has(id, "data-carve-ref-collapsed");
        let shortcut = self.dom.has(id, "data-carve-ref-shortcut");
        if key.is_none() && !collapsed && !shortcut {
            return None;
        }
        if let Some((name, reason)) = self.reference_fault(id, key.as_deref(), collapsed) {
            self.ignored_carrier(id, name, reason);
            return None;
        }
        let form = match (collapsed, shortcut) {
            (true, true) => {
                for name in ["data-carve-ref-collapsed", "data-carve-ref-shortcut"] {
                    self.ignored_carrier(id, name, "a reference has one form");
                }
                "full"
            }
            (true, false) => "collapsed",
            (false, true) => "shortcut",
            (false, false) => "full",
        };
        Some((key?, form))
    }

    fn reference_fault(
        &self,
        id: Id,
        key: Option<&str>,
        collapsed: bool,
    ) -> Option<(&'static str, &'static str)> {
        match key {
            None => Some((
                if collapsed {
                    "data-carve-ref-collapsed"
                } else {
                    "data-carve-ref-shortcut"
                },
                "there is no reference key",
            )),
            Some(k) if k.is_empty() || fold_label(k) != k => {
                Some(("data-carve-ref", "the key is not folded"))
            }
            Some(_) if link_forms(&self.dom, id).len() > 1 => {
                Some(("data-carve-ref", "the link has one form"))
            }
            Some(_) => None,
        }
    }

    /// C2-H-019, C2-H-191: the first reference of each key gives back its definition.
    fn define(&mut self, key: &str, href: Option<&str>, id: Id) {
        let Some(href) = href else { return };
        if self.definitions.iter().any(|(k, _)| k == key) {
            return;
        }
        if self.dom.is(id, "a") && href.starts_with('#') {
            self.anchor_keys.insert(key.to_string());
        }
        let mut m = node("link_reference_definition");
        m.insert("key".into(), key.into());
        m.insert("namespace".into(), "link".into());
        m.insert("href".into(), href.into());
        if let Some(t) = self.dom.attr(id, "title") {
            m.insert("tooltip".into(), t.into());
        }
        self.definitions.push((key.to_string(), Value::Object(m)));
    }

    fn safe_url(&mut self, id: Id, url: &str) -> Option<String> {
        if crate::html::url::allowed(url, &crate::options::UrlPolicy::default()) {
            return Some(url.to_string());
        }
        let tag = self.dom.tag(id).unwrap_or("").to_string();
        self.report(
            id,
            Code::DestinationDenied,
            format!("Denied URL scheme in {tag}"),
        );
        None
    }

    fn html_link(&mut self, id: Id, children: Vec<Value>) -> Vec<Value> {
        let href = self.dom.attr(id, "href").map(str::to_string);
        let safe = href.and_then(|h| self.safe_url(id, &h));
        let reference = self.reference(id);
        if let Some((key, _)) = &reference {
            self.define(key, safe.as_deref(), id);
        }
        let mut m = node("link");
        m.insert("children".into(), Value::Array(children));
        if let Some(s) = &safe {
            m.insert("href".into(), s.clone().into());
        }
        // C2-H-141: the title of a reference is that of its definition.
        if reference.is_none()
            && let Some(t) = self.dom.attr(id, "title")
        {
            m.insert("tooltip".into(), t.into());
        }
        if let Some((key, form)) = &reference {
            m.insert("key".into(), key.clone().into());
            m.insert("namespace".into(), "link".into());
            m.insert("refForm".into(), (*form).into());
        }
        self.attrs(id, &["href", "title"], false).apply(&mut m);
        with_destination(Value::Object(m))
    }

    pub(crate) fn html_image(&mut self, id: Id) -> Vec<Value> {
        let src = self.dom.attr(id, "src").map(str::to_string);
        let safe = src.and_then(|s| self.safe_url(id, &s));
        let long = self.dom.attr(id, "longdesc").map(str::to_string);
        let long_safe = long.and_then(|l| self.safe_url(id, &l));
        let reference = self.reference(id);
        if let Some((key, _)) = &reference {
            self.define(key, safe.as_deref(), id);
        }
        let mut m = node("image");
        if let Some(s) = &safe {
            m.insert("src".into(), s.clone().into());
        }
        m.insert("alt".into(), self.dom.attr(id, "alt").unwrap_or("").into());
        if reference.is_none()
            && let Some(t) = self.dom.attr(id, "title")
        {
            m.insert("tooltip".into(), t.into());
        }
        if let Some((key, form)) = &reference {
            m.insert("key".into(), key.clone().into());
            m.insert("namespace".into(), "link".into());
            m.insert("refForm".into(), (*form).into());
        }
        if let Some(l) = long_safe {
            let link = json!({ "type": "link", "href": l, "children": [text(&l)] });
            m.insert("longDescription".into(), json!([paragraph(vec![link])]));
        }
        self.attrs(id, &["src", "alt", "title", "longdesc"], false)
            .apply(&mut m);
        with_destination(Value::Object(m))
    }
}

const NEEDS_CONTENT: &[&str] = &[
    "emphasis",
    "strong",
    "strike",
    "delete",
    "insert",
    "highlight",
    "underline",
    "subscript",
    "superscript",
    "span",
    "link",
];

fn format_kind(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "em" | "i" => "emphasis",
        "strong" | "b" => "strong",
        "s" | "strike" => "strike",
        "del" => "delete",
        "ins" => "insert",
        "mark" => "highlight",
        "u" => "underline",
        "sub" => "subscript",
        "sup" => "superscript",
        _ => return None,
    })
}

fn trim_leading(children: &mut Vec<Value>) -> bool {
    let mut trimmed = false;
    while let Some(first) = children.first_mut() {
        match ty(first) {
            "soft_break" => {
                children.remove(0);
            }
            "text" => {
                let value = str_of(first);
                let stripped = value.trim_start_matches(edge_space);
                if stripped == value {
                    break;
                }
                if stripped.is_empty() {
                    children.remove(0);
                } else {
                    let s = stripped.to_string();
                    set_str(first, s);
                }
            }
            _ => break,
        }
        trimmed = true;
    }
    trimmed
}

fn trim_trailing(children: &mut Vec<Value>) -> bool {
    let mut trimmed = false;
    while let Some(last) = children.last_mut() {
        match ty(last) {
            "soft_break" | "hard_break" => {
                children.pop();
            }
            "text" => {
                let value = str_of(last);
                let stripped = value.trim_end_matches(edge_space);
                if stripped == value {
                    break;
                }
                if stripped.is_empty() {
                    children.pop();
                } else {
                    let s = stripped.to_string();
                    set_str(last, s);
                }
            }
            _ => break,
        }
        trimmed = true;
    }
    trimmed
}

pub(crate) fn footnote_key(fragment: &str) -> String {
    let hash = fragment.strip_prefix('#').unwrap_or(fragment);
    for prefix in ["user-content-fn-", "fn-", "fn"] {
        if let Some(rest) = hash.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    hash.to_string()
}

/// C2-H-186: why the `data-carve-footnote-label` of a reference is not read.
fn label_fault(
    dom: &super::dom::Dom,
    id: Id,
    label: &str,
    note: Option<&notes::Note>,
) -> Option<&'static str> {
    if label.is_empty() || fold_label(label) != label {
        return Some("the key is not folded");
    }
    if dom.has(id, "data-carve-xref") {
        return Some("the reference is also a cross-reference");
    }
    note.filter(|n| n.label.as_deref() != Some(label))
        .map(|_| "it does not name the note it links to")
}

/// C2-H-088, C2-H-202: the id a render gives the Nth reference to note K.
fn is_generated_ref_id(id: Option<&str>, href: &str, shown: &str) -> bool {
    let Some(id) = id else { return true };
    let Some(number) = href.strip_prefix("#fn") else {
        return true;
    };
    [format!("fnref{number}"), format!("fnref{shown}")]
        .iter()
        .any(|stem| id == stem || suffixed(id, stem))
}

fn suffixed(id: &str, stem: &str) -> bool {
    let Some(rest) = id.strip_prefix(stem).and_then(|r| r.strip_prefix('-')) else {
        return false;
    };
    rest.split('-').all(|part| {
        !part.is_empty()
            && (part == "0" || !part.starts_with('0'))
            && part.bytes().all(|b| b.is_ascii_digit())
            && part.parse::<u64>().is_ok_and(|n| n > 1)
    })
}

const LINK_FORMS: &[&str] = &["data-carve-ref", "data-carve-autolink", "data-carve-xref"];

/// C2-H-185: a reference, an autolink and a cross-reference exclude each other.
pub(crate) fn link_forms(dom: &super::dom::Dom, id: Id) -> Vec<&'static str> {
    LINK_FORMS
        .iter()
        .copied()
        .filter(|n| dom.has(id, n))
        .collect()
}

/// A link with no destination and no key becomes its text; an image its alt.
fn with_destination(v: Value) -> Vec<Value> {
    let kind = ty(&v);
    let attrs = v.get("attrs").cloned();
    let span = |children: Vec<Value>| match &attrs {
        None => children,
        Some(a) => vec![json!({ "type": "span", "children": children, "attrs": a })],
    };
    if kind == "link" && v.get("href").is_none() && v.get("key").is_none() {
        return span(
            v.get("children")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
    }
    if kind != "image" || v.get("src").is_some() || v.get("key").is_some() {
        return vec![v];
    }
    let alt = v.get("alt").and_then(Value::as_str).unwrap_or("");
    span(if alt.is_empty() {
        Vec::new()
    } else {
        vec![text(alt)]
    })
}
