//! Blocks of HTML to block nodes (C2-M-011 to C2-M-043, C2-M-059 to C2-M-064).

use super::attrs::{
    self, Attrs, Mapped, Treatment, class_list, dropped_attribute, is_carrier, is_raw_span,
};
use super::dom::Id;
use super::inline::{
    coalesce, footnote_key, node, paragraph, parent, soft_text, ty, with, with_role,
};
use super::notes::is_footnote_section;
use super::{Code, Cx};
use serde_json::{Map, Value, json};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reader {
    Block,
    DocumentChild,
}

fn html_integer(value: Option<&str>) -> Option<i64> {
    let v = value?;
    let rest = v.trim_start_matches([' ', '\t', '\n', '\r']);
    let (sign, digits) = match rest.as_bytes().first() {
        Some(b'-') => (-1, rest.get(1..).unwrap_or("")),
        Some(b'+') => (1, rest.get(1..).unwrap_or("")),
        _ => (1, rest),
    };
    let end = digits.bytes().take_while(u8::is_ascii_digit).count();
    let number: i64 = digits.get(..end).filter(|d| !d.is_empty())?.parse().ok()?;
    // Number.isSafeInteger
    (number < (1 << 53)).then_some(sign * number)
}

fn html_span(value: Option<&str>) -> Option<i64> {
    html_integer(value).filter(|&n| n > 1)
}

/// C2-H-020: a bare image is a block image; C2-H-102: a paragraph of one
/// display formula has the role `math`.
fn run_paragraph(children: Vec<Value>) -> Value {
    if let [only] = children.as_slice()
        && ty(only) == "math"
        && only["display"] == true
    {
        return with_role("paragraph", "math", children);
    }
    paragraph(children)
}

const ADMONITION_KINDS: &[&str] = &[
    "note", "tip", "warning", "danger", "info", "success", "example", "quote",
];
const DIRECTIVE_KINDS: &[&str] = &[
    "bibliography",
    "footnotes",
    "glossary",
    "index",
    "references",
    "toc",
];
const SCOPES: &[&str] = &["col", "row", "colgroup", "rowgroup"];
const VALIGNS: &[&str] = &["top", "middle", "bottom", "baseline"];
const ALIGNS: &[&str] = &["left", "right", "center"];
// C2-M-031, C2-B-219: a start lies between 0 and 999999999.
const MAX_START: i64 = 999_999_999;

struct Titled {
    title: Option<Vec<Value>>,
    label: Option<String>,
    rest: Vec<Id>,
}

impl Titled {
    fn apply(&self, m: &mut Map<String, Value>) {
        if let Some(t) = &self.title {
            m.insert("title".into(), Value::Array(t.clone()));
        }
        if let Some(l) = &self.label {
            m.insert("groupLabel".into(), l.clone().into());
        }
    }
}

fn is_kind(kinds: &[&str], value: Option<&str>) -> bool {
    value.is_some_and(|v| kinds.contains(&v))
}

/// The heading id a section or heading carries (C2-H-114).
#[derive(Default)]
struct HeadingId {
    authored: Option<String>,
}

impl Cx {
    fn is_phrasing(&self, id: Id) -> bool {
        self.dom.is_text(id)
            || (self.dom.is_element(id)
                && !attrs::is_block_element(&self.dom, id)
                && !self.dom.is(id, "img"))
    }

    /// C2-M-065: a comment and a raw element.
    fn is_raw(&self, id: Id) -> bool {
        self.dom.is_comment(id)
            || (self.dom.is_element(id)
                && (attrs::treatment(&self.dom, id) == Treatment::Raw
                    || is_raw_span(&self.dom, id)))
    }

    fn read(&mut self, id: Id, reader: Reader) -> Vec<Value> {
        match reader {
            Reader::Block => self.block(id),
            Reader::DocumentChild => self.document_child(id),
        }
    }

    fn flush(&mut self, run: &[Id], host: Id, reader: Reader, out: &mut Vec<Value>) {
        let solid: Vec<Id> = run
            .iter()
            .copied()
            .filter(|&n| !self.dom.is_blank(n))
            .collect();
        if solid.is_empty() {
            return;
        }
        // C2-M-013: a run of images and comments is one image paragraph per
        // image; C2-M-065: raw nodes alone are blocks.
        let all_raw = solid.iter().all(|&n| self.is_raw(n));
        let images = solid.iter().any(|&n| self.dom.is(n, "img"))
            && solid
                .iter()
                .all(|&n| self.dom.is(n, "img") || self.dom.is_comment(n));
        if all_raw || images {
            for n in solid {
                out.extend(self.read(n, reader));
            }
            return;
        }
        let mut all = Vec::new();
        for &n in run {
            all.extend(self.inline(n));
        }
        let children = self.normalize_breaks(coalesce(all), host);
        let children = self.trim_inline_edges(children, host);
        if !children.is_empty() {
            out.push(run_paragraph(children));
        }
    }

    /// The blocks of a flow: a run of text and inline elements is one paragraph (C2-M-012).
    pub(crate) fn flow_of(&mut self, nodes: &[Id], host: Id, reader: Reader) -> Vec<Value> {
        let mut out = Vec::new();
        let mut run: Vec<Id> = Vec::new();
        for &n in nodes {
            if self.is_phrasing(n) || self.dom.is(n, "img") || self.dom.is_comment(n) {
                run.push(n);
            } else {
                self.flush(&run, host, reader, &mut out);
                run.clear();
                out.extend(self.read(n, reader));
            }
        }
        self.flush(&run, host, reader, &mut out);
        out
    }

    fn block_children(&mut self, id: Id) -> Vec<Value> {
        let nodes = self.kids(id);
        self.flow_of(&nodes, id, Reader::Block)
    }

    fn document_child(&mut self, id: Id) -> Vec<Value> {
        if !self.dom.is_element(id) {
            return self.block(id);
        }
        if self.dom.is(id, "template")
            && let Some(t) = self.abbreviation_template(id)
        {
            return t;
        }
        if self.dom.is(id, "section") {
            return self.section(id, Reader::DocumentChild);
        }
        if matches!(self.dom.tag(id), Some("html" | "body")) {
            self.wrapper_attributes(id);
            let nodes = self.kids(id);
            return self.flow_of(&nodes, id, Reader::DocumentChild);
        }
        self.block(id)
    }

    /// C2-M-004: of `<html>` and `<body>` only `lang` and `dir` reach the document.
    fn wrapper_attributes(&mut self, id: Id) {
        let tag = self.dom.tag_name(id);
        for (name, value) in self.dom.attrs(id).to_vec() {
            if name != "lang" && name != "dir" && dropped_attribute(&name, &value).is_none() {
                self.report(
                    id,
                    Code::AttributeDropped,
                    format!("Attribute {name} of {tag} dropped"),
                );
            }
        }
    }

    fn block(&mut self, id: Id) -> Vec<Value> {
        if self.dom.is_text(id) {
            if self.dom.is_blank(id) {
                return Vec::new();
            }
            let children = soft_text(self.dom.text(id));
            let children = self.normalize_breaks(children, id);
            let children = self.trim_inline_edges(children, id);
            return vec![paragraph(children)];
        }
        if self.dom.is_comment(id) {
            return if self.keeps_raw(id) {
                vec![self.raw_block(id)]
            } else {
                Vec::new()
            };
        }
        if self.dom.is_element(id) {
            return self.block_element(id);
        }
        self.report(id, Code::ElementDropped, "Doctype dropped");
        Vec::new()
    }

    /// C2-M-065: a raw block value ends with a line feed (C2-A-045).
    fn raw_block(&self, id: Id) -> Value {
        let mut value = self.raw_value(id);
        if !value.ends_with('\n') {
            value.push('\n');
        }
        json!({ "type": "raw_block", "format": "html", "value": value })
    }

