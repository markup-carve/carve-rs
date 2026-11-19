//! The provisional glossary bundle of the 0.2 reference.

use super::apparatus::{self as app, Body};
use super::{Bundle, Claim, Engine, Form, Matcher, declaration};
use crate::api::{Channel, DocumentTransform, Error, Extension, RenderContext, Renderer, Tree};
use crate::options::HeadingIds;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

const TERM: &str = "carve.term";
const GLOSSARY: &str = "carve.glossary";

fn term_key(text: &str) -> String {
    text.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn term_slug(text: &str) -> String {
    crate::resolve::slug::slug(text, HeadingIds::KeepCase)
}

fn claim_term(claim: &Claim<'_>) -> Option<Value> {
    (claim.form == Form::Inline).then(|| {
        let content = app::children(claim.node);
        let mut node = json!({"type":"inline_extension", "name":TERM, "version":"1",
            "children":[], "fallback":app::core_nodes(content)});
        if !content.is_empty() {
            node["slots"] = json!({"text":content});
        }
        node
    })
}

struct Transform;
impl DocumentTransform for Transform {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        let mut doc = app::view(view.clone())?;
        let mut taken = app::view_ids(&doc);
        let mut slugs = HashSet::new();
        let mut targets = HashMap::new();
        let mut ids = HashMap::new();
        app::walk(&mut doc, &mut |node| {
            if app::ty(node) != "directive"
                || node.get("kind").and_then(Value::as_str) != Some("glossary")
            {
                return true;
            }
            for list in app::children(node)
                .iter()
                .filter(|part| app::ty(part) == "definition_list")
            {
                for term in app::children(list)
                    .iter()
                    .filter(|part| app::ty(part) == "definition_term")
                {
                    let text = app::plain(app::children(term));
                    let slug = term_slug(&text);
                    let authored = term.pointer("/attrs/id").and_then(Value::as_str);
                    let id = if slugs.insert(slug.clone()) && authored.is_none() {
                        let id = app::fresh_id(&format!("gloss-{slug}"), &mut taken);
                        if let Some(at) = term.pointer("/pos/startOffset").and_then(Value::as_u64) {
                            ids.insert(at, id.clone());
                        }
                        Some(id)
                    } else {
                        authored.map(str::to_string)
                    };
                    if let Some(id) = id {
                        targets.entry(term_key(&text)).or_insert(id);
                    }
                }
            }
            true
        });
        app::walk(&mut doc, &mut |node| {
            if app::ty(node) == "inline_extension"
                && node.get("name").and_then(Value::as_str) == Some(TERM)
            {
                let text = app::plain(
                    node.pointer("/slots/text")
                        .and_then(Value::as_array)
                        .map_or(&[], Vec::as_slice),
                );
                if let Some(id) = targets.get(&term_key(&text)) {
                    node["props"] = json!({"href":format!("#{id}")});
                }
            }
            true
        });
        let mut claimed = false;
        app::walk(&mut doc, &mut |node| {
            if app::ty(node) != "directive"
                || node.get("kind").and_then(Value::as_str) != Some("glossary")
            {
                return true;
            }
            claimed = true;
            let mut body = node.clone();
            if let Some(parts) = body.get_mut("children").and_then(Value::as_array_mut) {
                for list in parts
                    .iter_mut()
                    .filter(|part| app::ty(part) == "definition_list")
                {
                    let attrs = list
                        .as_object_mut()
                        .and_then(|map| Some(map.entry("attrs").or_insert_with(|| json!({}))));
                    if let Some(attrs) = attrs {
                        let mut classes = attrs
                            .get("classes")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        classes.insert(0, Value::from("glossary"));
                        attrs["classes"] = Value::Array(classes);
                    }
                    if let Some(terms) = list.get_mut("children").and_then(Value::as_array_mut) {
                        for term in terms {
                            if let Some(at) =
                                term.pointer("/pos/startOffset").and_then(Value::as_u64)
                                && let Some(id) = ids.get(&at)
                            {
                                term["attrs"]["id"] = Value::from(id.clone());
                            }
                        }
                    }
                }
            }
            *node = app::claim_directive(GLOSSARY, node, vec![body]);
            false
        });
        if claimed {
            super::super::standard::declare_names(&mut doc, &[(GLOSSARY, json!(["body"]))]);
        }
        app::finish(view, doc)
    }
}

struct Term;
impl Renderer for Term {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let text = cx.render_inlines(node.pointer("/slots/text").unwrap_or(&Value::Null))?;
        let href = node.pointer("/props/href").and_then(Value::as_str);
        Ok(match href {
            Some(href) => format!(
                "<a href=\"{}\"{}>{text}</a>",
                cx.escape_attribute(href),
                app::inline_attrs(node, cx, "term", &["href", "HREF"])
            ),
            None => format!(
                "<span{}>{text}</span>",
                app::inline_attrs(node, cx, "term", &[])
            ),
        })
    }
}

/// Claims term uses, resolves the document's definition entries and places the glossary.
#[must_use]
pub fn glossary() -> Bundle {
    Bundle {
        extensions: vec![
            Extension {
                declaration: declaration(TERM, &["text"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(Term)),
                    (Channel::BuildTime, Box::new(Term)),
                ],
                transform: None,
            },
            Extension {
                declaration: declaration(GLOSSARY, &["body"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(Body)),
                    (Channel::BuildTime, Box::new(Body)),
                ],
                transform: Some(Box::new(Transform)),
            },
        ],
        matchers: vec![Matcher {
            extension: TERM.into(),
            form: Form::Inline,
            name: "term".into(),
            claimer: Box::new(Engine(claim_term)),
        }],
        host: super::HostFields::default(),
    }
}
