//! C2-H-114: the abbreviation template of a `roundtrip` render holds the Carve
//! source of a run of definitions.

use super::Cx;
use super::dom::Id;
use super::notes::is_footnote_section;
use serde_json::Value;

fn definitions(text: &str) -> Option<Vec<Value>> {
    let tree: Value =
        serde_json::from_str(&crate::tree_json(text, &crate::Options::default())).ok()?;
    let mut out = Vec::new();
    for child in tree
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if child["type"] != "abbreviation_def" {
            return None;
        }
        let mut child = child.clone();
        if let Some(m) = child.as_object_mut() {
            m.remove("pos");
        }
        out.push(child);
    }
    (!out.is_empty()).then_some(out)
}

fn read(cx: &mut Cx, id: Id) -> Option<Vec<Value>> {
    let nodes: Vec<Id> = cx
        .dom
        .node(id)
        .content
        .map(|c| cx.dom.children(c).to_vec())
        .unwrap_or_default();
    if !nodes.iter().all(|&n| cx.dom.is_text(n)) {
        cx.ignored_carrier(id, "data-carve-abbreviations", "the template holds markup");
        return None;
    }
    let text: String = nodes.iter().map(|&n| cx.dom.text(n)).collect();
    let found = definitions(&text);
    if found.is_none() {
        cx.ignored_carrier(
            id,
            "data-carve-abbreviations",
            "the template holds more than definitions",
        );
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
        if cx.dom.is(n, "template") && cx.dom.has(n, "data-carve-abbreviations") {
            let found = read(cx, n).unwrap_or_default();
            for definition in &found {
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
        if self.dom.has(id, "data-carve-abbreviations") {
            self.ignored_carrier(
                id,
                "data-carve-abbreviations",
                "a render writes a template only in the flow of the document",
            );
        }
        None
    }
}
