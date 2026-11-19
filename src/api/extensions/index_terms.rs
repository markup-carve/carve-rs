//! The provisional index bundle of the 0.2 reference.

#![allow(clippy::indexing_slicing)]

use super::apparatus::{self as app, Body};
use super::{Bundle, Claim, Engine, Form, Matcher, declaration};
use crate::api::{Channel, DocumentTransform, Error, Extension, RenderContext, Renderer, Tree};
use crate::options::HeadingIds;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const MARKER: &str = "carve.indexTerm";
const INDEX: &str = "carve.index";

/// The label at the start of an index back-link's accessible name.
#[derive(Debug, Clone, Default)]
pub struct IndexOptions {
    pub backref_label: Option<String>,
}

fn claim_marker(claim: &Claim<'_>) -> Option<Value> {
    (claim.form == Form::Inline).then(|| {
        let content = app::children(claim.node);
        let mut node = json!({"type":"inline_extension", "name":MARKER, "version":"1",
            "children":[], "fallback":[]});
        if !content.is_empty() {
            node["slots"] = json!({"term":content});
        }
        node
    })
}

struct Entry {
    display: Vec<Value>,
    ids: Vec<String>,
}
struct Transform {
    backref_label: String,
}

fn index_line(entry: &Entry, label: &str, budget: &mut i64) -> Vec<Value> {
    let term = app::plain(&entry.display);
    let name = format!("{label} {term}");
    let mut line = entry.display.clone();
    line.push(json!({"type":"text", "value":" "}));
    for (at, id) in entry.ids.iter().enumerate() {
        *budget -= i64::try_from(id.len() + name.len() + 80).unwrap_or(i64::MAX);
        if *budget < 0 {
            break;
        }
        if at > 0 {
            line.push(json!({"type":"text", "value":" "}));
        }
        let total = entry.ids.len();
        let mut children = vec![json!({"type":"text", "value":"↩"})];
        if total > 1 {
            children.push(json!({"type":"superscript", "children":[{"type":"text", "value":(at + 1).to_string()}]}));
        }
        let accessible = if total == 1 {
            name.clone()
        } else {
            format!("{name} {}", at + 1)
        };
        line.push(
            json!({"type":"link", "href":format!("#{id}"), "children":children,
            "attrs":{"classes":["index-backref"], "keyValues":[["aria-label",accessible]]}}),
        );
    }
    line
}

fn index_list(entries: &BTreeMap<String, Entry>, label: &str, budget: &mut i64) -> Option<Value> {
    let mut items = Vec::new();
    for entry in entries.values() {
        *budget -= i64::try_from(app::plain(&entry.display).len() + 10).unwrap_or(i64::MAX);
        if *budget < 0 {
            break;
        }
        items.push(json!({"type":"list_item", "children":[{"type":"paragraph",
            "children":index_line(entry, label, budget)}]}));
        if *budget < 0 {
            break;
        }
    }
    (!items.is_empty()).then(|| {
        json!({"type":"list", "marker":{"kind":"bullet", "char":"-"},
        "spacing":"tight", "attrs":{"classes":["index"]}, "children":items})
    })
}

impl DocumentTransform for Transform {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        let mut doc = app::view(&view)?;
        let mut taken = app::view_ids(&doc);
        let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
        app::walk(&mut doc, &mut |node| {
            if matches!(app::ty(node), "footnote" | "inline_footnote") {
                return false;
            }
            if app::ty(node) != "inline_extension"
                || node.get("name").and_then(Value::as_str) != Some(MARKER)
            {
                return true;
            }
            let term = node
                .pointer("/slots/term")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            let slug = crate::resolve::slug::slug(&app::plain(term), HeadingIds::Lower);
            let entry = entries.entry(slug.clone()).or_insert_with(|| Entry {
                display: app::core_nodes(term),
                ids: Vec::new(),
            });
            let id = app::fresh_id(&format!("idx-{slug}-{}", entry.ids.len() + 1), &mut taken);
            entry.ids.push(id.clone());
            node["props"] = json!({"id":id});
            false
        });
        if entries.is_empty() {
            return app::finish(&view, doc);
        }
        let src_bytes = doc
            .get("srcByteLength")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let mut budget = 1_000_000_i64.max(8 * src_bytes);
        let mut claimed = false;
        app::walk(&mut doc, &mut |node| {
            if app::ty(node) != "directive"
                || node.get("kind").and_then(Value::as_str) != Some("index")
            {
                return true;
            }
            let Some(list) = index_list(&entries, &self.backref_label, &mut budget) else {
                return false;
            };
            let mut body = node.clone();
            if let Some(parts) = body.get_mut("children").and_then(Value::as_array_mut) {
                parts.push(list);
            }
            *node = app::claim_directive(INDEX, node, &[body]);
            claimed = true;
            false
        });
        if claimed {
            super::super::standard::declare_names(&mut doc, &[(INDEX, json!(["body"]))]);
        }
        app::finish(&view, doc)
    }
}

struct Marker;
impl Renderer for Marker {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        Ok(node
            .pointer("/props/id")
            .and_then(Value::as_str)
            .map_or_else(
                || "<span class=\"index-term\"></span>".to_string(),
                |id| {
                    format!(
                        "<span id=\"{}\" class=\"index-term\"></span>",
                        cx.escape_attribute(id)
                    )
                },
            ))
    }
}

/// Invisible index markers and a sorted list with a back-link per use.
#[must_use]
pub fn index(options: IndexOptions) -> Bundle {
    let label = options.backref_label.unwrap_or_else(|| "Back to".into());
    Bundle {
        extensions: vec![
            Extension {
                declaration: declaration(MARKER, &["term"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(Marker)),
                    (Channel::BuildTime, Box::new(Marker)),
                ],
                transform: None,
            },
            Extension {
                declaration: declaration(INDEX, &["body"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(Body)),
                    (Channel::BuildTime, Box::new(Body)),
                ],
                transform: Some(Box::new(Transform {
                    backref_label: label,
                })),
            },
        ],
        matchers: vec![Matcher {
            extension: MARKER.into(),
            form: Form::Inline,
            name: "index".into(),
            claimer: Box::new(Engine(claim_marker)),
        }],
        host: super::HostFields::default(),
    }
}