    fn treated_block(&mut self, id: Id) -> Option<Vec<Value>> {
        let kind = attrs::treatment(&self.dom, id);
        let tag = self.dom.tag_name(id);
        if kind == Treatment::Drop {
            if &*tag == "head" {
                return Some(self.head(id));
            }
            self.drop_element(id);
            return Some(Vec::new());
        }
        if kind == Treatment::Raw || is_raw_span(&self.dom, id) {
            return Some(self.raw_element(id));
        }
        if kind == Treatment::Unwrap || attrs::is_orphan(&self.dom, id) {
            if &*tag != "html" && &*tag != "body" {
                self.report(id, Code::ElementMapped, format!("HTML {tag} unwrapped"));
            }
            return Some(self.block_children(id));
        }
        if kind != Treatment::Div {
            return None;
        }
        self.report(id, Code::ElementMapped, format!("HTML {tag} read as a div"));
        let children = self.div_children(id);
        let mapped = self.plain_attrs(id);
        Some(vec![with("div", children, &mapped)])
    }

    /// C2-M-059: the first `<summary>` of a `<details>` and the `<legend>` of a
    /// `<fieldset>` are its first paragraph.
    fn div_children(&mut self, id: Id) -> Vec<Value> {
        let lead = match self.dom.tag(id) {
            Some("details") => Some("summary"),
            Some("fieldset") => Some("legend"),
            _ => None,
        };
        let first = lead.and_then(|l| {
            self.dom
                .elements(id)
                .into_iter()
                .find(|&c| self.dom.is(c, l))
        });
        let Some(first) = first else {
            return self.block_children(id);
        };
        let rest: Vec<Id> = self.kids(id).into_iter().filter(|&c| c != first).collect();
        let title = self.inline_children(first);
        let title = self.trim_inline_edges(title, first);
        let mut out = Vec::new();
        if !title.is_empty() {
            out.push(paragraph(title));
        }
        out.extend(self.flow_of(&rest, id, Reader::Block));
        out
    }

    /// C2-M-005: the head is dropped and reported once; an implied empty head is no fact.
    fn head(&mut self, id: Id) -> Vec<Value> {
        if self.dom.node(id).span.is_none() && self.dom.children(id).is_empty() {
            return Vec::new();
        }
        self.drop_element(id);
        Vec::new()
    }

    fn raw_element(&mut self, id: Id) -> Vec<Value> {
        if self.dom.has(id, "data-carve-abbreviations") {
            self.ignored_carrier(
                id,
                "data-carve-abbreviations",
                "a template is read only at the top level of the flow",
            );
        }
        if self.keeps_raw(id) {
            vec![self.raw_block(id)]
        } else {
            Vec::new()
        }
    }

    fn block_element(&mut self, id: Id) -> Vec<Value> {
        if let Some(t) = self.treated_block(id) {
            return t;
        }
        if let Some(s) = self.simple_block_element(id) {
            return s;
        }
        if let Some(c) = self.container_element(id) {
            return c;
        }
        match self.dom.tag(id).unwrap_or("") {
            "ul" | "ol" | "menu" => self.html_list(id),
            "pre" => vec![self.html_pre(id)],
            "table" => vec![self.table(id)],
            "dl" => self.definition_list(id),
            "figure" => vec![self.figure(id, &[])],
            _ => self.fallback_block(id),
        }
    }

    /// C2-H-047, C2-H-048: the title of the opener on `<pre>`, and its label.
    fn html_pre(&mut self, id: Id) -> Value {
        let code = self
            .dom
            .elements(id)
            .into_iter()
            .find(|&c| self.dom.is(c, "code"));
        let classes = code.map(|c| class_list(&self.dom, c)).unwrap_or_default();
        let language = classes
            .iter()
            .find_map(|c| c.strip_prefix("language-"))
            .map(str::to_string);
        if let Some(c) = code
            && (self.dom.attrs(c).iter().any(|(n, _)| n != "class")
                || classes.iter().any(|n| !n.starts_with("language-")))
        {
            self.report(
                c,
                Code::AttributeDropped,
                "Code element attributes other than language dropped",
            );
        }
        let mut value = self.dom.plain(code.unwrap_or(id));
        if !value.is_empty() && !value.ends_with('\n') {
            value.push('\n');
        }
        let title = self.dom.attr(id, "title").map(str::to_string);
        let label = self.code_label(id, code);
        let mut m = node("code_block");
        m.insert("value".into(), value.into());
        if let Some(l) = language.filter(|l| !l.is_empty()) {
            m.insert("syntax".into(), l.into());
        }
        if let Some(t) = &title {
            m.insert("title".into(), json!([{ "type": "text", "value": t }]));
        }
        if let Some(l) = label {
            m.insert("groupLabel".into(), l.into());
        }
        let omit: &[&str] = if title.is_some() { &["title"] } else { &[] };
        self.attrs(id, omit, false).apply(&mut m);
        Value::Object(m)
    }

    /// C2-H-186, C2-B-117: a fence label holds no `]` and no line break.
    fn code_label(&mut self, id: Id, code: Option<Id>) -> Option<String> {
        let label = self.dom.attr(id, "data-carve-label")?.to_string();
        let only = code.is_some_and(|c| self.dom.children(id).iter().all(|&n| n == c));
        if only && !label.contains([']', '\n', '\r']) {
            return Some(label);
        }
        let reason = if only {
            "the value is no fence label"
        } else {
            "the element holds more than its code"
        };
        self.ignored_carrier(id, "data-carve-label", reason);
        None
    }

    fn simple_block_element(&mut self, id: Id) -> Option<Vec<Value>> {
        let tag = self.dom.is_element(id).then(|| self.dom.tag_name(id))?;
        Some(match &*tag {
            "p" => {
                let p = self.paragraph_element(id);
                self.with_content(p, id)
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = u64::from(tag.as_bytes().get(1).copied().unwrap_or(b'1') - b'0');
                let h = self.heading(id, level, None);
                self.with_content(h, id)
            }
            "hr" => {
                let mut m = node("thematic_break");
                self.plain_attrs(id).apply(&mut m);
                vec![Value::Object(m)]
            }
            "blockquote" => {
                let children = self.block_children(id);
                let mapped = self.plain_attrs(id);
                vec![with("block_quote", children, &mapped)]
            }
            "section" => self.section(id, Reader::Block),
            "aside" => match self.admonition(id) {
                Some(a) => a,
                None => vec![self.div_element(id)],
            },
            _ => return None,
        })
    }

    /// C2-M-069: a paragraph, heading or term with no content is dropped.
    fn with_content(&mut self, v: Value, element: Id) -> Vec<Value> {
        if v.get("children")
            .and_then(Value::as_array)
            .is_none_or(|c| !c.is_empty())
        {
            return vec![v];
        }
        if v.get("attrs").is_some() || v.get("lang").is_some() || v.get("dir").is_some() {
            let tag = self.dom.tag_name(element);
            self.report(
                element,
                Code::StructureRepaired,
                format!("Empty HTML {tag} with attributes dropped"),
            );
        }
        Vec::new()
    }

    fn paragraph_element(&mut self, id: Id) -> Value {
        let children = self.inline_children(id);
        // C2-H-020: a block image keeps its `<p>`. C2-B-197: a shortcut never is one.
        let image = matches!(children.as_slice(), [only] if ty(only) == "image" && only["refForm"] != "shortcut");
        let mut base = if image {
            with_role("paragraph", "image", children)
        } else {
            run_paragraph(children)
        };
        let mapped = self.attrs(id, &[], true);
        if let Some(m) = base.as_object_mut() {
            mapped.apply(m);
        }
        base
    }

    /// C2-H-021, C2-A-134: a heading, its level, and whether its id was authored.
    fn heading(&mut self, id: Id, level: u64, chosen: Option<HeadingId>) -> Value {
        let own = self.heading_id(id);
        let chosen = chosen.unwrap_or(own);
        let omit: &[&str] = if level > 6 {
            &["id", "role", "aria-level"]
        } else {
            &["id"]
        };
        let mut mapped = self.attrs(id, omit, true);
        if let Some(a) = chosen.authored {
            mapped.attrs.id = Some(a);
        }
        let level = self.source_level(id, level);
        let children = self.inline_children(id);
        let mut m = node("heading");
        m.insert("level".into(), level.into());
        m.insert("children".into(), Value::Array(children));
        mapped.apply(&mut m);
        Value::Object(m)
    }

