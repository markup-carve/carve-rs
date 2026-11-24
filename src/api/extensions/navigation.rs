//! Opt-in navigation and link transforms carried forward from the 0.1 host.

use super::{Bundle, declaration};
use crate::api::{Channel, DocumentTransform, Error, Extension, RenderContext, Renderer, Tree};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn bundle(name: &str, transform: impl DocumentTransform + 'static) -> Bundle {
    Bundle {
        extensions: vec![Extension {
            declaration: declaration(name, &[]),
            renderers: Vec::new(),
            transform: Some(Box::new(transform)),
        }],
        ..Bundle::default()
    }
}

fn edit(view: &Tree, f: impl FnOnce(&mut Value)) -> Result<Tree, Error> {
    let mut doc = view.value()?;
    f(&mut doc);
    Tree::trusted_view(doc, view.top.clone())
}

fn visit(value: &mut Value, f: &mut impl FnMut(&mut Map<String, Value>)) {
    match value {
        Value::Array(items) => {
            for item in items {
                visit(item, f);
            }
        }
        Value::Object(map) => {
            if map.contains_key("type") {
                f(map);
            }
            for (key, child) in map.iter_mut() {
                if !matches!(
                    key.as_str(),
                    "fallback" | "source" | "payload" | "props" | "pos" | "attrs"
                ) {
                    visit(child, f);
                }
            }
        }
        _ => {}
    }
}

fn rewrite(
    value: &mut Value,
    parents: &mut Vec<String>,
    f: &mut impl FnMut(&Value, &[String]) -> Option<Vec<Value>>,
) {
    match value {
        Value::Array(items) => {
            let mut at = 0;
            while at < items.len() {
                let Some(current) = items.get(at) else {
                    break;
                };
                if let Some(replacement) = f(current, parents) {
                    let count = replacement.len();
                    items.splice(at..=at, replacement);
                    at += count;
                } else {
                    if let Some(current) = items.get_mut(at) {
                        rewrite(current, parents, f);
                    }
                    at += 1;
                }
            }
        }
        Value::Object(map) => {
            let name = map
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            parents.push(name);
            for (key, child) in map.iter_mut() {
                if !matches!(
                    key.as_str(),
                    "source" | "payload" | "props" | "pos" | "attrs"
                ) {
                    rewrite(child, parents, f);
                }
            }
            parents.pop();
        }
        _ => {}
    }
}

fn attrs(map: &mut Map<String, Value>) -> Option<&mut Map<String, Value>> {
    map.entry("attrs")
        .or_insert_with(|| json!({}))
        .as_object_mut()
}

fn key(attrs: &mut Map<String, Value>, name: &str, value: &str) {
    let Some(keys) = attrs
        .entry("keyValues")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return;
    };
    keys.retain(|v| {
        !v.get(0)
            .and_then(Value::as_str)
            .is_some_and(|k| k.eq_ignore_ascii_case(name))
    });
    keys.push(json!([name, value]));
}

fn derived(nodes: &[Value]) -> Vec<Value> {
    nodes
        .iter()
        .filter(|node| {
            !node
                .get("data")
                .and_then(Value::as_object)
                .is_some_and(|data| data.values().any(|v| v.as_str() == Some("chrome")))
                && !matches!(
                    node.get("type").and_then(Value::as_str),
                    Some("footnote_ref" | "inline_footnote")
                )
        })
        .cloned()
        .collect()
}

fn freeze_xrefs(doc: &mut Value) {
    let mut by_id = BTreeMap::new();
    visit_read(doc, &mut |node| {
        if node.get("type").and_then(Value::as_str) == Some("heading") {
            if let Some(id) = heading_id(node) {
                by_id.entry(id).or_insert_with(|| {
                    node.get("children")
                        .and_then(Value::as_array)
                        .map_or_else(Vec::new, |nodes| derived(nodes))
                });
            }
        }
    });
    rewrite(doc, &mut Vec::new(), &mut |node, _| {
        if node.get("type").and_then(Value::as_str) != Some("xref")
            || node.get("namespace").and_then(Value::as_str) != Some("heading")
            || node
                .get("display")
                .and_then(Value::as_str)
                .is_some_and(|v| v != "title")
        {
            return None;
        }
        let key = node.get("key").and_then(Value::as_str)?;
        let children = by_id.get(key)?;
        Some(vec![
            json!({"type":"link","href":format!("#{key}"),"children":children}),
        ])
    });
}

fn heading_id(map: &Map<String, Value>) -> Option<String> {
    map.get("attrs")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .or_else(|| map.get("generatedId").and_then(Value::as_str))
        .map(str::to_string)
}

