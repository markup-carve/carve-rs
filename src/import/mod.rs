//! The HTML importer of `import-html.md`: HTML text to a 0.2 tree and the
//! record of what the tree does not hold (C2-M-001, C2-M-080).

mod adapter;
mod align;
mod attrs;
mod blocks;
pub(crate) mod dom;
mod inline;
mod notes;
pub(crate) mod parse;
mod template;

use crate::api::{Entry, Range};
use dom::{Dom, Id};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

/// C2-A-171: the deepest foreign tree read; a list and its item take two
/// levels per block level, and inline content nests below.
const MAX_FOREIGN_DEPTH: usize = 2 * 200 + 200 + 16;
/// C2-B-190, C2-I-118: the block and inline nesting a reader accepts.
const MAX_NESTING: usize = 200;

/// Why an import gives no tree (C2-M-006).
#[derive(Debug)]
pub(crate) struct Refused(pub String);

/// The tree as JSON and the importer's entries, in document order.
pub(crate) struct Imported {
    pub tree: Value,
    pub diagnostics: Vec<Entry>,
}

pub(crate) struct Cx {
    dom: Dom,
    raw_html: bool,
    diagnostics: Vec<Entry>,
    root_lang: Option<String>,
    root_dir: Option<String>,
    notes: HashMap<String, notes::Note>,
    footnotes: Vec<Value>,
    abbreviations: HashMap<String, Value>,
    definitions: Vec<(String, Value)>,
    anchor_keys: HashSet<String>,
    templates: HashMap<Id, Vec<Value>>,
}

/// The codes of C2-M-083.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    AttributeDropped,
    CarrierIgnored,
    DestinationDenied,
    ElementDropped,
    ElementMapped,
    StructureRepaired,
    WhitespaceNormalized,
}

impl Code {
    const fn name(self) -> &'static str {
        match self {
            Self::AttributeDropped => "import-attribute-dropped",
            Self::CarrierIgnored => "import-carrier-ignored",
            Self::DestinationDenied => "import-destination-denied",
            Self::ElementDropped => "import-element-dropped",
            Self::ElementMapped => "import-element-mapped",
            Self::StructureRepaired => "import-structure-repaired",
            Self::WhitespaceNormalized => "import-whitespace-normalized",
        }
    }
}

impl Cx {
    fn new(dom: Dom, raw_html: bool) -> Self {
        Self {
            dom,
            raw_html,
            diagnostics: Vec::new(),
            root_lang: None,
            root_dir: None,
            notes: HashMap::new(),
            footnotes: Vec::new(),
            abbreviations: HashMap::new(),
            definitions: Vec::new(),
            anchor_keys: HashSet::new(),
            templates: HashMap::new(),
        }
    }

    /// C2-M-081: the lower-case tag name, or `#comment`, `#text` or `#doctype`.
    fn foreign_type(&self, id: Id) -> String {
        match &self.dom.node(id).kind {
            dom::Kind::Element { name, .. } => str::to_ascii_lowercase(name),
            dom::Kind::Text(_) => "#text".into(),
            dom::Kind::Comment(_) => "#comment".into(),
            _ => "#doctype".into(),
        }
    }

    pub(crate) fn report(&mut self, id: Id, code: Code, message: impl Into<String>) {
        let span = self.dom.located(id);
        let foreign = self.foreign_type(id);
        self.finding(code, foreign, message.into(), span);
    }

    fn finding(
        &mut self,
        code: Code,
        foreign: String,
        message: String,
        span: Option<(usize, usize)>,
    ) {
        // C2-M-020: whitespace normalization changes no visible text.
        let severity = if code == Code::WhitespaceNormalized {
            "hint"
        } else {
            "warning"
        };
        self.diagnostics.push(Entry {
            code: code.name().to_string(),
            severity: severity.to_string(),
            channel: "render-loss".to_string(),
            message,
            path: None,
            range: span.map(|(s, e)| Range {
                start_offset: s,
                end_offset: e,
            }),
            file: None,
            foreign_type: Some(foreign),
        });
    }

    /// C2-H-114: a carrier the HTML around it contradicts is not read (C2-M-009).
    pub(crate) fn ignored_carrier(&mut self, id: Id, name: &str, reason: &str) {
        self.report(
            id,
            Code::CarrierIgnored,
            format!("Carrier {name} ignored: {reason}"),
        );
    }

