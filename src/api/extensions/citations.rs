//! The provisional citations bundle of the 0.2 reference.

#![allow(clippy::indexing_slicing)]

use super::apparatus::{self as app, Body};
use super::{Bundle, Claim, Engine, Form, Matcher, declaration};
use crate::api::{Channel, DocumentTransform, Error, Extension, RenderContext, Renderer, Tree};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

const CITE: &str = "carve.cite";
const REFERENCES: &str = "carve.references";
const OWN: &[&str] = &["locator", "prefix", "suffix", "mode"];

/// The citation label style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CitationMode {
    #[default]
    Numbered,
    AuthorDate,
}

/// Optional CSL-JSON entries and the citation label style.
#[derive(Debug, Clone, Default)]
pub struct CitationsOptions {
    pub mode: CitationMode,
    pub bibliography: Option<Vec<Value>>,
}

struct Entry {
    blocks: Vec<Value>,
    author: Option<String>,
    year: Option<String>,
}

fn cite_keys(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .collect()
}

fn claim_cite(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Inline {
        return None;
    }
    let text = app::plain(app::children(claim.node));
    let keys = cite_keys(&text);
    (!keys.is_empty()).then(|| {
        json!({"type":"inline_extension", "name":CITE, "version":"1",
        "children":[], "props":{"keys":keys}, "fallback":[{"type":"text", "value":text}]})
    })
}

fn references(node: &Value) -> bool {
    app::ty(node) == "directive"
        && matches!(
            node.get("kind").and_then(Value::as_str),
            Some("references" | "bibliography")
        )
}

fn author_year(term: &Value) -> (Option<String>, Option<String>) {
    let span = app::children(term)
        .iter()
        .find(|child| app::ty(child) == "span");
    let fact = |key| {
        app::attr(term, key)
            .or_else(|| span.and_then(|node| app::attr(node, key)))
            .map(str::to_string)
    };
    (fact("author"), fact("year"))
}