/// The default target and relation of external HTTP links.
#[derive(Debug, Clone)]
pub struct ExternalLinksOptions {
    pub target: String,
    pub rel: String,
    pub nofollow: bool,
}
impl Default for ExternalLinksOptions {
    fn default() -> Self {
        Self {
            target: "_blank".into(),
            rel: "noopener noreferrer".into(),
            nofollow: false,
        }
    }
}
struct ExternalLinks(ExternalLinksOptions);
impl DocumentTransform for ExternalLinks {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        edit(&view, |doc| {
            rewrite(doc, &mut Vec::new(), &mut |node, _| {
                if !matches!(
                    node.get("type").and_then(Value::as_str),
                    Some("link" | "autolink")
                ) {
                    return None;
                }
                let href = node.get("href").and_then(Value::as_str)?;
                if !href.to_ascii_lowercase().starts_with("http://")
                    && !href.to_ascii_lowercase().starts_with("https://")
                {
                    return None;
                }
                let target = self.0.target.clone();
                let mut rel = self.0.rel.clone();
                if self.0.nofollow && !rel.split_whitespace().any(|s| s == "nofollow") {
                    if !rel.is_empty() {
                        rel.push(' ');
                    }
                    rel.push_str("nofollow");
                }
                let mut link = node.as_object().cloned().unwrap_or_default();
                if node.get("type").and_then(Value::as_str) == Some("autolink") {
                    link.insert("type".into(), "link".into());
                    link.insert("children".into(), json!([{"type":"text","value":node.get("text").and_then(Value::as_str).unwrap_or(href)}]));
                    link.remove("text");
                }
                let a = attrs(&mut link)?;
                if target.is_empty() {
                    if let Some(keys) = a.get_mut("keyValues").and_then(Value::as_array_mut) {
                        keys.retain(|v| {
                            !v.get(0)
                                .and_then(Value::as_str)
                                .is_some_and(|k| k.eq_ignore_ascii_case("target"))
                        });
                    }
                } else {
                    key(a, "target", &target);
                }
                key(a, "rel", &rel);
                Some(vec![Value::Object(link)])
            });
        })
    }
}
#[must_use]
pub fn external_links(options: ExternalLinksOptions) -> Bundle {
    bundle("carve.externalLinks", ExternalLinks(options))
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // independent public extension options
pub struct HeadingPermalinksOptions {
    pub symbol: String,
    pub css_class: String,
    pub aria_label: String,
    pub levels: Vec<u8>,
    pub prepend: bool,
    pub show_on_hover: bool,
    pub copy_to_clipboard: bool,
    pub lowercase_ids: bool,
}
impl Default for HeadingPermalinksOptions {
    fn default() -> Self {
        Self {
            symbol: "¶".into(),
            css_class: "permalink".into(),
            aria_label: "Permalink".into(),
            levels: vec![1, 2, 3, 4, 5, 6],
            prepend: false,
            show_on_hover: false,
            copy_to_clipboard: false,
            lowercase_ids: false,
        }
    }
}
struct HeadingPermalinks(HeadingPermalinksOptions);
impl DocumentTransform for HeadingPermalinks {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        edit(&view, |doc| {
            freeze_xrefs(doc);
            visit(doc, &mut |node| {
                if node.get("type").and_then(Value::as_str) != Some("heading") {
                    return;
                }
                let level = node
                    .get("level")
                    .and_then(Value::as_u64)
                    .and_then(|n| u8::try_from(n).ok())
                    .unwrap_or(0);
                if !self.0.levels.contains(&level) {
                    return;
                }
                let Some(mut id) = heading_id(node) else {
                    return;
                };
                let has_explicit_id = node.get("attrs").and_then(|a| a.get("id")).is_some();
                if self.0.lowercase_ids && !has_explicit_id {
                    id = id.to_lowercase();
                }
                if !has_explicit_id {
                    if let Some(a) = attrs(node) {
                        a.insert("id".into(), id.clone().into());
                    }
                }
                let mut keys = vec![json!(["aria-label", self.0.aria_label])];
                if self.0.copy_to_clipboard {
                    keys.push(json!(["data-permalink-copy", ""]));
                }
                let anchor = json!({"type":"link", "href":format!("#{id}"), "data":{"carve.headingPermalinks":"chrome"},
                "attrs":{"classes":[self.0.css_class],"keyValues":keys},
                "children":[{"type":"text","value":self.0.symbol}]});
                let anchor = if self.0.show_on_hover {
                    json!({"type":"span","data":{"carve.headingPermalinks":"chrome"},"attrs":{"classes":["permalink-wrapper","permalink-hover"]},"children":[anchor]})
                } else {
                    anchor
                };
                if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
                    if self.0.prepend {
                        children.insert(0, json!({"type":"text","value":" ","data":{"carve.headingPermalinks":"chrome"}}));
                        children.insert(0, anchor);
                    } else {
                        children.push(json!({"type":"text","value":" ","data":{"carve.headingPermalinks":"chrome"}}));
                        children.push(anchor);
                    }
                }
            });
        })
    }
}
#[must_use]
pub fn heading_permalinks(options: HeadingPermalinksOptions) -> Bundle {
    bundle("carve.headingPermalinks", HeadingPermalinks(options))
}

