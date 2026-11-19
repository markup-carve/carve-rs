//! PROVISIONAL: the standard extensions of `data/extensions.json`, the
//! `carve.tabs` and `carve.tab` pair (C2-X-039 to C2-X-048, C2-X-056). The
//! claim runs as a separate pass over the parsed tree (C2-X-005).

use super::{Error, Tree, WriteOptions};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Write as _;

/// The standard extensions a host enables, and whether their standard
/// renderers are registered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Standard {
    pub tabs: bool,
    pub tabs_renderer: bool,
}

impl Standard {
    pub(crate) fn enable(&mut self, name: &str, renderer: bool) -> Result<(), Error> {
        if name != "carve.tabs" {
            return Err(Error::option(
                "extensions",
                format!("{name} is not a standard extension of this engine"),
            ));
        }
        self.tabs = true;
        self.tabs_renderer |= renderer;
        Ok(())
    }

    pub(crate) const fn is_empty(self) -> bool {
        !self.tabs
    }
}

/// C2-X-039 to C2-X-042: claims each `tabs` div whose body is one or more
/// `tab` divs with a nonempty quoted title. `opener` gives the source name of
/// the div at a byte offset; `false` means the claim source could not be
/// written and the core node stays (C2-X-051).
pub(crate) fn claim_tabs(doc: &mut Value, opener: &dyn Fn(usize) -> Option<String>) -> bool {
    let definitions: Vec<Value> = at(doc, "/children")
        .as_array()
        .map(|c| {
            c.iter()
                .filter(|n| at(n, "/type") == "link_reference_definition")
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let cx = Claim {
        opener,
        definitions,
        failed: RefCell::new(false),
    };
    let mut claimed = false;
    if let Some(children) = doc.get_mut("children") {
        claimed = cx.visit(children);
    }
    if claimed {
        declare(doc);
    }
    !*cx.failed.borrow()
}

struct Claim<'a> {
    opener: &'a dyn Fn(usize) -> Option<String>,
    definitions: Vec<Value>,
    failed: RefCell<bool>,
}

impl Claim<'_> {
    fn name(&self, node: &Value) -> Option<String> {
        if at(node, "/type") != "div" {
            return None;
        }
        let offset = at(node, "/pos/startOffset").as_u64()?;
        (self.opener)(usize::try_from(offset).ok()?)
    }

    fn is_tab(&self, node: &Value) -> bool {
        self.name(node).as_deref() == Some("tab")
            && at(node, "/title")
                .as_array()
                .is_some_and(|t| !plain_text(t).trim().is_empty())
    }

    /// Claims in every block list below `v`; true when a claim was made.
    fn visit(&self, v: &mut Value) -> bool {
        let mut claimed = false;
        match v {
            Value::Array(items) => {
                for item in items.iter_mut() {
                    if let Some(next) = self.tabs(item) {
                        *item = next;
                        claimed = true;
                    }
                    claimed |= self.visit(item);
                }
            }
            Value::Object(map) => {
                let extension = matches!(
                    map.get("type").and_then(Value::as_str),
                    Some("block_extension" | "inline_extension")
                );
                for (key, child) in map.iter_mut() {
                    if (extension && key == "fallback") || key == "source" || key == "pos" {
                        continue;
                    }
                    claimed |= self.visit(child);
                }
            }
            _ => {}
        }
        claimed
    }

    fn tabs(&self, node: &Value) -> Option<Value> {
        if self.name(node).as_deref() != Some("tabs") {
            return None;
        }
        let body = at(node, "/children").as_array()?;
        if body.is_empty() || !body.iter().all(|n| self.is_tab(n)) {
            return None;
        }
        let Some(source) = self.source(node, "tabs") else {
            *self.failed.borrow_mut() = true;
            return None;
        };
        let mut tabs = Vec::new();
        for tab in body {
            let Some(source) = self.source(tab, "tab") else {
                *self.failed.borrow_mut() = true;
                return None;
            };
            tabs.push(tab_node(tab, source));
        }
        let fallback: Vec<Value> = tabs
            .iter()
            .flat_map(|t| at(t, "/fallback").as_array().cloned().unwrap_or_default())
            .collect();
        let mut out = Map::new();
        out.insert("type".into(), "block_extension".into());
        out.insert("name".into(), "carve.tabs".into());
        out.insert("version".into(), "1".into());
        out.insert("children".into(), json!([]));
        out.insert("slots".into(), json!({ "tabs": tabs }));
        out.insert("fallback".into(), Value::Array(fallback));
        out.insert("source".into(), source);
        finish(&mut out, node);
        Some(Value::Object(out))
    }

    /// C2-X-026, C2-W-089: the canonical body of the div, written with the
    /// link reference definitions of the document around it.
    fn source(&self, node: &Value, name: &str) -> Option<Value> {
        let mut children = vec![node.clone()];
        children.extend(self.definitions.iter().cloned());
        let doc = json!({"type": "document", "astVersion": "2.0", "children": children});
        let tree = Tree::from_value(&doc).ok()?;
        let text = super::Carve::default()
            .write(&tree, &WriteOptions::default())
            .ok()?;
        let lines: Vec<&str> = text.split('\n').collect();
        let open = lines.iter().position(|l| l.starts_with(":::"))?;
        let opener = lines.get(open)?;
        let run = opener.get(..opener.bytes().take_while(|&b| b == b':').count())?;
        let close = lines.iter().rposition(|l| *l == run)?;
        let body = lines.get(open + 1..close)?.join("\n");
        let mut source = Map::new();
        source.insert("form".into(), "div".into());
        source.insert("name".into(), name.into());
        if let Some(title) = at(node, "/title").as_array() {
            source.insert("title".into(), plain_text(title).into());
        }
        if let Some(label) = node.get("groupLabel") {
            source.insert("groupLabel".into(), label.clone());
        }
        let body = if body.is_empty() { body } else { body + "\n" };
        source.insert("body".into(), body.into());
        Some(Value::Object(source))
    }
}