    fn kids(&self, id: Id) -> Vec<Id> {
        self.dom.children(id).to_vec()
    }
}

/// C2-M-002: a document starts, after leading ASCII whitespace and comments,
/// with `<!doctype` or `<html`.
fn is_full_document(input: &str) -> bool {
    let mut at = 0;
    let bytes = input.as_bytes();
    while let Some(&b) = bytes.get(at) {
        if matches!(b, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r') {
            at += 1;
        } else if input.get(at..).is_some_and(|r| r.starts_with("<!--")) {
            let Some(end) = input.get(at + 4..).and_then(|r| r.find("-->")) else {
                return false;
            };
            at += 4 + end + 3;
        } else {
            break;
        }
    }
    let head: String = input
        .get(at..)
        .unwrap_or("")
        .chars()
        .take(9)
        .collect::<String>()
        .to_ascii_lowercase();
    head == "<!doctype" || head.starts_with("<html")
}

fn nesting_exceeded(root: &Value) -> bool {
    let mut pending = vec![(root, 0usize, 0usize)];
    while let Some((value, blocks, inlines)) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(
                items
                    .iter()
                    .filter(|v| v.is_array() || v.is_object())
                    .map(|v| (v, blocks, inlines)),
            ),
            Value::Object(map) => {
                let kind = crate::view::json_field(value, "type")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let blocks = blocks + usize::from(attrs::is_nesting_block(kind));
                let inlines = inlines + usize::from(attrs::is_nesting_inline(kind));
                if blocks > MAX_NESTING || inlines > MAX_NESTING {
                    return true;
                }
                pending.extend(
                    map.values()
                        .filter(|v| v.is_array() || v.is_object())
                        .map(|v| (v, blocks, inlines)),
                );
            }
            _ => {}
        }
    }
    false
}

/// `importDocument` for HTML (C2-M-001): the tree as JSON and the record.
#[cfg(test)]
pub(crate) fn import_html(
    input: &str,
    raw_html: bool,
    note_pass: bool,
) -> Result<Imported, Refused> {
    build(input, parse_html(input)?, raw_html, note_pass)
}

/// The parsed input of [`build`]. Parsing does not recurse, so it runs on any stack.
pub(crate) struct Parsed {
    dom: Dom,
    full: bool,
    /// The deepest node of the parsed tree, which bounds the recursion of [`build`].
    pub depth: usize,
}

/// The parse half of [`import_html`].
pub(crate) fn parse_html(input: &str) -> Result<Parsed, Refused> {
    let full = is_full_document(input);
    let dom = if full {
        parse::document(input)
    } else {
        parse::fragment(input)
    };
    let depth = dom.depth(dom.root);
    if depth > MAX_FOREIGN_DEPTH {
        return Err(Refused(format!(
            "Foreign input exceeds the engine tree depth limit of {MAX_FOREIGN_DEPTH}"
        )));
    }
    Ok(Parsed { dom, full, depth })
}

/// The tree half of [`import_html`], recursing once per level of `parsed`.
pub(crate) fn build(
    input: &str,
    parsed: Parsed,
    raw_html: bool,
    note_pass: bool,
) -> Result<Imported, Refused> {
    let Parsed { mut dom, full, .. } = parsed;
    if note_pass {
        adapter::note_pass(&mut dom);
    }
    let mut cx = Cx::new(dom, raw_html);
    if !full {
        fragment_body(input, &mut cx);
    }
    let top = cx.kids(cx.dom.root);
    notes::collect(&mut cx, &top);
    root_language(&mut cx, &top);
    for &child in &top {
        audit(&mut cx, child);
    }
    template::collect(&mut cx, &top);
    let children = match top.first() {
        Some(&first) => blocks::flow(&mut cx, &top, first, blocks::Reader::DocumentChild),
        None => Vec::new(),
    };
    let definitions = blocks::rebuilt_definitions(&cx, &children);
    let mut all = children;
    all.extend(definitions);
    all.append(&mut cx.footnotes);
    let tree = document_of(&cx, all);
    if nesting_exceeded(&tree) {
        return Err(Refused(format!(
            "Foreign input exceeds the nesting limit of {MAX_NESTING} levels"
        )));
    }
    Ok(Imported {
        tree,
        diagnostics: cx.diagnostics,
    })
}