#[derive(Debug, Clone, Copy, Default)]
pub enum CrossrefStyle {
    Number,
    #[default]
    NumberTitle,
    Title,
}
#[derive(Debug, Clone)]
pub struct HeadingNumbersOptions {
    pub min_level: u8,
    pub label: String,
    pub crossref: CrossrefStyle,
}
impl Default for HeadingNumbersOptions {
    fn default() -> Self {
        Self {
            min_level: 1,
            label: "Section".into(),
            crossref: CrossrefStyle::NumberTitle,
        }
    }
}
struct HeadingNumbers(HeadingNumbersOptions);

fn freeze_heading_links(doc: &mut Value) {
    let mut heading_ids = Vec::new();
    let mut definitions = Vec::new();
    visit_read(
        doc,
        &mut |node| match node.get("type").and_then(Value::as_str) {
            Some("heading") => {
                if let Some(id) = heading_id(node) {
                    heading_ids.push(id);
                }
            }
            Some("link_reference_definition") => {
                if let Some(key) = node.get("key").and_then(Value::as_str) {
                    definitions.push(key.trim().to_lowercase());
                }
            }
            _ => {}
        },
    );
    visit(doc, &mut |node| {
        if node.get("type").and_then(Value::as_str) != Some("link")
            || node.get("namespace").and_then(Value::as_str) != Some("link")
        {
            return;
        }
        let Some(key) = node.get("key").and_then(Value::as_str) else {
            return;
        };
        if definitions.contains(&key.trim().to_lowercase())
            || !node
                .get("href")
                .and_then(Value::as_str)
                .is_some_and(|h| heading_ids.iter().any(|id| h == format!("#{id}")))
        {
            return;
        }
        node.remove("key");
        node.remove("namespace");
        node.remove("refForm");
        node.insert(
            "data".into(),
            json!({"carve.headingNumbers":"heading-reference"}),
        );
    });
}

/// Advances the depth counter for a heading at `level`; closing deeper headings continues their counter.
fn count_heading(levels: &mut Vec<usize>, counts: &mut Vec<usize>, level: usize) -> String {
    let mut closed = None;
    while levels.last().is_some_and(|last| *last > level) {
        levels.pop();
        closed = counts.pop();
    }
    if levels.last() == Some(&level) {
        if let Some(last) = counts.last_mut() {
            *last += 1;
        }
    } else {
        levels.push(level);
        counts.push(closed.map_or(1, |n| n + 1));
    }
    counts
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

fn number_headings(doc: &mut Value, min_level: u8) -> BTreeMap<String, (String, Vec<Value>)> {
    let mut levels: Vec<usize> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    let mut labels = BTreeMap::new();
    // carve#6531: only top-level headings, the ones a section wraps (C2-H-026), and a number counts depth, not level.
    let Some(children) = doc.get_mut("children").and_then(Value::as_array_mut) else {
        return labels;
    };
    for node in children.iter_mut() {
        let Some(node) = node.as_object_mut() else {
            continue;
        };
        if node.get("type").and_then(Value::as_str) != Some("heading") {
            continue;
        }
        let level = node
            .get("level")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(0);
        if level < usize::from(min_level.max(1)) || level > 6 {
            continue;
        }
        if node
            .get("attrs")
            .and_then(|a| a.get("classes"))
            .and_then(Value::as_array)
            .is_some_and(|cs| cs.iter().any(|c| c.as_str() == Some("unnumbered")))
        {
            continue;
        }
        let number = count_heading(&mut levels, &mut counts, level);
        let title = node
            .get("children")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |n| derived(n));
        if let Some(id) = heading_id(node) {
            labels.insert(id.clone(), (number.clone(), title));
            if node.get("attrs").and_then(|a| a.get("id")).is_none()
                && let Some(a) = attrs(node)
            {
                a.insert("id".into(), id.into());
            }
        }
        if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
            children.insert(
                0,
                json!({"type":"text","value":" ","data":{"carve.headingNumbers":"chrome"}}),
            );
            children.insert(0, json!({"type":"span","data":{"carve.headingNumbers":"chrome"},
                "attrs":{"classes":["section-number"]},"children":[{"type":"text","value":number}]}));
        }
    }
    labels
}

fn number_xrefs(
    doc: &mut Value,
    labels: &BTreeMap<String, (String, Vec<Value>)>,
    options: &HeadingNumbersOptions,
) {
    visit(doc, &mut |node| {
        if node.get("type").and_then(Value::as_str) != Some("xref")
            || node.get("namespace").and_then(Value::as_str) != Some("heading")
            || node
                .get("display")
                .and_then(Value::as_str)
                .is_some_and(|d| d != "title")
        {
            return;
        }
        let Some(id) = node.get("key").and_then(Value::as_str) else {
            return;
        };
        let Some((number, title)) = labels.get(id) else {
            return;
        };
        let children = match options.crossref {
            CrossrefStyle::Number => {
                vec![json!({"type":"text","value":format!("{} {number}", options.label)})]
            }
            CrossrefStyle::NumberTitle => {
                let lead = json!({"type":"text","value":format!("{} {number} - ", options.label)});
                std::iter::once(lead).chain(title.iter().cloned()).collect()
            }
            CrossrefStyle::Title => return,
        };
        node.insert("type".into(), "link".into());
        node.insert("children".into(), Value::Array(children));
        node.remove("namespace");
        node.remove("key");
    });
}