fn tab_node(node: &Value, source: Value) -> Value {
    let title = at(node, "/title").as_array().cloned().unwrap_or_default();
    let children = at(node, "/children")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut fallback = Vec::new();
    if !title.is_empty() {
        fallback.push(
            json!({"type": "paragraph", "children": [{"type": "strong", "children": title}]}),
        );
    }
    fallback.extend(children.iter().cloned());
    let mut out = Map::new();
    out.insert("type".into(), "block_extension".into());
    out.insert("name".into(), "carve.tab".into());
    out.insert("version".into(), "1".into());
    out.insert("children".into(), json!([]));
    out.insert("slots".into(), json!({ "title": title, "body": children }));
    out.insert("fallback".into(), Value::Array(fallback));
    out.insert("source".into(), source);
    finish(&mut out, node);
    Value::Object(out)
}

/// The position and the attributes left after the source name (C2-X-009).
fn finish(out: &mut Map<String, Value>, node: &Value) {
    if let Some(pos) = node.get("pos") {
        out.insert("pos".into(), pos.clone());
    }
    let Some(attrs) = node.get("attrs").and_then(Value::as_object) else {
        return;
    };
    let mut rest = attrs.clone();
    if let Some(classes) = rest.get_mut("classes").and_then(Value::as_array_mut) {
        if !classes.is_empty() {
            classes.remove(0);
        }
        if classes.is_empty() {
            rest.remove("classes");
        }
    }
    if !rest.is_empty() {
        out.insert("attrs".into(), Value::Object(rest));
    }
}

/// C2-A-017: the root declares each qualified extension once, sorted by name.
fn declare(doc: &mut Value) {
    let mut list: Vec<Value> = at(doc, "/extensions")
        .as_array()
        .cloned()
        .unwrap_or_default();
    for (name, slots) in [
        ("carve.tab", json!(["title", "body"])),
        ("carve.tabs", json!(["tabs"])),
    ] {
        if !list.iter().any(|d| at(d, "/name") == name) {
            list.push(json!({"name": name, "version": "1", "required": false, "slots": slots}));
        }
    }
    list.sort_by(|a, b| at(a, "/name").as_str().cmp(&at(b, "/name").as_str()));
    if let Some(map) = doc.as_object_mut() {
        map.insert("extensions".into(), Value::Array(list));
    }
}

fn at<'v>(v: &'v Value, pointer: &str) -> &'v Value {
    v.pointer(pointer).unwrap_or(&Value::Null)
}

/// The plain text of inline nodes, as an accessible name reads it.
pub(crate) fn plain_text(nodes: &[Value]) -> String {
    let mut out = String::new();
    let mut pending: Vec<&Value> = nodes.iter().rev().collect();
    while let Some(n) = pending.pop() {
        if let Some(v) = at(n, "/value").as_str() {
            out.push_str(v);
        }
        if let Some(children) = at(n, "/children").as_array() {
            pending.extend(children.iter().rev());
        }
    }
    out
}

pub(crate) fn is_attribute_line(line: &str) -> bool {
    line.trim_start_matches([' ', '\t', '>'])
        .trim_start_matches([' ', '\t'])
        .starts_with('{')
}