    /// C2-H-186: `data-carve-source-level` gives the level before `headingShift`
    /// when it is a decimal integer from 1 without a leading zero below `level`.
    fn source_level(&mut self, id: Id, level: u64) -> u64 {
        let name = super::attrs::SOURCE_LEVEL;
        let Some(value) = self.dom.attr(id, name) else {
            return level;
        };
        let read = (!value.starts_with('0') && value.bytes().all(|b| b.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten()
            .filter(|&n| n >= 1 && n < level);
        if let Some(n) = read {
            return n;
        }
        self.ignored_carrier(id, name, "the value is no level below the element's");
        level
    }

    fn heading_id(&mut self, id: Id) -> HeadingId {
        let own = self.dom.attr(id, "id").map(str::to_string);
        let explicit = self.dom.has(id, "data-carve-explicit-id");
        let generated = self.dom.has(id, "data-carve-generated-id");
        if explicit && generated {
            self.ignored_carrier(
                id,
                "data-carve-explicit-id",
                "an id is authored or generated, not both",
            );
            return HeadingId { authored: own };
        }
        if (explicit || generated) && own.as_deref().is_none_or(str::is_empty) {
            let name = if explicit {
                "data-carve-explicit-id"
            } else {
                "data-carve-generated-id"
            };
            self.ignored_carrier(id, name, "the element has no id");
            return HeadingId::default();
        }
        if generated {
            return HeadingId::default();
        }
        HeadingId { authored: own }
    }

    pub(super) fn heading_level(&self, id: Id) -> Option<u64> {
        let tag = self.dom.tag(id)?;
        if let Some(d) = tag.strip_prefix('h')
            && d.len() == 1
            && let Some(n) = d.bytes().next().filter(|b| (b'1'..=b'6').contains(b))
        {
            return Some(u64::from(n - b'0'));
        }
        if tag != "div" || self.dom.attr(id, "role") != Some("heading") {
            return None;
        }
        html_integer(self.dom.attr(id, "aria-level"))
            .filter(|&l| l > 6)
            .and_then(|l| u64::try_from(l).ok())
    }

    /// C2-M-027: a render writes no attribute but the id on the section it opens.
    fn section_attributes(&mut self, id: Id) {
        for (name, value) in self.dom.attrs(id).to_vec() {
            if name == "id" || is_carrier(&name) || dropped_attribute(&name, &value).is_some() {
                continue;
            }
            self.report(
                id,
                Code::AttributeDropped,
                format!("Attribute {name} of a section dropped"),
            );
        }
    }

    /// C2-H-026, C2-H-027: a section a render opens at a heading.
    fn section(&mut self, id: Id, reader: Reader) -> Vec<Value> {
        if is_footnote_section(&self.dom, id) {
            return self.footnote_section(id);
        }
        let kids = self.kids(id);
        let first = self.dom.elements(id).first().copied();
        let level = first.and_then(|f| self.heading_level(f));
        let index = first.and_then(|f| kids.iter().position(|&k| k == f));
        let opens = match (first, level, index) {
            (Some(f), Some(l), Some(i)) => kids
                .get(..i)
                .unwrap_or(&[])
                .iter()
                .all(|&k| self.dom.is_blank(k))
                .then_some((f, l, i)),
            _ => None,
        };
        let Some((first, level, index)) = opens else {
            for name in ["data-carve-explicit-id", "data-carve-generated-id"] {
                if self.dom.has(id, name) {
                    self.ignored_carrier(id, name, "the section opens with no heading");
                }
            }
            // C2-M-028: a section that does not open with a heading is a div.
            self.report(
                id,
                Code::ElementMapped,
                "HTML section without a heading read as a div",
            );
            let children = self.block_children(id);
            let mapped = self.plain_attrs(id);
            return vec![with("div", children, &mapped)];
        };
        self.section_attributes(id);
        let rest = kids.get(index + 1..).unwrap_or(&[]).to_vec();
        let chosen = self.section_id(id, first);
        let mut out = vec![self.heading(first, level, chosen)];
        out.extend(self.flow_of(&rest, id, reader));
        out
    }

    /// C2-M-026: the id of the section is the heading's, unless the heading
    /// carries an id of its own that differs.
    fn section_id(&mut self, id: Id, first: Id) -> Option<HeadingId> {
        let own = self.dom.attr(first, "id").map(str::to_string);
        let outer = self.dom.attr(id, "id").map(str::to_string);
        let Some(own) = own.filter(|o| !o.is_empty()) else {
            return Some(self.heading_id(id));
        };
        if outer.is_some_and(|o| o != own) {
            self.report(
                id,
                Code::AttributeDropped,
                format!("Attribute id of a section dropped: its heading carries the id {own}"),
            );
        }
        None
    }

    /// C2-H-089: the notes stand at the section; an inline footnote reads back
    /// at its reference.
    fn footnote_section(&mut self, id: Id) -> Vec<Value> {
        let Some(ordered) = self
            .dom
            .elements(id)
            .into_iter()
            .find(|&c| self.dom.is(c, "ol"))
        else {
            self.report(
                id,
                Code::StructureRepaired,
                "Footnote section has no ordered list",
            );
            return Vec::new();
        };
        for item in self.dom.elements(ordered) {
            if !self.dom.is(item, "li") {
                continue;
            }
            let Some(note_id) = self
                .dom
                .attr(item, "id")
                .filter(|i| !i.is_empty())
                .map(str::to_string)
            else {
                self.report(item, Code::StructureRepaired, "Footnote has no id");
                continue;
            };
            let note = self.notes.get(&note_id).cloned();
            if note.as_ref().is_some_and(|n| n.inline) {
                continue;
            }
            let key = note
                .and_then(|n| n.label)
                .unwrap_or_else(|| footnote_key(&note_id));
            // C2-M-061: an item whose key would be empty is dropped.
            if key.is_empty() {
                self.report(item, Code::StructureRepaired, "Footnote key is empty");
                continue;
            }
            let children = self.block_children(item);
            self.footnotes.push(json!({
                "type": "footnote", "key": key, "namespace": "footnote", "children": children,
            }));
        }
        self.notes_directive(id)
    }

    /// C2-H-093, C2-H-158: a top-level `::: footnotes` directive places the section.
    fn notes_directive(&mut self, id: Id) -> Vec<Value> {
        let parts = self.titled(id);
        let behind = self.dom.parent(id).is_some_and(|p| {
            let siblings = self.dom.children(p);
            siblings
                .iter()
                .position(|&s| s == id)
                .is_some_and(|i| siblings.iter().skip(i + 1).any(|&s| !self.dom.is_blank(s)))
        });
        if parts.title.is_none() && parts.label.is_none() && !behind {
            return Vec::new();
        }
        let mut m = node("directive");
        m.insert("kind".into(), "footnotes".into());
        m.insert("children".into(), json!([]));
        if let Some(t) = &parts.title {
            m.insert("title".into(), Value::Array(t.clone()));
        }
        if let Some(l) = &parts.label {
            m.insert("groupLabel".into(), l.clone().into());
        }
        vec![Value::Object(m)]
    }

    /// C2-H-154: `<p class="admonition-title">` and `<p class="div-label">`
    /// lead the body of a div or admonition.
    fn titled(&mut self, id: Id) -> Titled {
        let nodes = self.kids(id);
        let mut index = 0;
        let mut next = |cx: &Cx, name: &str| -> Option<Id> {
            while nodes.get(index).is_some_and(|&n| cx.dom.is_blank(n)) {
                index += 1;
            }
            let candidate = *nodes.get(index)?;
            if !cx.dom.is(candidate, "p") || cx.dom.attr(candidate, "class") != Some(name) {
                return None;
            }
            index += 1;
            Some(candidate)
        };
        let title = next(self, "admonition-title");
        let label = next(self, "div-label");
        let rest = nodes.get(index..).unwrap_or(&[]).to_vec();
        Titled {
            title: title.map(|t| self.inline_children(t)),
            label: label.map(|l| self.dom.plain(l)),
            rest,
        }
    }

    fn admonition(&mut self, id: Id) -> Option<Vec<Value>> {
        let classes = class_list(&self.dom, id);
        let kind = classes.get(1).cloned();
        if classes.first().map(String::as_str) != Some("admonition")
            || !is_kind(ADMONITION_KINDS, kind.as_deref())
        {
            return None;
        }
        let kind = kind.unwrap_or_default();
        let parts = self.titled(id);
        let mapped = self
            .plain_attrs(id)
            .without_classes(&["admonition", &kind])
            .without_keys(&["aria-label", "aria-labelledby"]);
        let children = self.flow_of(&parts.rest, id, Reader::Block);
        let mut m = node("admonition");
        m.insert("kind".into(), kind.into());
        parts.apply(&mut m);
        m.insert("children".into(), Value::Array(children));
        mapped.apply(&mut m);
        Some(vec![Value::Object(m)])
    }

    fn container_element(&mut self, id: Id) -> Option<Vec<Value>> {
        if !self.dom.is(id, "div") {
            return None;
        }
        if let Some(level) = self.heading_level(id) {
            return Some(vec![self.heading(id, level, None)]);
        }
        let fault = self.line_block_fault(id);
        let carried = self.dom.has(id, "data-carve-line-block");
        if carried && fault.is_none() {
            return Some(vec![self.line_block(id)]);
        }
        if let Some(f) = fault
            && carried
        {
            self.ignored_carrier(id, "data-carve-line-block", f);
        }
        if self.dom.has(id, "data-carve-div") && self.dom.attrs(id).len() > 1 {
            self.ignored_carrier(id, "data-carve-div", "the div carries other attributes");
        }
        Some(vec![self.div_element(id)])
    }

    fn line_block_fault(&self, id: Id) -> Option<&'static str> {
        if self.dom.has(id, "data-carve-div") {
            return Some("the element is also a div");
        }
        if !class_list(&self.dom, id).iter().any(|c| c == "line-block") {
            return Some("the element lacks the class line-block");
        }
        let ok = self
            .dom
            .children(id)
            .iter()
            .all(|&c| self.dom.is_blank(c) || self.dom.is(c, "p"));
        (!ok).then_some("the element holds more than paragraphs")
    }

    fn div_element(&mut self, id: Id) -> Value {
        let parts = self.titled(id);
        let mapped = self.attrs(id, &[], true);
        let kind = mapped.attrs.classes.first().cloned();
        let children = self.flow_of(&parts.rest, id, Reader::Block);
        // C2-H-157: a directive that places nothing is a div with its kind as the first class.
        if is_kind(DIRECTIVE_KINDS, kind.as_deref()) {
            let kind = kind.unwrap_or_default();
            let mut m = node("directive");
            m.insert("kind".into(), kind.clone().into());
            parts.apply(&mut m);
            m.insert("children".into(), Value::Array(children));
            mapped.without_classes(&[&kind]).apply(&mut m);
            return Value::Object(m);
        }
        let mut m = node("div");
        parts.apply(&mut m);
        m.insert("children".into(), Value::Array(children));
        mapped.apply(&mut m);
        Value::Object(m)
    }

    /// C2-H-043: one paragraph per stanza, a line per `<br>`, leading spaces as no-break spaces.
    fn line_block(&mut self, id: Id) -> Value {
        let mut lines: Vec<Value> = Vec::new();
        for stanza in self.dom.elements(id) {
            if !self.dom.is(stanza, "p") {
                continue;
            }
            if !lines.is_empty() {
                lines.push(json!({ "type": "line", "children": [] }));
            }
            let mut current = Vec::new();
            for child in self.kids(stanza) {
                if self.dom.is(child, "br") {
                    let children = self.line_inline(&current, stanza);
                    lines.push(parent("line", children));
                    current.clear();
                } else {
                    current.push(child);
                }
            }
            let children = self.line_inline(&current, stanza);
            lines.push(parent("line", children));
        }
        let mut m = node("line_block");
        m.insert("children".into(), Value::Array(lines));
        self.plain_attrs(id)
            .without_classes(&["line-block"])
            .apply(&mut m);
        Value::Object(m)
    }

    fn line_inline(&mut self, nodes: &[Id], host: Id) -> Vec<Value> {
        let mut all = Vec::new();
        for &n in nodes {
            all.extend(self.inline(n));
        }
        let mut children = coalesce(all);
        if children.first().is_some_and(|c| ty(c) == "soft_break") {
            children.remove(0);
        }
        let mut spaces = String::new();
        while children
            .first()
            .is_some_and(|c| ty(c) == "non_breaking_space")
        {
            children.remove(0);
            spaces.push(' ');
        }
        let lead_text = children.first().is_some_and(|c| ty(c) == "text");
        if let Some(lead) = children.first_mut()
            && lead_text
        {
            let value = lead["value"].as_str().unwrap_or("").to_string();
            let rest = value.trim_start_matches('\u{a0}');
            let count = value.chars().count() - rest.chars().count();
            spaces.push_str(&" ".repeat(count));
            let rest = rest.to_string();
            if let Some(m) = lead.as_object_mut() {
                m.insert("value".into(), rest.into());
            }
        }
        if !spaces.is_empty() {
            if let Some(lead) = children.first_mut()
                && lead_text
            {
                let value = format!("{spaces}{}", lead["value"].as_str().unwrap_or(""));
                if let Some(m) = lead.as_object_mut() {
                    m.insert("value".into(), value.into());
                }
            } else {
                children.insert(0, json!({ "type": "text", "value": spaces }));
            }
        }
        self.normalize_breaks(coalesce(children), host)
    }

    /// C2-H-030, C2-H-031, C2-H-037: the marker of a list.
    fn list_marker(&mut self, id: Id) -> Value {
        let carried = self.dom.attr(id, "data-marker").map(str::to_string);
        if !self.dom.is(id, "ol") {
            return match carried.as_deref() {
                None => json!({ "kind": "bullet", "char": "-" }),
                Some(c @ ("-" | "*")) => json!({ "kind": "bullet", "char": c }),
                Some(_) => {
                    self.ignored_carrier(id, "data-marker", "the bullet is not a list marker");
                    json!({ "kind": "bullet", "char": "-" })
                }
            };
        }
        if carried.is_some() {
            self.ignored_carrier(id, "data-marker", "an ordered list has no bullet");
        }
        let style = match self.dom.attr(id, "type") {
            Some(t @ ("a" | "A" | "i" | "I")) => t.to_string(),
            _ => "1".to_string(),
        };
        let least = i64::from(style != "1");
        let start = self.list_start(id, least);
        let delimiter = if self.dom.attr(id, "data-delim") == Some(")") {
            ")"
        } else {
            "."
        };
        let mut m = Map::new();
        m.insert("kind".into(), "ordered".into());
        m.insert("style".into(), style.into());
        m.insert("delimiter".into(), delimiter.into());
        if let Some(s) = start.filter(|&s| s != 1) {
            m.insert("start".into(), s.into());
        }
        Value::Object(m)
    }

    /// C2-M-031, C2-B-045: the `start` of `<ol>` as an HTML integer.
    fn list_start(&mut self, id: Id, least: i64) -> Option<i64> {
        let start = html_integer(self.dom.attr(id, "start"))?;
        if start >= least && start <= MAX_START {
            return Some(start);
        }
        self.report(
            id,
            Code::AttributeDropped,
            format!("Attribute start of ol dropped: {start} lies outside {least} to 999999999"),
        );
        None
    }

    /// C2-H-034: a loose list writes the first paragraph of an item as `<p>`.
    fn has_loose_item(&self, id: Id) -> bool {
        self.dom.elements(id).into_iter().any(|item| {
            self.dom.is(item, "li")
                && self.dom.elements(item).into_iter().any(|child| {
                    if !self.dom.is(child, "p") || !self.dom.attrs(child).is_empty() {
                        return false;
                    }
                    let kids = self.dom.children(item);
                    let at = kids.iter().position(|&k| k == child).unwrap_or(0);
                    kids.get(..at)
                        .unwrap_or(&[])
                        .iter()
                        .all(|&e| self.dom.is_blank(e) || self.dom.is(e, "input"))
                })
        })
    }

    /// C2-H-138, C2-M-033: tight or loose.
    fn list_spacing(&mut self, id: Id, items: &[Value]) -> &'static str {
        let loose = self.has_loose_item(id);
        if self.dom.has(id, "data-carve-tight") {
            if !loose {
                return "tight";
            }
            self.ignored_carrier(
                id,
                "data-carve-tight",
                "an item writes its first paragraph as an element",
            );
            return if items.len() > 1 { "loose" } else { "tight" };
        }
        if items.len() <= 1 {
            return "tight";
        }
        if loose {
            return "loose";
        }
        let many = items.iter().any(|item| {
            item["children"]
                .as_array()
                .is_some_and(|c| c.iter().filter(|b| ty(b) != "list").count() > 1)
        });
        if many { "loose" } else { "tight" }
    }