impl DocumentTransform for HeadingNumbers {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        edit(&view, |doc| {
            freeze_heading_links(doc);
            let labels = number_headings(doc, self.0.min_level);
            number_xrefs(doc, &labels, &self.0);
            freeze_xrefs(doc);
        })
    }
}
#[must_use]
pub fn heading_numbers(options: HeadingNumbersOptions) -> Bundle {
    bundle("carve.headingNumbers", HeadingNumbers(options))
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ListType {
    #[default]
    Ul,
    Ol,
}
#[derive(Debug, Clone, Copy, Default)]
pub enum Position {
    #[default]
    Top,
    Bottom,
}
#[derive(Debug, Clone)]
pub struct TableOfContentsOptions {
    pub min_level: u8,
    pub max_level: u8,
    pub list_type: ListType,
    pub css_class: String,
    pub position: Position,
    pub collapsible: bool,
    pub summary: String,
    pub open: bool,
}
impl Default for TableOfContentsOptions {
    fn default() -> Self {
        Self {
            min_level: 1,
            max_level: 6,
            list_type: ListType::Ul,
            css_class: "toc".into(),
            position: Position::Top,
            collapsible: false,
            summary: "Table of Contents".into(),
            open: false,
        }
    }
}
// carve#6531: nest by depth like carve-js `entryList`; a heading that closes deeper ones continues at its depth.
fn list_node(entries: &[(u8, String, Vec<Value>)], ordered: bool) -> Option<Value> {
    fn finish(items: Vec<Value>, ordered: bool) -> Value {
        let marker = if ordered {
            json!({"kind":"ordered","style":"1","delimiter":"."})
        } else {
            json!({"kind":"bullet","char":"-"})
        };
        let mut list = Map::new();
        list.insert("type".into(), "list".into());
        list.insert("marker".into(), marker);
        list.insert("spacing".into(), "tight".into());
        list.insert("children".into(), Value::Array(items));
        Value::Object(list)
    }
    fn close(stack: &mut Vec<(u8, Vec<Value>)>, ordered: bool) {
        if let Some((_, items)) = stack.pop()
            && let Some(Value::Array(children)) = stack
                .last_mut()
                .and_then(|(_, parent)| parent.last_mut())
                .and_then(|item| item.get_mut("children"))
        {
            children.push(finish(items, ordered));
        }
    }
    let parent_level = |stack: &[(u8, Vec<Value>)]| {
        stack
            .len()
            .checked_sub(2)
            .and_then(|i| stack.get(i))
            .map(|(level, _)| *level)
    };
    let mut stack: Vec<(u8, Vec<Value>)> = vec![(entries.first()?.0, Vec::new())];
    for (level, id, title) in entries {
        if let Some((top_level, top_items)) = stack.last()
            && !top_items.is_empty()
        {
            if level > top_level {
                stack.push((*level, Vec::new()));
            } else {
                while parent_level(&stack).is_some_and(|parent| *level <= parent) {
                    close(&mut stack, ordered);
                }
                if let Some(top) = stack.last_mut() {
                    top.0 = *level;
                }
            }
        }
        if let Some((_, items)) = stack.last_mut() {
            items.push(json!({"type":"list_item","children":[
                {"type":"paragraph","children":[{"type":"link","href":format!("#{id}"),"children":title}]}
            ]}));
        }
    }
    while stack.len() > 1 {
        close(&mut stack, ordered);
    }
    stack.pop().map(|(_, items)| finish(items, ordered))
}

fn toc_entries(doc: &Value) -> Vec<(u8, String, Vec<Value>)> {
    let mut entries = Vec::new();
    let mut add = |node: &Map<String, Value>| {
        if node.get("type").and_then(Value::as_str) != Some("heading") {
            return;
        }
        let level = node
            .get("level")
            .and_then(Value::as_u64)
            .and_then(|n| u8::try_from(n).ok())
            .unwrap_or(0);
        if let Some(id) = heading_id(node) {
            let title = node
                .get("children")
                .and_then(Value::as_array)
                .map_or_else(Vec::new, |nodes| derived(nodes));
            entries.push((level, id, title));
        }
    };
    if let Some(children) = doc.get("children").and_then(Value::as_array) {
        for node in children {
            if let Some(map) = node.as_object() {
                add(map);
            }
        }
    }
    entries
}
fn visit_read(value: &Value, f: &mut impl FnMut(&Map<String, Value>)) {
    match value {
        Value::Array(items) => {
            for item in items {
                visit_read(item, f);
            }
        }
        Value::Object(map) => {
            if map.contains_key("type") {
                f(map);
            }
            for (key, child) in map {
                if !matches!(
                    key.as_str(),
                    "fallback" | "source" | "payload" | "props" | "pos" | "attrs"
                ) {
                    visit_read(child, f);
                }
            }
        }
        _ => {}
    }
}
struct TableOfContents(TableOfContentsOptions);
impl DocumentTransform for TableOfContents {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        edit(&view, |doc| {
            let entries: Vec<_> = toc_entries(doc)
                .into_iter()
                .filter(|(level, _, _)| *level >= self.0.min_level && *level <= self.0.max_level)
                .collect();
            let Some(list) = list_node(&entries, matches!(self.0.list_type, ListType::Ol)) else {
                return;
            };
            let mut node = json!({"type":"block_extension","name":"carve.toc","version":"1",
                "children":[],"attrs":{"classes":[self.0.css_class]},"slots":{"list":[list.clone()]},
                "fallback":[list]});
            if self.0.collapsible {
                if let Some(map) = node.as_object_mut() {
                    map.insert(
                        "props".into(),
                        json!({"collapsible":true,"summary":self.0.summary,"open":self.0.open}),
                    );
                }
            }
            if let Some(children) = doc.get_mut("children").and_then(Value::as_array_mut) {
                if matches!(self.0.position, Position::Top) {
                    children.insert(0, node);
                } else {
                    children.push(node);
                }
            }
        })
    }
}
#[must_use]
pub fn table_of_contents(options: TableOfContentsOptions) -> Bundle {
    toc_bundle("carve.toc", TableOfContents(options), false)
}