fn document_of(cx: &Cx, children: Vec<Value>) -> Value {
    let mut doc = Map::new();
    doc.insert("type".into(), "document".into());
    doc.insert("astVersion".into(), "2.0".into());
    doc.insert("children".into(), Value::Array(children));
    if let Some(lang) = &cx.root_lang {
        doc.insert("lang".into(), lang.clone().into());
    }
    if let Some(dir) = &cx.root_dir {
        doc.insert("dir".into(), dir.clone().into());
    }
    doc.insert(
        "extensions".into(),
        json!([
            { "name": "carve.source", "version": "1", "required": false, "slots": [] },
            { "name": "carve.style", "version": "1", "required": false, "slots": [] },
        ]),
    );
    doc.insert(
        "data".into(),
        json!({ "carve.source": {
            "format": "html",
            "locator": { "part": "input" },
            "importer": { "name": "carve-import", "version": "0.0.0" },
        }}),
    );
    Value::Object(doc)
}

/// C2-M-004: a fragment that holds a `<body>` tag gives its `lang` and `dir`.
fn fragment_body(input: &str, cx: &mut Cx) {
    let lower = input.to_ascii_lowercase();
    let has_body = lower.match_indices("<body").any(|(i, _)| {
        lower
            .as_bytes()
            .get(i + 5)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
    });
    if !has_body {
        return;
    }
    let doc = parse::document(input);
    let Some(html) = doc
        .children(doc.root)
        .iter()
        .copied()
        .find(|&n| doc.is(n, "html"))
    else {
        return;
    };
    let Some(body) = doc.elements(html).into_iter().find(|&n| doc.is(n, "body")) else {
        return;
    };
    let Some(span) = doc.node(body).span else {
        return;
    };
    let names: Vec<(String, String)> = doc.attrs(body).to_vec();
    for (name, value) in &names {
        if name != "lang" && name != "dir" && attrs::dropped_attribute(name, value).is_none() {
            cx.finding(
                Code::AttributeDropped,
                "body".into(),
                format!("Attribute {name} of body dropped"),
                Some(span),
            );
        }
    }
    if let Some(lang) = doc.attr(body, "lang").filter(|l| !l.is_empty()) {
        cx.root_lang = Some(lang.to_string());
    }
    if let Some(dir) = attrs::direction_of(doc.attr(body, "dir")) {
        cx.root_dir = Some(dir.to_string());
    }
}

fn root_language(cx: &mut Cx, top: &[Id]) {
    let Some(html) = top.iter().copied().find(|&n| cx.dom.is(n, "html")) else {
        return;
    };
    let body = cx
        .dom
        .elements(html)
        .into_iter()
        .find(|&n| cx.dom.is(n, "body"));
    let pick = |name: &str| {
        body.and_then(|b| cx.dom.attr(b, name))
            .or_else(|| cx.dom.attr(html, name))
            .map(str::to_string)
    };
    let lang = pick("lang");
    let dir = pick("dir");
    if let Some(lang) = lang.filter(|l| !l.is_empty()) {
        cx.root_lang = Some(lang);
    }
    if let Some(dir) = attrs::direction_of(dir.as_deref()) {
        cx.root_dir = Some(dir.to_string());
    }
}

/// The elements that never reach the tree as elements, with their content.
fn silent(cx: &Cx, id: Id) -> bool {
    let kind = attrs::treatment(&cx.dom, id);
    kind == attrs::Treatment::Drop
        || kind == attrs::Treatment::Raw
        || attrs::is_raw_span(&cx.dom, id)
        || cx.dom.is(id, "input")
}

/// C2-M-052, C2-M-054: every attribute the tree cannot keep, once; markup
/// inside code and math is dropped unread.
fn audit(cx: &mut Cx, id: Id) {
    let mut pending = vec![id];
    while let Some(n) = pending.pop() {
        if !cx.dom.is_element(n) || silent(cx, n) {
            continue;
        }
        let list = cx.dom.attrs(n).to_vec();
        for (name, value) in &list {
            if let Some(reason) = attrs::dropped_attribute(name, value) {
                cx.report(
                    n,
                    Code::AttributeDropped,
                    format!("Attribute {name} dropped: {reason}"),
                );
            }
        }
        if matches!(cx.dom.tag(n), Some("math" | "pre" | "code")) {
            continue;
        }
        pending.extend(cx.dom.children(n).iter().rev());
    }
}