    /// C2-M-034, C2-M-070: a child of a list that is no item is dropped.
    fn html_list(&mut self, id: Id) -> Vec<Value> {
        for child in self.kids(id) {
            if self.dom.is_blank(child) || self.dom.is_comment(child) || self.dom.is(child, "li") {
                continue;
            }
            self.report(
                child,
                Code::StructureRepaired,
                "Non-item child of an HTML list dropped",
            );
        }
        let mut items = Vec::new();
        for child in self.dom.elements(id) {
            if self.dom.is(child, "li") {
                items.push(self.list_item(child));
            }
        }
        if items.is_empty() {
            self.report(
                id,
                Code::StructureRepaired,
                "HTML list with no item dropped",
            );
            return Vec::new();
        }
        let mapped = self.attrs(id, &["start", "type", "data-delim"], false);
        let tasks = items.iter().any(|i| i.get("task").is_some());
        let marker = self.list_marker(id);
        let spacing = self.list_spacing(id, &items);
        let mut m = node("list");
        m.insert("children".into(), Value::Array(items));
        m.insert("marker".into(), marker);
        m.insert("spacing".into(), spacing.into());
        let mapped = if tasks {
            mapped.without_classes(&["task-list"])
        } else {
            mapped
        };
        mapped.apply(&mut m);
        vec![Value::Object(m)]
    }