struct TocPlacement;
impl DocumentTransform for TocPlacement {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        edit(&view, |doc| {
            // carve#6531: the placed table lists the top-level headings, as tableOfContents does.
            let entries = toc_entries(doc);
            visit(doc, &mut |node| {
                if node.get("type").and_then(Value::as_str) == Some("directive")
                    && node.get("kind").and_then(Value::as_str) == Some("toc")
                {
                    let body = node
                        .get("children")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let title = node
                        .get("title")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let mut attrs = node.get("attrs").cloned().unwrap_or_else(|| json!({}));
                    let pairs = attrs
                        .get("keyValues")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let value = |key: &str| {
                        pairs
                            .iter()
                            .find(|v| v.get(0).and_then(Value::as_str) == Some(key))
                            .and_then(|v| v.get(1))
                            .and_then(Value::as_str)
                            .and_then(|s| s.parse::<u8>().ok())
                            .map(|n| n.clamp(1, 6))
                    };
                    let (low, high) = if value("from").is_some() || value("to").is_some() {
                        let a = value("from").unwrap_or(1);
                        let b = value("to").unwrap_or(6);
                        (a.min(b), a.max(b))
                    } else {
                        (1, value("depth").unwrap_or(6))
                    };
                    let selected: Vec<_> = entries
                        .iter()
                        .filter(|(level, _, _)| *level >= low && *level <= high)
                        .cloned()
                        .collect();
                    let list = list_node(&selected, false).map_or_else(Vec::new, |v| vec![v]);
                    if let Some(map) = attrs.as_object_mut() {
                        if let Some(classes) = map
                            .entry("classes")
                            .or_insert_with(|| json!([]))
                            .as_array_mut()
                        {
                            classes.retain(|v| v.as_str() != Some("toc"));
                            classes.insert(0, "toc".into());
                        }
                        if let Some(pairs) = map.get_mut("keyValues").and_then(Value::as_array_mut)
                        {
                            pairs.retain(|v| {
                                !v.get(0)
                                    .and_then(Value::as_str)
                                    .is_some_and(|k| matches!(k, "from" | "to" | "depth"))
                            });
                        }
                    }
                    let mut fallback = Vec::new();
                    if !title.is_empty() {
                        fallback.push(json!({"type":"paragraph","children":[{"type":"strong","children":title.clone()}]}));
                    }
                    fallback.extend(body.clone());
                    fallback.extend(list.clone());
                    let label = node.get("groupLabel").cloned();
                    if let Value::Object(next) = json!({"type":"block_extension","name":"carve.tocPlacement",
                        "version":"1","children":[],"attrs":attrs,"slots":{"title":title,"body":body,"list":list},
                        "fallback":fallback})
                    {
                        *node = next;
                    }
                    if let Some(label) = label {
                        node.insert("props".into(), json!({"label":label}));
                    }
                }
            });
        })
    }
}
#[must_use]
pub fn toc_placement() -> Bundle {
    toc_bundle("carve.tocPlacement", TocPlacement, true)
}

