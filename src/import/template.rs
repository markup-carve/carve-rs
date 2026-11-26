//! C2-H-114: the abbreviation template of a `roundtrip` render holds the Carve
//! source of a run of definitions.

use super::Cx;
use super::dom::Id;
use super::notes::is_footnote_section;
use serde_json::Value;

fn without_pos(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("segments");
            m.values_mut().for_each(without_pos);
        }
        Value::Array(a) => a.iter_mut().for_each(without_pos),
        _ => {}
    }
}

/// The definitions of `ty` a template text holds, or `None` where it holds anything else.
fn definitions(text: &str, ty: &str) -> Option<Vec<Value>> {
    let tree: Value =
        serde_json::from_str(&crate::tree_json(text, &crate::Options::default())).ok()?;
    let mut out = Vec::new();
    for child in tree
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if child["type"] != ty {
            return None;
        }
        let mut child = child.clone();
        without_pos(&mut child);
        out.push(child);
    }
    (!out.is_empty()).then_some(out)
}

/// The carrier a template holds: the abbreviation or, C2-H-216, the snippet template.
fn carrier_of(cx: &Cx, id: Id) -> Option<&'static str> {
    if cx.dom.has(id, "data-carve-abbreviations") {
        Some("data-carve-abbreviations")
    } else if cx.dom.has(id, "data-carve-snippets") {
        Some("data-carve-snippets")
    } else {
        None
    }
}

fn read(cx: &mut Cx, id: Id, carrier: &str) -> Option<Vec<Value>> {
    let nodes: Vec<Id> = cx
        .dom
        .node(id)
        .content
        .map(|c| cx.dom.children(c).to_vec())
        .unwrap_or_default();
    if !nodes.iter().all(|&n| cx.dom.is_text(n)) {
        cx.ignored_carrier(id, carrier, "the template holds markup");
        return None;
    }
    let text: String = nodes.iter().map(|&n| cx.dom.text(n)).collect();
    let ty = if carrier == "data-carve-snippets" {
        "snippet_definition"
    } else {
        "abbreviation_def"
    };
    let found = definitions(&text, ty);
    if found.is_none() {
        cx.ignored_carrier(id, carrier, "the template holds more than definitions");
    } else if carrier == "data-carve-snippets" {
        cx.snippet_scope(id);
    }
    found
}

/// Reads every template that stands where a render writes one: among the
/// children of the document, or of a section it opened at a heading.
pub(crate) fn collect(cx: &mut Cx, nodes: &[Id]) {
    let mut pending: Vec<Id> = nodes.iter().rev().copied().collect();
    while let Some(n) = pending.pop() {
        if !cx.dom.is_element(n) {
            continue;
        }
        let carrier = cx
            .dom
            .is(n, "template")
            .then(|| carrier_of(cx, n))
            .flatten();
        if let Some(carrier) = carrier {
            let found = read(cx, n, carrier).unwrap_or_default();
            for definition in found.iter().filter(|d| d["type"] == "abbreviation_def") {
                let key = definition["key"].as_str().unwrap_or("").to_string();
                cx.abbreviations
                    .entry(key)
                    .or_insert_with(|| definition.clone());
            }
            cx.templates.insert(n, found);
        } else if matches!(cx.dom.tag(n), Some("section" | "html" | "body"))
            && !is_footnote_section(&cx.dom, n)
        {
            pending.extend(cx.dom.children(n).iter().rev());
        }
    }
}

impl Cx {
    /// The definitions at the place of the template, or `None` for any other.
    pub(crate) fn abbreviation_template(&mut self, id: Id) -> Option<Vec<Value>> {
        if let Some(found) = self.templates.get(&id) {
            return Some(found.clone());
        }
        if let Some(carrier) = carrier_of(self, id) {
            self.ignored_carrier(
                id,
                carrier,
                "a render writes a template only in the flow of the document",
            );
        }
        None
    }
}