    /// C2-M-032: the mark of a task item.
    fn task_mark(&mut self, id: Id, checkbox: Id) -> String {
        let state = self.dom.attr(id, "data-task-state").map(str::to_string);
        if let Some(s) = &state {
            if matches!(s.as_str(), "!" | "-" | "/" | ">" | "?" | "_" | "x") {
                return s.clone();
            }
            self.report(
                id,
                Code::AttributeDropped,
                format!("Unknown task state {s} dropped"),
            );
        }
        if self.dom.attr(checkbox, "checked").is_some() {
            "x".into()
        } else {
            " ".into()
        }
    }

    /// C2-H-032, C2-H-033: the box of a task item and its state.
    fn list_item(&mut self, id: Id) -> Value {
        let boxes: Vec<Id> = self
            .dom
            .elements(id)
            .into_iter()
            .filter(|&c| self.dom.is(c, "input") && self.dom.attr(c, "type") == Some("checkbox"))
            .collect();
        let checkbox = boxes.first().copied();
        let further = boxes.get(1..).unwrap_or(&[]).to_vec();
        // C2-M-032: each box after the first is dropped, also with rawHtml on.
        for &b in &further {
            self.report(
                b,
                Code::ElementDropped,
                "A further checkbox of a task item dropped",
            );
        }
        if let Some(c) = checkbox
            && self
                .dom
                .attrs(c)
                .iter()
                .any(|(n, _)| !matches!(n.as_str(), "type" | "checked" | "disabled" | "aria-label"))
        {
            self.report(
                c,
                Code::AttributeDropped,
                "Checkbox attributes outside task state dropped",
            );
        }
        let nodes: Vec<Id> = self
            .kids(id)
            .into_iter()
            .filter(|&c| Some(c) != checkbox)
            .collect();
        if checkbox.is_some()
            && let Some(&lead) = nodes.first()
            && self.dom.is_text(lead)
            && let Some(rest) = self.dom.text(lead).strip_prefix(' ')
        {
            let rest = rest.to_string();
            self.dom.set_text(lead, rest);
        }
        let nodes: Vec<Id> = nodes.into_iter().filter(|c| !further.contains(c)).collect();
        let mapped = self.attrs(id, &["data-task-state"], false);
        let own = if checkbox.is_some() {
            without_style(mapped.without_classes(&["task"]))
        } else {
            mapped
        };
        let task = checkbox.map(|c| self.task_mark(id, c));
        let children = self.flow_of(&nodes, id, Reader::Block);
        let mut m = node("list_item");
        m.insert("children".into(), Value::Array(children));
        if let Some(t) = task {
            m.insert("task".into(), json!({ "mark": t }));
        }
        own.apply(&mut m);
        Value::Object(m)
    }

    fn figure_caption(&mut self, id: Id) -> Value {
        let children = self.block_children(id);
        let mapped = self.plain_attrs(id);
        with("caption", children, &mapped)
    }

    /// C2-H-199, C2-H-063, C2-H-064, C2-M-042, C2-M-043: a figure, a figure
    /// group and its panels.
    fn figure(&mut self, id: Id, drop: &[&str]) -> Value {
        let classes = class_list(&self.dom, id);
        let group = classes.first().map(String::as_str) == Some("carve-figure-group");
        let caption = self
            .dom
            .elements(id)
            .into_iter()
            .find(|&c| self.dom.is(c, "figcaption"));
        let nodes: Vec<Id> = self
            .kids(id)
            .into_iter()
            .filter(|&c| Some(c) != caption)
            .collect();
        let children = if group {
            let mut out = Vec::new();
            for n in nodes {
                if self.dom.is(n, "figure") {
                    out.push(self.panel(n));
                } else {
                    out.extend(self.block(n));
                }
            }
            out
        } else {
            self.flow_of(&nodes, id, Reader::Block)
        };
        let caption_node = caption.map(|c| self.figure_caption(c));
        let put_caption = |m: &mut Map<String, Value>| {
            if let Some(c) = &caption_node {
                m.insert("caption".into(), c.clone());
            }
        };
        if group {
            let mut m = node("figure_group");
            m.insert("children".into(), Value::Array(children));
            put_caption(&mut m);
            self.plain_attrs(id)
                .without_classes(&["carve-figure-group"])
                .apply(&mut m);
            return Value::Object(m);
        }
        let own = self.plain_attrs(id).without_classes(drop);
        if let [only] = children.as_slice()
            && takes_caption(only, caption.is_some())
        {
            let mut m = only.as_object().cloned().unwrap_or_default();
            put_caption(&mut m);
            let merged = self.merged(id, &own, only);
            m.remove("attrs");
            m.remove("lang");
            m.remove("dir");
            merged.apply(&mut m);
            return Value::Object(m);
        }
        let mut m = node("figure_group");
        m.insert("children".into(), Value::Array(children));
        put_caption(&mut m);
        own.apply(&mut m);
        Value::Object(m)
    }

    /// C2-M-043: the attributes of a figure and of its block, the figure's first.
    fn merged(&mut self, id: Id, figure: &Mapped, block: &Value) -> Mapped {
        let inner = Mapped {
            attrs: Attrs::from_value(block.get("attrs")),
            lang: block["lang"].as_str().map(str::to_string),
            dir: block["dir"].as_str().map(str::to_string),
        };
        if replaces_figure(figure, &inner) {
            self.report(
                id,
                Code::AttributeDropped,
                "A figure attribute that the captioned block replaces dropped",
            );
        }
        let mut attrs = Attrs {
            id: inner.attrs.id.clone().or_else(|| figure.attrs.id.clone()),
            classes: figure.attrs.classes.clone(),
            key_values: Vec::new(),
        };
        attrs.classes.extend(inner.attrs.classes.iter().cloned());
        for (k, v) in figure
            .attrs
            .key_values
            .iter()
            .chain(&inner.attrs.key_values)
        {
            match attrs.key_values.iter_mut().find(|(key, _)| key == k) {
                Some(slot) => slot.1.clone_from(v),
                None => attrs.key_values.push((k.clone(), v.clone())),
            }
        }
        Mapped {
            attrs,
            lang: inner.lang.or_else(|| figure.lang.clone()),
            dir: inner.dir.or_else(|| figure.dir.clone()),
        }
    }