fn indent_html(text: &str, depth: usize) -> String {
    let prefix = "  ".repeat(depth);
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

struct TocRenderer {
    placed: bool,
}
fn toc_nav_name(node: &Value, cx: &RenderContext<'_>) -> String {
    let label = cx.label("tocNav").unwrap_or("Table of contents");
    let authored = node
        .pointer("/attrs/keyValues")
        .and_then(Value::as_array)
        .is_some_and(|pairs| {
            pairs.iter().any(|pair| {
                pair.get(0).and_then(Value::as_str).is_some_and(|name| {
                    matches!(
                        name.to_ascii_lowercase().as_str(),
                        "aria-label" | "aria-labelledby"
                    )
                })
            })
        });
    if authored || label.is_empty() {
        String::new()
    } else {
        format!(" aria-label=\"{}\"", cx.escape_attribute(label))
    }
}
impl Renderer for TocRenderer {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let list = cx.render_blocks(node.pointer("/slots/list").unwrap_or(&Value::Null))?;
        let depth = cx.block_depth();
        let lead = "  ".repeat(depth);
        let list = indent_html(&list, depth + 1);
        let attrs = cx.attributes(node);
        if self.placed {
            let body = cx.render_blocks(node.pointer("/slots/body").unwrap_or(&Value::Null))?;
            let mut name = toc_nav_name(node, cx);
            let title = node.pointer("/slots/title").and_then(Value::as_array);
            let mut head = String::new();
            if title.is_some_and(|items| !items.is_empty()) {
                let id = if name.is_empty() {
                    String::new()
                } else {
                    cx.next_id("toc-title")
                };
                if !id.is_empty() {
                    name = format!(" aria-labelledby=\"{}\"", cx.escape_attribute(&id));
                }
                let title_html =
                    cx.render_inlines(node.pointer("/slots/title").unwrap_or(&Value::Null))?;
                let _ = writeln!(
                    head,
                    "{}<p class=\"admonition-title\"{}>{title_html}</p>",
                    "  ".repeat(depth + 1),
                    if id.is_empty() {
                        String::new()
                    } else {
                        format!(" id=\"{}\"", cx.escape_attribute(&id))
                    }
                );
            }
            if let Some(label) = node
                .pointer("/props/label")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                let _ = writeln!(
                    head,
                    "{}<p class=\"div-label\">{}</p>",
                    "  ".repeat(depth + 1),
                    cx.escape(label)
                );
            }
            let nav = format!("<nav{attrs}{name}>\n{head}{list}\n{lead}</nav>");
            return Ok(if body.is_empty() {
                nav
            } else {
                format!("{body}\n{lead}{nav}")
            });
        }
        if node.pointer("/props/collapsible") == Some(&Value::Bool(true)) {
            let summary = node
                .pointer("/props/summary")
                .and_then(Value::as_str)
                .unwrap_or("Table of Contents");
            let open = if node.pointer("/props/open") == Some(&Value::Bool(true)) {
                " open"
            } else {
                ""
            };
            Ok(format!(
                "<details{attrs}{open}>\n{}<summary>{}</summary>\n{list}\n{lead}</details>",
                "  ".repeat(depth + 1),
                cx.escape(summary)
            ))
        } else {
            let name = toc_nav_name(node, cx);
            Ok(format!("<nav{attrs}{name}>\n{list}\n{lead}</nav>"))
        }
    }
}

fn toc_bundle(name: &str, transform: impl DocumentTransform + 'static, placed: bool) -> Bundle {
    Bundle {
        extensions: vec![Extension {
            declaration: declaration(name, &["list"]),
            renderers: vec![
                (Channel::Interactive, Box::new(TocRenderer { placed })),
                (Channel::BuildTime, Box::new(TocRenderer { placed })),
            ],
            transform: Some(Box::new(transform)),
        }],
        ..Bundle::default()
    }
}

#[derive(Debug, Clone)]
pub struct HeadingReferenceOptions {
    pub css_class: String,
}
impl Default for HeadingReferenceOptions {
    fn default() -> Self {
        Self {
            css_class: "heading-ref".into(),
        }
    }
}
struct HeadingReference(HeadingReferenceOptions);
fn is_heading_reference(node: &Map<String, Value>, ids: &[String], definitions: &[String]) -> bool {
    let marked = node
        .get("data")
        .and_then(Value::as_object)
        .is_some_and(|data| {
            data.values()
                .any(|v| v.as_str() == Some("heading-reference"))
        });
    let original = node.get("namespace").and_then(Value::as_str) == Some("link")
        && node
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|key| !definitions.contains(&key.trim().to_lowercase()));
    node.get("type").and_then(Value::as_str) == Some("link")
        && (original || marked)
        && node
            .get("href")
            .and_then(Value::as_str)
            .is_some_and(|href| {
                href.starts_with('#') && ids.iter().any(|id| href == format!("#{id}"))
            })
}
impl DocumentTransform for HeadingReference {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        edit(&view, |doc| {
            let mut ids = Vec::new();
            let mut definitions = Vec::new();
            visit_read(doc, &mut |node| {
                if node.get("type").and_then(Value::as_str) == Some("heading") {
                    if let Some(id) = heading_id(node) {
                        ids.push(id);
                    }
                } else if node.get("type").and_then(Value::as_str)
                    == Some("link_reference_definition")
                {
                    if let Some(key) = node.get("key").and_then(Value::as_str) {
                        definitions.push(key.trim().to_lowercase());
                    }
                }
            });
            visit(doc, &mut |node| {
                if !is_heading_reference(node, &ids, &definitions) {
                    return;
                }
                let Some(a) = attrs(node) else {
                    return;
                };
                let Some(classes) = a
                    .entry("classes")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                else {
                    return;
                };
                for class in self.0.css_class.split_whitespace() {
                    if !classes.iter().any(|c| c.as_str() == Some(class)) {
                        classes.push(class.into());
                    }
                }
            });
        })
    }
}
#[must_use]
pub fn heading_reference(options: HeadingReferenceOptions) -> Bundle {
    bundle("carve.headingReference", HeadingReference(options))
}