/// The source name of a div whose opener starts at `at`: the word after
/// the colon run (C2-X-008).
pub(crate) fn opener_name(source: &str, at: usize) -> Option<String> {
    // The span of a div with attributes starts at its attribute lines.
    let line = source.get(at..)?.lines().find(|l| !is_attribute_line(l))?;
    let line = line
        .trim_start_matches([' ', '\t', '>'])
        .trim_start_matches([' ', '\t']);
    let rest = line.trim_start_matches(':');
    if rest.len() + 3 > line.len() {
        return None;
    }
    let rest = rest.trim_start_matches([' ', '\t']);
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The ids a generated control id must avoid (C2-X-048): authored ids and
/// generated heading ids, and every prefix of them that ends before a `-`.
#[derive(Debug, Default)]
pub(crate) struct UsedIds {
    ids: HashSet<String>,
    prefixes: HashSet<String>,
    count: usize,
}

impl UsedIds {
    pub(crate) fn of(view: &Value) -> Self {
        let mut used = Self::default();
        let mut pending = vec![view];
        while let Some(v) = pending.pop() {
            match v {
                Value::Object(map) => {
                    if let Some(id) = map
                        .get("attrs")
                        .and_then(|a| a.get("id"))
                        .and_then(Value::as_str)
                    {
                        used.add(id);
                    }
                    if map.get("type").is_some_and(|t| t == "heading")
                        && let Some(id) = map.get("generatedId").and_then(Value::as_str)
                    {
                        used.add(id);
                    }
                    pending.extend(map.values());
                }
                Value::Array(items) => pending.extend(items),
                _ => {}
            }
        }
        used
    }

    fn add(&mut self, id: &str) {
        self.ids.insert(id.to_string());
        for (at, _) in id.match_indices('-') {
            self.prefixes.insert(id[..at].to_string());
        }
    }

    pub(crate) fn next(&mut self, prefix: &str) -> String {
        loop {
            self.count += 1;
            let candidate = format!("{prefix}-{}", self.count);
            if !self.ids.contains(&candidate) && !self.prefixes.contains(&candidate) {
                self.add(&candidate);
                return candidate;
            }
        }
    }
}

/// The standard tabs renderer: radio controls in `interactive` mode
/// (C2-X-043 to C2-X-048), sections in `static` mode (C2-X-056, ruling D6).
pub(crate) fn render_tabs(
    node: &Value,
    cx: &super::RenderContext<'_>,
    group: &dyn Fn() -> String,
) -> Result<String, Error> {
    let tabs = at(node, "/slots/tabs")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let label = cx
        .options()
        .labels
        .get("tabsGroup")
        .map_or("Tabs", String::as_str);
    let mut out = format!(
        r#"<div class="tabs" role="group" aria-label="{}">"#,
        cx.escape_attribute(label)
    );
    let titles: Vec<(Vec<Value>, String)> = tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let title = at(tab, "/slots/title")
                .as_array()
                .cloned()
                .unwrap_or_default();
            let name = if title.is_empty() {
                format!("Tab {}", i + 1)
            } else {
                plain_text(&title)
            };
            (title, name)
        })
        .collect();
    let label_html = |title: &[Value], name: &str| -> Result<String, Error> {
        if title.is_empty() {
            Ok(cx.escape(name))
        } else {
            cx.render_inlines(&Value::Array(title.to_vec()))
        }
    };
    if cx.mode() == crate::options::Mode::Static {
        for (tab, (title, name)) in tabs.iter().zip(&titles) {
            let _ = write!(
                out,
                r#"<section class="tabs-panel"><h3 class="tabs-label">{}</h3>{}</section>"#,
                label_html(title, name)?,
                cx.render_blocks(at(tab, "/slots/body"))?
            );
        }
        out.push_str("</div>");
        return Ok(out);
    }
    let selected = tabs
        .iter()
        .position(|t| {
            at(t, "/attrs/keyValues")
                .as_array()
                .is_some_and(|kv| kv.iter().any(|p| p.get(0).is_some_and(|k| k == "selected")))
        })
        .unwrap_or(0);
    let group = group();
    let mut panels = String::new();
    for (i, (tab, (title, name))) in tabs.iter().zip(&titles).enumerate() {
        let id = format!("{group}-{i}");
        let _ = write!(
            out,
            r#"<input type="radio" name="{group}" id="{id}"{}><label for="{id}">{}</label>"#,
            if i == selected { " checked" } else { "" },
            label_html(title, name)?
        );
        let _ = write!(
            panels,
            r#"<div class="tab-panel" role="group" aria-label="{}">{}</div>"#,
            cx.escape_attribute(name),
            cx.render_blocks(at(tab, "/slots/body"))?
        );
    }
    out.push_str(&panels);
    out.push_str("</div>");
    Ok(out)
}