fn list_entries(list: &Value, entries: &mut HashMap<String, Entry>, keys: &mut Vec<String>) {
    let mut terms = Vec::new();
    let mut facts = (None, None);
    let mut open = false;
    for item in app::children(list) {
        match app::ty(item) {
            "definition_term" => {
                if open {
                    terms.clear();
                }
                open = false;
                terms.push(app::plain(app::children(item)).trim().to_string());
                facts = author_year(item);
            }
            "definition_description" => {
                open = true;
                for key in &terms {
                    if !key.is_empty() {
                        entries.insert(
                            key.clone(),
                            Entry {
                                blocks: app::children(item).to_vec(),
                                author: facts.0.clone(),
                                year: facts.1.clone(),
                            },
                        );
                        keys.push(key.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

fn csl_name(name: &Value) -> String {
    if let Some(literal) = name
        .get("literal")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return literal.into();
    }
    let family = name.get("family").and_then(Value::as_str).unwrap_or("");
    let given = name
        .get("given")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    given.map_or_else(|| family.to_string(), |given| format!("{family}, {given}"))
}

fn csl_entry(csl: &Value) -> Entry {
    let authors = csl
        .get("author")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let year = csl
        .pointer("/issued/date-parts/0/0")
        .and_then(Value::as_i64)
        .map(|n| n.to_string())
        .or_else(|| {
            csl.pointer("/issued/literal")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let mut head = authors
        .iter()
        .map(csl_name)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    if !year.is_empty() {
        head = if head.is_empty() {
            format!("({year})")
        } else {
            format!("{head} ({year})")
        };
    }
    let mut parts = vec![head];
    if let Some(title) = csl.get("title").and_then(Value::as_str) {
        parts.push(title.into());
    }
    parts.retain(|part| !part.is_empty());
    let text = if parts.is_empty() {
        String::new()
    } else {
        format!("{}.", parts.join(". "))
    };
    let blocks = if text.is_empty() {
        Vec::new()
    } else {
        vec![json!({"type":"paragraph", "children":[{"type":"text", "value":text}]})]
    };
    let author = authors
        .first()
        .and_then(|name| name.get("literal").or_else(|| name.get("family")))
        .and_then(Value::as_str)
        .map(str::to_string);
    Entry {
        blocks,
        author,
        year: (!year.is_empty()).then_some(year),
    }
}

/// C2-X-058: the entries, and the keys each directive lists, in walk order.
/// A duplicate key resolves to its last definition and has one list item.
fn collect_entries(doc: &mut Value, pool: &[Value]) -> (HashMap<String, Entry>, Vec<Vec<String>>) {
    let mut entries = HashMap::new();
    let mut directives = Vec::new();
    app::walk(doc, &mut |node| {
        if !references(node) {
            return true;
        }
        let mut keys = Vec::new();
        for list in app::children(node)
            .iter()
            .filter(|node| app::ty(node) == "definition_list")
        {
            list_entries(list, &mut entries, &mut keys);
        }
        directives.push(keys);
        false
    });
    for item in pool {
        if let Some(id) = item.get("id").and_then(Value::as_str) {
            entries
                .entry(id.to_string())
                .or_insert_with(|| csl_entry(item));
        }
    }
    let mut last = HashMap::new();
    for (at, keys) in directives.iter().enumerate() {
        for key in keys {
            last.insert(key.clone(), at);
        }
    }
    for (at, keys) in directives.iter_mut().enumerate() {
        let mut seen = HashSet::new();
        let mut kept: Vec<String> = keys
            .iter()
            .rev()
            .filter(|key| last.get(*key) == Some(&at) && seen.insert((*key).clone()))
            .cloned()
            .collect();
        kept.reverse();
        *keys = kept;
    }
    (entries, directives)
}

struct Uses<'a> {
    entries: &'a HashMap<String, Entry>,
    mode: CitationMode,
    backlinked: bool,
    taken: HashSet<String>,
    numbers: HashMap<String, usize>,
    order: Vec<String>,
    ref_ids: HashMap<String, String>,
    use_ids: HashMap<String, Vec<String>>,
}
impl Uses<'_> {
    fn label(&self, entry: &Entry, number: usize, suppress: bool) -> String {
        if self.mode != CitationMode::AuthorDate {
            return number.to_string();
        }
        if suppress {
            return entry.year.clone().unwrap_or_else(|| number.to_string());
        }
        let label = format!(
            "{} {}",
            entry.author.as_deref().unwrap_or(""),
            entry.year.as_deref().unwrap_or("")
        )
        .trim()
        .to_string();
        if label.is_empty() {
            number.to_string()
        } else {
            label
        }
    }
    /// Directive entries take their numbers in source order, before any use.
    fn number_sources(&mut self, directives: &[Vec<String>]) -> HashSet<String> {
        let mut source_keys = HashSet::new();
        for key in directives.iter().flatten() {
            source_keys.insert(key.clone());
            if !self.numbers.contains_key(key) {
                self.numbers.insert(key.clone(), self.numbers.len() + 1);
                self.ref_ids.insert(
                    key.clone(),
                    app::fresh_id(&format!("ref-{key}"), &mut self.taken),
                );
            }
        }
        source_keys
    }
    fn cite(&mut self, node: &mut Value) -> bool {
        if app::ty(node) != "inline_extension"
            || node.get("name").and_then(Value::as_str) != Some(CITE)
        {
            return true;
        }
        let keys: Vec<String> = node
            .pointer("/props/keys")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if !keys.iter().all(|key| self.entries.contains_key(key)) {
            return false;
        }
        let suppress = app::attr(node, "mode") == Some("suppress-author");
        let mut refs = Vec::new();
        for key in keys {
            if !self.numbers.contains_key(&key) {
                self.numbers.insert(key.clone(), self.numbers.len() + 1);
                self.order.push(key.clone());
                self.ref_ids.insert(
                    key.clone(),
                    app::fresh_id(&format!("ref-{key}"), &mut self.taken),
                );
            }
            let number = self.numbers[&key];
            let mut reference = json!({"key":key, "href":format!("#{}", self.ref_ids[&key]),
                "label":self.label(&self.entries[&key], number, suppress)});
            if self.backlinked {
                let uses = self.use_ids.entry(key.clone()).or_default();
                let id = app::fresh_id(&format!("cite-{key}-{}", uses.len() + 1), &mut self.taken);
                uses.push(id.clone());
                reference["id"] = Value::from(id);
            }
            refs.push(reference);
        }
        node["props"]["refs"] = Value::Array(refs);
        false
    }
}

fn backlinks(ids: &[String]) -> Vec<Value> {
    ids.iter()
        .flat_map(|id| {
            [
                json!({"type":"text", "value":" "}),
                json!({"type":"link", "href":format!("#{id}"), "attrs":{"classes":["ref-backref"]},
            "children":[{"type":"text", "value":"↩"}]}),
            ]
        })
        .collect()
}

fn list_item(entry: &Entry, id: &str, uses: &[String]) -> Value {
    let mut blocks = app::core_nodes(&entry.blocks);
    let mut links = backlinks(uses);
    if blocks.len() == 1 && app::ty(&blocks[0]) == "paragraph" {
        if let Some(children) = blocks[0].get_mut("children").and_then(Value::as_array_mut) {
            children.append(&mut links);
        }
    } else if !links.is_empty() {
        links.remove(0);
        blocks.push(json!({"type":"paragraph", "children":links}));
    }
    json!({"type":"list_item", "attrs":{"id":id}, "children":blocks})
}

fn reference_list(uses: &Uses<'_>, mut keys: Vec<String>, source_order: bool) -> Value {
    if uses.mode == CitationMode::AuthorDate && !source_order {
        keys.sort_by(|a, b| {
            uses.entries[a]
                .author
                .as_deref()
                .unwrap_or(a)
                .cmp(uses.entries[b].author.as_deref().unwrap_or(b))
        });
    }
    let items: Vec<Value> = keys
        .iter()
        .map(|key| {
            list_item(
                &uses.entries[key],
                &uses.ref_ids[key],
                uses.use_ids.get(key).map_or(&[], Vec::as_slice),
            )
        })
        .collect();
    let start = keys.first().and_then(|key| uses.numbers.get(key)).copied();
    let marker = if uses.mode == CitationMode::AuthorDate {
        json!({"kind":"bullet", "char":"-"})
    } else if let Some(start) = start.filter(|n| source_order && *n != 1) {
        json!({"kind":"ordered", "style":"1", "delimiter":".", "start":start})
    } else {
        json!({"kind":"ordered", "style":"1", "delimiter":"."})
    };
    json!({"type":"list", "marker":marker, "spacing":"tight", "attrs":{"classes":["references"]}, "children":items})
}

struct Transform {
    options: CitationsOptions,
}
impl DocumentTransform for Transform {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        let mut doc = app::view(&view)?;
        let (entries, directives) = collect_entries(
            &mut doc,
            self.options.bibliography.as_deref().unwrap_or(&[]),
        );
        let mut uses = Uses {
            entries: &entries,
            mode: self.options.mode,
            backlinked: self.options.bibliography.is_some(),
            taken: app::view_ids(&doc),
            numbers: HashMap::new(),
            order: Vec::new(),
            ref_ids: HashMap::new(),
            use_ids: HashMap::new(),
        };
        let source_keys = uses.number_sources(&directives);
        // Body uses are numbered first, even if a note definition precedes them.
        app::walk(&mut doc, &mut |node| {
            if references(node) || matches!(app::ty(node), "footnote" | "inline_footnote") {
                return false;
            }
            uses.cite(node)
        });
        app::walk(&mut doc, &mut |node| {
            if matches!(app::ty(node), "footnote" | "inline_footnote") {
                app::walk(node, &mut |child| {
                    if references(child) {
                        false
                    } else {
                        uses.cite(child)
                    }
                });
                return false;
            }
            true
        });
        if place(&mut doc, &uses, &directives, &source_keys) {
            super::super::standard::declare_names(&mut doc, &[(REFERENCES, json!(["body"]))]);
        }
        app::finish(&view, doc)
    }
}

/// Places the reference lists: in each directive, the first one with the
/// cited entries no directive defines, or else at the end of the document.
/// Returns whether anything was claimed.
fn place(
    doc: &mut Value,
    uses: &Uses<'_>,
    directives: &[Vec<String>],
    source_keys: &HashSet<String>,
) -> bool {
    let mut placed = false;
    let mut at = 0;
    app::walk(doc, &mut |node| {
        if !references(node) {
            return true;
        }
        let mut keys = directives.get(at).cloned().unwrap_or_default();
        at += 1;
        if !placed {
            keys.extend(
                uses.order
                    .iter()
                    .filter(|key| !source_keys.contains(*key))
                    .cloned(),
            );
        }
        let mut body = node.clone();
        if let Some(parts) = body.get_mut("children").and_then(Value::as_array_mut) {
            parts.retain(|part| {
                app::ty(part) != "definition_list"
                    || !app::children(part)
                        .iter()
                        .any(|item| app::ty(item) == "definition_description")
            });
            parts.push(reference_list(uses, keys, true));
        }
        placed = true;
        *node = app::claim_directive(REFERENCES, node, &[body]);
        false
    });
    if placed || uses.order.is_empty() {
        return placed;
    }
    let Some(parts) = doc.get_mut("children").and_then(Value::as_array_mut) else {
        return false;
    };
    parts.push(json!({"type":"block_extension", "name":REFERENCES, "version":"1",
        "children":[], "slots":{"body":[reference_list(uses, uses.order.clone(), false)]}, "fallback":[]}));
    true
}

struct Cite {
    mode: CitationMode,
}
impl Renderer for Cite {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let Some(refs) = node.pointer("/props/refs").and_then(Value::as_array) else {
            return cx.render_inlines(node.get("fallback").unwrap_or(&Value::Null));
        };
        let mut items = Vec::new();
        for reference in refs {
            let id = reference
                .get("id")
                .and_then(Value::as_str)
                .map(|id| format!(" id=\"{}\"", cx.escape_attribute(id)))
                .unwrap_or_default();
            let href =
                cx.escape_attribute(reference.get("href").and_then(Value::as_str).unwrap_or(""));
            let key =
                cx.escape_attribute(reference.get("key").and_then(Value::as_str).unwrap_or(""));
            let label = cx.escape(reference.get("label").and_then(Value::as_str).unwrap_or(""));
            items.push(format!(
                "<a{id} href=\"{href}\" data-cite-key=\"{key}\">{label}</a>"
            ));
        }
        let part = |key, before: &str, after: &str| {
            app::attr(node, key).map_or(String::new(), |value| {
                format!("{before}{}{after}", cx.escape(value))
            })
        };
        let separator = if self.mode == CitationMode::AuthorDate {
            "; "
        } else {
            ", "
        };
        let inner = format!(
            "{}{}{}{}",
            part("prefix", "", " "),
            items.join(separator),
            part("locator", ", ", ""),
            part("suffix", " ", "")
        );
        let text = if self.mode == CitationMode::AuthorDate {
            format!("({inner})")
        } else {
            format!("[{inner}]")
        };
        let integral = app::attr(node, "mode") == Some("author-in-text");
        let own = node.pointer("/attrs/id").is_some()
            || node
                .pointer("/attrs/classes")
                .and_then(Value::as_array)
                .is_some_and(|classes| !classes.is_empty())
            || node
                .pointer("/attrs/keyValues")
                .and_then(Value::as_array)
                .is_some_and(|pairs| {
                    pairs.iter().any(|pair| {
                        pair.get(0)
                            .and_then(Value::as_str)
                            .is_some_and(|key| !OWN.contains(&key))
                    })
                });
        if !integral && !own {
            return Ok(text);
        }
        Ok(format!(
            "<span{}{}>{text}</span>",
            app::inline_attrs(node, cx, "citation", OWN),
            if integral {
                " data-cite-mode=\"integral\""
            } else {
                ""
            }
        ))
    }
}

/// Resolves citation uses and places numbered or author-date references.
#[must_use]
pub fn citations(options: CitationsOptions) -> Bundle {
    Bundle {
        extensions: vec![
            Extension {
                declaration: declaration(CITE, &[]),
                renderers: vec![
                    (Channel::Interactive, Box::new(Cite { mode: options.mode })),
                    (Channel::BuildTime, Box::new(Cite { mode: options.mode })),
                ],
                transform: None,
            },
            Extension {
                declaration: declaration(REFERENCES, &["body"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(Body)),
                    (Channel::BuildTime, Box::new(Body)),
                ],
                transform: Some(Box::new(Transform { options })),
            },
        ],
        matchers: vec![Matcher {
            extension: CITE.into(),
            form: Form::Inline,
            name: "cite".into(),
            claimer: Box::new(Engine(claim_cite)),
        }],
        host: super::HostFields::default(),
    }
}