#[derive(Debug, Clone)]
pub struct AutolinkOptions {
    pub allowed_schemes: Vec<String>,
}
impl Default for AutolinkOptions {
    fn default() -> Self {
        Self {
            allowed_schemes: vec!["https".into(), "http".into(), "mailto".into()],
        }
    }
}
struct Autolink(AutolinkOptions);

fn linkify_text(source: &str, url: &regex::Regex, allowed: &[String]) -> Option<Vec<Value>> {
    let mut result = Vec::new();
    let mut last = 0;
    for found in url.find_iter(source) {
        let before = source
            .get(..found.start())
            .and_then(|text| text.chars().next_back());
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        let word = found
            .as_str()
            .trim_end_matches(|c: char| ".,;:!?'\"".contains(c));
        if word.is_empty() {
            continue;
        }
        let is_url = word.contains("://");
        let scheme = if is_url {
            word.split(':').next().unwrap_or("")
        } else {
            "mailto"
        };
        if !allowed.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
            continue;
        }
        if let Some(before) = source.get(last..found.start()).filter(|s| !s.is_empty()) {
            result.push(json!({"type":"text","value":before}));
        }
        let href = if is_url || word.starts_with("mailto:") {
            word.to_string()
        } else {
            format!("mailto:{word}")
        };
        result.push(json!({"type":"autolink","href":href,"text":word}));
        last = found.start() + word.len();
    }
    if result.is_empty() {
        return None;
    }
    if let Some(after) = source.get(last..).filter(|s| !s.is_empty()) {
        result.push(json!({"type":"text","value":after}));
    }
    Some(result)
}

impl DocumentTransform for Autolink {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        let url = regex::Regex::new(r"(?i)https?://[^\s<>\[\](){}]+|mailto:[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
            .map_err(|e| Error::option("autolink", e.to_string()))?;
        edit(&view, |doc| {
            rewrite(doc, &mut Vec::new(), &mut |node, parents| {
                if node.get("type").and_then(Value::as_str) != Some("text")
                    || parents.iter().any(|p| {
                        matches!(
                            p.as_str(),
                            "link"
                                | "autolink"
                                | "image"
                                | "xref"
                                | "code"
                                | "raw_inline"
                                | "math"
                                | "literal_inline"
                        )
                    })
                {
                    return None;
                }
                linkify_text(
                    node.get("value").and_then(Value::as_str)?,
                    &url,
                    &self.0.allowed_schemes,
                )
            });
        })
    }
}
#[must_use]
pub fn autolink(options: AutolinkOptions) -> Bundle {
    bundle("carve.autolink", Autolink(options))
}