    fn panel(&mut self, id: Id) -> Value {
        let inner = self.dom.elements(id);
        let panel_class =
            class_list(&self.dom, id).first().map(String::as_str) == Some("carve-figure-panel");
        if let [only] = inner.as_slice()
            && panel_class
            && self.dom.is(*only, "table")
            && self.dom.attrs(id).len() == 1
        {
            return self.table(*only);
        }
        let drop: &[&str] = if panel_class {
            &["carve-figure-panel"]
        } else {
            &[]
        };
        self.figure(id, drop)
    }

    fn table_cell(&mut self, id: Id, in_head: bool) -> Value {
        let has_blocks = self.dom.elements(id).into_iter().any(|c| {
            attrs::is_block_element(&self.dom, c)
                || attrs::treatment(&self.dom, c) == Treatment::Div
        });
        let header = self.dom.is(id, "th");
        let mut children = if has_blocks {
            self.block_children(id)
        } else {
            let inline = self.cell_inline(id);
            vec![parent("cell_paragraph", coalesce(inline))]
        };
        // C2-M-038, C2-A-091: a cell whose blocks were all dropped keeps one empty paragraph.
        if children.is_empty() {
            children.push(json!({ "type": "cell_paragraph", "children": [] }));
            self.report(
                id,
                Code::StructureRepaired,
                "A table cell whose blocks were all dropped holds an empty paragraph",
            );
        }
        let mut m = node("table_cell");
        m.insert("header".into(), header.into());
        m.insert("children".into(), Value::Array(children));
        if let Some(c) = html_span(self.dom.attr(id, "colspan")) {
            m.insert("colspan".into(), c.into());
        }
        let raw = self.dom.attr(id, "rowspan").map(str::to_string);
        if html_integer(raw.as_deref()) == Some(0) {
            self.report(id, Code::StructureRepaired, "HTML rowspan=0 gives no span");
        }
        if let Some(r) = html_span(raw.as_deref()) {
            m.insert("rowspan".into(), r.into());
        }
        let styled = self.cell_marks(id, &mut m, in_head);
        let mut omit = vec!["colspan", "rowspan", "scope", "align", "valign"];
        if styled {
            omit.push("style");
        }
        self.attrs(id, &omit, false).apply(&mut m);
        Value::Object(m)
    }