#[derive(Debug, Clone)]
pub struct LinkPolicyOptions {
    pub allowed_schemes: Option<Vec<String>>,
    pub denied_schemes: Vec<String>,
    pub allowed_domains: Option<Vec<String>>,
    pub denied_domains: Vec<String>,
    pub allow_external: bool,
    pub allow_internal: bool,
    pub rel: Vec<String>,
    pub base_host: Option<String>,
}
impl Default for LinkPolicyOptions {
    fn default() -> Self {
        Self {
            allowed_schemes: None,
            denied_schemes: vec![
                "javascript".into(),
                "vbscript".into(),
                "data".into(),
                "file".into(),
            ],
            allowed_domains: None,
            denied_domains: Vec::new(),
            allow_external: true,
            allow_internal: true,
            rel: Vec::new(),
            base_host: None,
        }
    }
}
fn numeric_host(host: &str) -> Option<String> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let nums: Vec<u64> = parts
        .iter()
        .map(|part| {
            if let Some(hex) = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16).ok()
            } else if part.len() > 1 && part.starts_with('0') {
                u64::from_str_radix(part, 8).ok()
            } else {
                part.parse().ok()
            }
        })
        .collect::<Option<_>>()?;
    let mut ip = 0u64;
    for (i, num) in nums.iter().enumerate() {
        let bits = if i + 1 == nums.len() {
            8 * (5 - nums.len())
        } else {
            8
        };
        if *num >= 1u64 << bits {
            return None;
        }
        ip = (ip << bits) | num;
    }
    if ip > u64::from(u32::MAX) {
        return None;
    }
    Some(format!(
        "{}.{}.{}.{}",
        ip >> 24,
        (ip >> 16) & 255,
        (ip >> 8) & 255,
        ip & 255
    ))
}
fn canonical_host(host: &str) -> String {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    numeric_host(&lower).unwrap_or(lower)
}
fn domain(host: &str, list: &[String]) -> bool {
    list.iter().any(|d| {
        let d = canonical_host(d);
        host == d || host.ends_with(&format!(".{d}"))
    })
}
fn link_allowed(href: &str, opts: &LinkPolicyOptions) -> bool {
    let trimmed = href.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_ascii_control());
    let clean = trimmed.replace(['\t', '\r', '\n'], "");
    let clean = clean.as_str();
    let prefix = clean.replace('\\', "/");
    if clean.starts_with('#') || (!clean.contains(':') && !prefix.starts_with("//")) {
        return opts.allow_internal;
    }
    if let Some(colon) = clean.find(':') {
        if clean[..colon].contains(['/', '?', '#']) {
            return opts.allow_internal;
        }
    }
    let (scheme, rest) = if let Some(rest) = prefix.strip_prefix("//") {
        ("http", rest)
    } else if let Some((scheme, rest)) = clean.split_once(':') {
        (scheme, rest.trim_start_matches('/'))
    } else {
        return opts.allow_internal;
    };
    let scheme = scheme.to_ascii_lowercase();
    if opts
        .denied_schemes
        .iter()
        .any(|s| s.eq_ignore_ascii_case(&scheme))
        || opts
            .allowed_schemes
            .as_ref()
            .is_some_and(|list| !list.iter().any(|s| s.eq_ignore_ascii_case(&scheme)))
    {
        return false;
    }
    if !matches!(scheme.as_str(), "http" | "https") {
        return true;
    }
    let authority = rest
        .split(['/', '\\', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("");
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed
            .find(']')
            .and_then(|end| authority.get(..end + 2))
            .unwrap_or(authority)
    } else {
        authority.split(':').next().unwrap_or("")
    }
    .trim_end_matches('.');
    let host = canonical_host(host);
    if domain(&host, &opts.denied_domains)
        || opts
            .allowed_domains
            .as_ref()
            .is_some_and(|list| !domain(&host, list))
    {
        return false;
    }
    opts.allow_external
        || opts
            .base_host
            .as_ref()
            .is_some_and(|base| host == canonical_host(base))
}
struct LinkPolicy(LinkPolicyOptions);
impl DocumentTransform for LinkPolicy {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        edit(&view, |doc| {
            rewrite(doc, &mut Vec::new(), &mut |node, _| {
                let ty = node.get("type").and_then(Value::as_str)?;
                if !matches!(ty, "link" | "autolink" | "image") {
                    return None;
                }
                let href = node
                    .get(if ty == "image" { "src" } else { "href" })
                    .and_then(Value::as_str)?;
                if !link_allowed(href, &self.0) {
                    return Some(match ty {
                        "link" => node
                            .get("children")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default(),
                        "image" => node
                            .get("alt")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map_or_else(Vec::new, |s| vec![json!({"type":"text","value":s})]),
                        _ => vec![
                            json!({"type":"text","value":node.get("text").and_then(Value::as_str).unwrap_or(href)}),
                        ],
                    });
                }
                if ty == "image" || self.0.rel.is_empty() {
                    return None;
                }
                let mut link = node.as_object().cloned().unwrap_or_default();
                if ty == "autolink" {
                    link.insert("type".into(), "link".into());
                    link.insert("children".into(), json!([{"type":"text","value":node.get("text").and_then(Value::as_str).unwrap_or(href)}]));
                }
                let a = attrs(&mut link)?;
                let existing = a
                    .get("keyValues")
                    .and_then(Value::as_array)
                    .and_then(|kv| {
                        kv.iter().find(|v| {
                            v.get(0)
                                .and_then(Value::as_str)
                                .is_some_and(|k| k.eq_ignore_ascii_case("rel"))
                        })
                    })
                    .and_then(|v| v.get(1))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mut parts: Vec<String> =
                    existing.split_whitespace().map(str::to_string).collect();
                for value in &self.0.rel {
                    if !parts.contains(value) {
                        parts.push(value.clone());
                    }
                }
                key(a, "rel", &parts.join(" "));
                Some(vec![Value::Object(link)])
            });
        })
    }
}
#[must_use]
pub fn link_policy(options: LinkPolicyOptions) -> Bundle {
    bundle("carve.linkPolicy", LinkPolicy(options))
}