    fn cell_value(&mut self, id: Id, name: &str, set: &[&'static str]) -> Option<&'static str> {
        let raw = self.dom.attr(id, name)?.to_ascii_lowercase();
        if let Some(v) = set.iter().copied().find(|v| *v == raw) {
            return Some(v);
        }
        self.report(
            id,
            Code::AttributeDropped,
            format!("A {name} value outside its set dropped"),
        );
        None
    }

    /// C2-M-040: an axis that a declaration gives drops the attribute of that axis.
    fn cell_axis(
        &mut self,
        id: Id,
        name: &str,
        set: &[&'static str],
        declared: Option<&'static str>,
    ) -> Option<&'static str> {
        let Some(d) = declared else {
            return self.cell_value(id, name, set);
        };
        if self.dom.has(id, name) {
            self.report(
                id,
                Code::AttributeDropped,
                format!("A {name} attribute beside a declaration of its axis dropped"),
            );
        }
        Some(d)
    }

    /// C2-H-056 to C2-H-058, C2-M-040: the scope and the alignments.
    fn cell_marks(&mut self, id: Id, m: &mut Map<String, Value>, in_head: bool) -> bool {
        let scope = self.cell_value(id, "scope", SCOPES);
        let implied = self.dom.is(id, "th") && scope == Some(if in_head { "col" } else { "row" });
        if let Some(s) = scope.filter(|_| !implied) {
            m.insert("scope".into(), s.into());
        }
        let declared = cell_style(self.dom.attr(id, "style"));
        let (da, dv) = declared.unwrap_or((None, None));
        let align = self.cell_axis(id, "align", ALIGNS, da);
        let valign = self.cell_axis(id, "valign", VALIGNS, dv);
        if let Some(a) = align {
            m.insert("align".into(), a.into());
        }
        if let Some(v) = valign {
            m.insert("valign".into(), v.into());
        }
        declared.is_some()
    }

    fn cell_inline(&mut self, id: Id) -> Vec<Value> {
        let mut children = self.inline_children(id);
        let changed = flatten_breaks(&mut children);
        if changed {
            self.report(
                id,
                Code::WhitespaceNormalized,
                "HTML table cell line break read as a space",
            );
        }
        coalesce(children)
    }

    fn group_rows(&mut self, id: Id) -> Vec<Id> {
        if !self.dom.is(id, "table") {
            for child in self.dom.elements(id) {
                if !self.dom.is(child, "tr") {
                    self.report(
                        child,
                        Code::StructureRepaired,
                        "Non-row child in table group",
                    );
                }
            }
        }
        self.dom
            .elements(id)
            .into_iter()
            .filter(|&c| self.dom.is(c, "tr"))
            .collect()
    }

    fn rows(&mut self, id: Id, in_head: bool) -> Vec<Value> {
        let group_rows = self.group_rows(id);
        self.rows_of(&group_rows, in_head)
    }

    fn rows_of(&mut self, group_rows: &[Id], in_head: bool) -> Vec<Value> {
        let mut out = Vec::new();
        for &r in group_rows {
            let mut cells = Vec::new();
            for entry in self.dom.elements(r) {
                if self.dom.is(entry, "th") || self.dom.is(entry, "td") {
                    cells.push(self.table_cell(entry, in_head));
                } else {
                    self.report(entry, Code::StructureRepaired, "Non-cell child in HTML row");
                }
            }
            let mapped = self.plain_attrs(r);
            out.push(with("table_row", cells, &mapped));
        }
        self.clamp_rowspans(&mut out, group_rows);
        out
    }

    /// C2-M-037: a row heads the columns when it has cells and each is a `<th>` with no `scope` or with `scope` `col`.
    fn heads_columns(&self, tr: Id) -> bool {
        let mut cells = self
            .dom
            .elements(tr)
            .into_iter()
            .filter(|&c| self.dom.is(c, "th") || self.dom.is(c, "td"))
            .peekable();
        cells.peek().is_some()
            && cells.all(|c| {
                self.dom.is(c, "th")
                    && self
                        .dom
                        .attr(c, "scope")
                        .is_none_or(|s| s.eq_ignore_ascii_case("col"))
            })
    }

    /// C2-A-093, C2-B-174: a row span ends at the last row of its group.
    fn clamp_rowspans(&mut self, rows: &mut [Value], group_rows: &[Id]) {
        let count = rows.len();
        for (index, row) in rows.iter_mut().enumerate() {
            let room = count - index;
            for cell in row
                .get_mut("children")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let Some(span) = cell["rowspan"].as_u64() else {
                    continue;
                };
                if usize::try_from(span).unwrap_or(usize::MAX) <= room {
                    continue;
                }
                if let Some(&owner) = group_rows.get(index) {
                    self.report(
                        owner,
                        Code::StructureRepaired,
                        "HTML rowspan past the end of its row group clamped",
                    );
                }
                if let Some(m) = cell.as_object_mut() {
                    if room > 1 {
                        m.insert("rowspan".into(), room.into());
                    } else {
                        m.remove("rowspan");
                    }
                }
            }
        }
    }

    /// C2-M-037, C2-A-086: the first `<thead>` first and the first `<tfoot>` last. With no `<thead>`,
    /// the leading column-heading rows of the first body are the head, and a body they leave empty is dropped.
    fn table_groups(&mut self, id: Id) -> Vec<Value> {
        let groups: Vec<Id> = self
            .dom
            .elements(id)
            .into_iter()
            .filter(|&c| matches!(self.dom.tag(c), Some("thead" | "tbody" | "tfoot")))
            .collect();
        let head = groups.iter().copied().find(|&g| self.dom.is(g, "thead"));
        let foot = groups.iter().copied().find(|&g| self.dom.is(g, "tfoot"));
        let bodies: Vec<Id> = groups
            .iter()
            .copied()
            .filter(|&g| Some(g) != head && Some(g) != foot)
            .collect();
        let direct_rows = self.group_rows(id);
        let first_body = if direct_rows.is_empty() {
            bodies.first().copied()
        } else {
            Some(id)
        };
        let mut head_rows = Vec::new();
        let direct = if direct_rows.is_empty() {
            Vec::new()
        } else {
            self.body_rows(&direct_rows, head.is_none(), &mut head_rows)
        };
        let mut children = Vec::new();
        for &g in &bodies {
            if !self.dom.is(g, "tbody") {
                let tag = self.dom.tag_name(g);
                self.report(
                    g,
                    Code::StructureRepaired,
                    format!("A further {tag} read as a body"),
                );
            }
            let all = self.group_rows(g);
            let rows = self.body_rows(
                &all,
                head.is_none() && Some(g) == first_body,
                &mut head_rows,
            );
            let emptied = rows.is_empty() && Some(g) == first_body && !head_rows.is_empty();
            let mapped = self.plain_attrs(g);
            let body = with("table_body", rows, &mapped);
            if !emptied {
                children.push(body);
            } else if body.as_object().is_some_and(|m| m.len() > 2) {
                self.report(
                    g,
                    Code::AttributeDropped,
                    "The attributes of a body its head rows left empty dropped",
                );
            }
        }
        if !direct.is_empty() {
            children.insert(0, parent("table_body", direct));
        }
        if let Some(h) = head {
            let rows = self.head_cells(h);
            let mapped = self.plain_attrs(h);
            children.insert(0, with("table_head", rows, &mapped));
        } else if !head_rows.is_empty() {
            children.insert(0, parent("table_head", head_rows));
        }
        if let Some(f) = foot {
            let rows = self.rows(f, false);
            let mapped = self.plain_attrs(f);
            children.push(with("table_foot", rows, &mapped));
        }
        children
    }

    /// carve#6663: a head holds header cells only (C2-B-178), so each `<td>` of the `<thead>` is one, reported once.
    fn head_cells(&mut self, head: Id) -> Vec<Value> {
        let mut rows = self.rows(head, true);
        let mut changed = false;
        for cell in rows
            .iter_mut()
            .filter_map(|r| r["children"].as_array_mut())
            .flatten()
        {
            if cell["header"] == false {
                cell["header"] = true.into();
                changed = true;
            }
        }
        if changed {
            self.report(
                head,
                Code::StructureRepaired,
                "The data cells of a table head read as header cells",
            );
        }
        rows
    }

    /// The rows of a body; `lead` splits its leading column-heading rows off into `head_rows`.
    fn body_rows(&mut self, all: &[Id], lead: bool, head_rows: &mut Vec<Value>) -> Vec<Value> {
        if !lead {
            return self.rows_of(all, false);
        }
        let split = all.iter().take_while(|&&r| self.heads_columns(r)).count();
        let (lead_rows, rest) = all.split_at(split);
        *head_rows = self.rows_of(lead_rows, true);
        self.rows_of(rest, false)
    }

    /// C2-H-152: the widths of a column group, a fraction each.
    fn column_widths(&mut self, id: Id) -> Vec<Option<f64>> {
        let Some(group) = self
            .dom
            .elements(id)
            .into_iter()
            .find(|&c| self.dom.is(c, "colgroup"))
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for col in self.dom.elements(group) {
            if !self.dom.is(col, "col") {
                continue;
            }
            let style = self.dom.attr(col, "style").map(str::to_string);
            let (width, only) = style.as_deref().map_or((None, true), col_style_width);
            // C2-M-041: a col style is no key; one that gives no width or holds
            // another declaration is reported.
            if style.is_some() && (width.is_none() || !only) {
                self.report(
                    col,
                    Code::AttributeDropped,
                    "A col style that gives no width or holds another declaration dropped",
                );
            }
            out.push(width);
        }
        out
    }

    fn table_columns(&mut self, children: &[Value], id: Id) -> Vec<Value> {
        let width = children
            .iter()
            .filter(|g| ty(g) != "comment")
            .flat_map(|g| g["children"].as_array().into_iter().flatten())
            .filter(|r| ty(r) == "table_row")
            .map(|r| {
                r["children"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| c["colspan"].as_u64().unwrap_or(1))
                    .sum::<u64>()
            })
            .max()
            .unwrap_or(0);
        let widths = self.column_widths(id);
        (0..usize::try_from(width).unwrap_or(0))
            .map(|i| match widths.get(i).copied().flatten() {
                Some(w) => json!({ "type": "table_column", "width": w }),
                None => json!({ "type": "table_column" }),
            })
            .collect()
    }

    fn table(&mut self, id: Id) -> Value {
        let children = self.table_groups(id);
        let captions: Vec<Id> = self
            .dom
            .elements(id)
            .into_iter()
            .filter(|&c| self.dom.is(c, "caption"))
            .collect();
        // C2-M-041: each caption after the first is dropped with its content.
        for &extra in captions.iter().skip(1) {
            self.report(
                extra,
                Code::ElementDropped,
                "A second caption of a table dropped",
            );
        }
        for child in self.dom.elements(id) {
            let tag = self.dom.tag_name(child);
            if !matches!(
                &*tag,
                "caption" | "thead" | "tbody" | "tfoot" | "tr" | "colgroup"
            ) {
                self.report(
                    child,
                    Code::StructureRepaired,
                    format!("Unsupported table child {tag}"),
                );
            }
        }
        let caption = captions.first().map(|&c| self.figure_caption(c));
        let columns = self.table_columns(&children, id);
        let mut m = node("table");
        m.insert("children".into(), Value::Array(children));
        m.insert("columns".into(), Value::Array(columns));
        if let Some(c) = caption {
            m.insert("caption".into(), c);
        }
        self.plain_attrs(id).apply(&mut m);
        let mut v = Value::Object(m);
        super::align::resolve(&mut v);
        v
    }

    fn definition_entries(&mut self, nodes: &[Id], in_group: bool) -> Vec<Value> {
        let mut out = Vec::new();
        for &child in nodes {
            match self.dom.tag(child) {
                Some("dt") => {
                    let children = self.inline_children(child);
                    let mapped = self.plain_attrs(child);
                    let term = with("definition_term", children, &mapped);
                    out.extend(self.with_content(term, child));
                }
                Some("dd") => {
                    let children = self.block_children(child);
                    let mapped = self.plain_attrs(child);
                    out.push(with("definition_description", children, &mapped));
                }
                // C2-M-036: the HTML Standard allows a div around a group.
                Some("div") if !in_group => {
                    let kids = self.kids(child);
                    out.extend(self.definition_entries(&kids, true));
                }
                _ => {
                    if !self.dom.is_blank(child) && !self.dom.is_comment(child) {
                        self.report(
                            child,
                            Code::StructureRepaired,
                            "Non-entry child of a description list dropped",
                        );
                    }
                }
            }
        }
        out
    }

    /// C2-M-036, C2-M-070: a description list with no entry is dropped.
    fn definition_list(&mut self, id: Id) -> Vec<Value> {
        let kids = self.kids(id);
        let children = self.definition_entries(&kids, false);
        if children.is_empty() {
            self.report(
                id,
                Code::StructureRepaired,
                "Description list with no entry dropped",
            );
            return Vec::new();
        }
        let mapped = self.plain_attrs(id);
        vec![with("definition_list", children, &mapped)]
    }

    fn fallback_block(&mut self, id: Id) -> Vec<Value> {
        if self.dom.is(id, "img") {
            // A denied `src` leaves the alt text or nothing, no image (C2-M-069).
            let children = self.html_image(id);
            return match children.as_slice() {
                [] => Vec::new(),
                [only] if ty(only) == "image" => {
                    vec![with_role("paragraph", "image", children)]
                }
                _ => vec![run_paragraph(children)],
            };
        }
        let children = self.inline(id);
        let children = self.trim_inline_edges(children, id);
        if children.is_empty() {
            Vec::new()
        } else {
            vec![paragraph(children)]
        }
    }
}

fn without_style(mut mapped: Mapped) -> Mapped {
    let kv = &mut mapped.attrs.key_values;
    if let Some(i) = kv.iter().position(|(k, _)| k == "style") {
        let value = kv.get(i).map(|(_, v)| v.clone()).unwrap_or_default();
        let rest = if value == "list-style: none" {
            None
        } else if let Some(r) = value.strip_prefix("list-style: none; ") {
            Some(r.to_string())
        } else {
            Some(value)
        };
        match rest {
            None => {
                kv.remove(i);
            }
            Some(r) => {
                if let Some(slot) = kv.get_mut(i) {
                    slot.1 = r;
                }
            }
        }
    }
    mapped
}

fn replaces_figure(figure: &Mapped, inner: &Mapped) -> bool {
    if figure.lang.is_some() && inner.lang.is_some() {
        return true;
    }
    if figure.dir.is_some() && inner.dir.is_some() {
        return true;
    }
    if figure.attrs.id.is_some() && inner.attrs.id.is_some() {
        return true;
    }
    figure
        .attrs
        .key_values
        .iter()
        .any(|(k, _)| inner.attrs.key_values.iter().any(|(n, _)| n == k))
}

/// C2-M-043, C2-A-082: the blocks with a caption slot.
fn takes_caption(block: &Value, captioned: bool) -> bool {
    let kind = ty(block);
    let slot = matches!(kind, "code_block" | "block_quote" | "table")
        || (kind == "paragraph" && block.get("role").is_some());
    slot && (!captioned || block.get("caption").is_none())
}

/// C2-M-040: a `style` exactly as a render writes it (C2-H-057).
fn cell_style(style: Option<&str>) -> Option<(Option<&'static str>, Option<&'static str>)> {
    let style = style?;
    if let Some(a) = declaration(style, "text-align: ", ALIGNS) {
        return Some((Some(a), None));
    }
    if let Some(v) = declaration(style, "vertical-align: ", VALIGNS) {
        return Some((None, Some(v)));
    }
    let gap = style.find("; ")?;
    let first = declaration(style.get(..=gap)?, "text-align: ", ALIGNS)?;
    let second = declaration(style.get(gap + 2..)?, "vertical-align: ", VALIGNS)?;
    Some((Some(first), Some(second)))
}

fn declaration(text: &str, property: &str, set: &[&'static str]) -> Option<&'static str> {
    let value = text.strip_prefix(property)?.strip_suffix(';')?;
    set.iter().copied().find(|v| *v == value)
}

/// C2-M-041, C2-B-212: `width: N%;` with N above 0 and at most 100.
/// C2-M-041: the width of the last `width` declaration with a valid `N%`, and
/// whether the style holds that one declaration and no other.
fn col_style_width(style: &str) -> (Option<f64>, bool) {
    fn trim(s: &str) -> &str {
        s.trim_matches(|c: char| c.is_ascii_whitespace())
    }
    let mut width = None;
    let mut count = 0usize;
    for piece in style.split(';').map(trim).filter(|p| !p.is_empty()) {
        count += 1;
        let Some((name, value)) = piece.split_once(':') else {
            continue;
        };
        if trim(name).eq_ignore_ascii_case("width")
            && let Some(w) = percent_width(trim(value))
        {
            width = Some(w);
        }
    }
    (width, count == 1)
}

fn percent_width(value: &str) -> Option<f64> {
    let text = value.strip_suffix('%')?;
    let (int, frac) = match text.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (text, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(int) || frac.is_some_and(|f| !digits(f)) {
        return None;
    }
    let value: f64 = text.parse().ok()?;
    (value > 0.0 && value <= 100.0).then_some(value / 100.0)
}

/// C2-M-038: breaks in a cell become spaces, through every inline container.
fn flatten_breaks(children: &mut [Value]) -> bool {
    let mut changed = false;
    let mut pending: Vec<&mut Value> = children.iter_mut().collect();
    while let Some(child) = pending.pop() {
        match ty(child) {
            "soft_break" | "hard_break" => {
                *child = json!({ "type": "text", "value": " " });
                changed = true;
            }
            "ruby" => {
                for pair in child
                    .get_mut("pairs")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    if let Some(m) = pair.as_object_mut() {
                        for (k, v) in m.iter_mut() {
                            if k == "base" || k == "annotation" {
                                pending.extend(v.as_array_mut().into_iter().flatten());
                            }
                        }
                    }
                }
            }
            "citation_group" => {}
            _ => {
                if let Some(list) = child.get_mut("children").and_then(Value::as_array_mut) {
                    pending.extend(list.iter_mut());
                }
            }
        }
    }
    changed
}

/// The `href` of the first link of each key in `keys`, as resolution gave it.
fn resolved_hrefs(
    root: &Value,
    keys: &std::collections::HashSet<String>,
) -> Vec<(String, Option<String>)> {
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    let mut pending = vec![root];
    while let Some(v) = pending.pop() {
        match v {
            Value::Array(items) => pending.extend(items.iter().rev()),
            Value::Object(m) => {
                if m.get("type").and_then(Value::as_str) == Some("link")
                    && let Some(key) = m.get("key").and_then(Value::as_str)
                    && keys.contains(key)
                    && !found.iter().any(|(k, _)| k == key)
                {
                    found.push((
                        key.to_string(),
                        m.get("href").and_then(Value::as_str).map(str::to_string),
                    ));
                }
                let mut values: Vec<&Value> = m
                    .values()
                    .filter(|v| v.is_object() || v.is_array())
                    .collect();
                values.reverse();
                pending.extend(values);
            }
            _ => {}
        }
    }
    found
}

/// C2-H-198: no definition is rebuilt for a key whose first `<a>` links `#ID`
/// where the heading lookup of the key finds the heading with that id.
pub(crate) fn rebuilt_definitions(cx: &Cx, children: &[Value]) -> Vec<Value> {
    let all: Vec<&(String, Value)> = cx.definitions.iter().collect();
    if cx.anchor_keys.is_empty() {
        return all.into_iter().map(|(_, d)| d.clone()).collect();
    }
    let mut probe_children: Vec<Value> = children.to_vec();
    probe_children.extend(
        all.iter()
            .filter(|(k, _)| !cx.anchor_keys.contains(k))
            .map(|(_, d)| d.clone()),
    );
    probe_children.extend(cx.footnotes.iter().cloned());
    let probe = super::document_of(cx, probe_children);
    let found = resolve_probe(&probe)
        .map(|resolved| {
            resolved_hrefs(
                resolved.get("children").unwrap_or(&Value::Null),
                &cx.anchor_keys,
            )
        })
        .unwrap_or_default();
    all.into_iter()
        .filter(|(k, d)| {
            !cx.anchor_keys.contains(k)
                || found
                    .iter()
                    .find(|(f, _)| f == k)
                    .map(|(_, h)| h.as_deref())
                    != Some(d["href"].as_str())
        })
        .map(|(_, d)| d.clone())
        .collect()
}

fn resolve_probe(probe: &Value) -> Option<Value> {
    let read = crate::reader::read_tree_value(probe.clone()).ok()?;
    let arena = crate::Arena::new();
    let mut doc = read.document().in_arena(&arena);
    crate::resolve::clear_results(&mut doc);
    crate::resolve::resolve(&mut doc, &crate::Options::default(), &arena);
    serde_json::from_str(&crate::to_json(&doc)).ok()
}

pub(crate) fn flow(cx: &mut Cx, nodes: &[Id], host: Id, reader: Reader) -> Vec<Value> {
    cx.flow_of(nodes, host, reader)
}
