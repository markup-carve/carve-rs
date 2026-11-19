//! PROVISIONAL: the extension host of `extensions.md` (C2-X). A host registers
//! an [`Extension`]: its declaration, renderers per render channel (C2-X-052)
//! and an optional document transform (C2-X-053). The engine has no standard
//! extensions yet, so only host code registers. These types may change before
//! 0.2 is final.

use super::{Entry, Error, Options, Tree};
use crate::options::Mode;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

/// An extension declaration (C2-A-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionDeclaration {
    /// The qualified name (C2-A-123).
    pub name: String,
    pub version: String,
    pub required: bool,
    pub slots: Vec<String>,
}

/// The render channel a renderer registers for (C2-X-052).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Selected under `mode: interactive`.
    Interactive,
    /// The build-time channel, selected under `mode: static`.
    BuildTime,
}

impl Channel {
    const fn of(mode: Mode) -> Self {
        match mode {
            Mode::Interactive => Self::Interactive,
            Mode::Static => Self::BuildTime,
        }
    }
}

/// What a renderer receives beside the node (C2-X-033).
pub struct RenderContext<'a> {
    host: &'a Host<'a>,
    /// Whether the node is a block extension.
    pub block: bool,
    depth: usize,
}

impl RenderContext<'_> {
    /// The render options of the call, `mode`, `profile` and `urlPolicy`
    /// among them.
    #[must_use]
    pub fn options(&self) -> &Options {
        self.host.opts
    }

    /// The indentation level of the node's block, in two-space units
    /// (C2-H-042): a renderer that writes several lines indents the lines
    /// after the first by it.
    #[must_use]
    pub fn block_depth(&self) -> usize {
        self.depth
    }

    /// The render mode (C2-H-122).
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.host.opts.mode
    }

    /// Escapes text for an element body.
    #[must_use]
    pub fn escape(&self, text: &str) -> String {
        crate::html::esc::text_str(text)
    }

    /// Escapes text for a double-quoted attribute value.
    #[must_use]
    pub fn escape_attribute(&self, text: &str) -> String {
        let mut out = String::new();
        crate::html::esc::attr(text, &mut out);
        out
    }

    /// The HTML of a list of block nodes, such as a block slot, written with
    /// the options and renderers of the call.
    ///
    /// # Errors
    ///
    /// [`Error::Tree`] for nodes the reader refuses.
    pub fn render_blocks(&self, nodes: &Value) -> Result<String, Error> {
        self.host.render_list(nodes)
    }

    /// The plain text of inline nodes, as an accessible name reads it.
    #[must_use]
    pub fn plain_text(&self, nodes: &[Value]) -> String {
        super::standard::plain_text(nodes)
    }

    /// The `labels` render option (C2-H-125).
    #[must_use]
    pub fn label(&self, key: &str) -> Option<&str> {
        self.host.opts.labels.get(key).map(String::as_str)
    }

    /// A control id of the form `{prefix}-{n}` that no authored id, generated
    /// heading id or earlier control id of the render takes (C2-X-048).
    #[must_use]
    pub fn next_id(&self, prefix: &str) -> String {
        self.host.ids.borrow_mut().next(prefix)
    }

    /// C2-X-033: the attribute list of the element of `node` (its `attrs`
    /// and, for a block under `sourceLines`, its source line), written as
    /// every core element writes it: ` id="..." class="..." ...`.
    #[must_use]
    pub fn attributes(&self, node: &Value) -> String {
        let mut div = serde_json::Map::new();
        div.insert("type".into(), "div".into());
        div.insert("children".into(), Value::Array(Vec::new()));
        // A source line is written on block elements only.
        let keys: &[&str] = if self.block {
            &["attrs", "pos"]
        } else {
            &["attrs"]
        };
        for key in keys {
            if let Some(v) = node.get(*key) {
                div.insert((*key).into(), v.clone());
            }
        }
        let Ok(html) = self
            .host
            .render_list(&Value::Array(vec![Value::Object(div)]))
        else {
            return String::new();
        };
        open_tag_attributes(&html, "div").unwrap_or_default()
    }

    /// The HTML of a list of inline nodes, such as an inline slot.
    ///
    /// # Errors
    ///
    /// As [`RenderContext::render_blocks`].
    pub fn render_inlines(&self, nodes: &Value) -> Result<String, Error> {
        let paragraph = Value::Array(vec![serde_json::json!({
            "type": "paragraph",
            "children": nodes,
        })]);
        let html = self.host.render_list(&paragraph)?;
        Ok(html
            .strip_prefix("<p>")
            .and_then(|h| h.strip_suffix("</p>"))
            .map_or(html.clone(), str::to_string))
    }
}

/// The attribute text of the first `<tag ...>` of `html`, quoted values skipped.
fn open_tag_attributes(html: &str, tag: &str) -> Option<String> {
    let rest = html.strip_prefix('<')?.strip_prefix(tag)?;
    let mut quoted = false;
    for (i, c) in rest.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(rest[..i].to_string()),
            _ => {}
        }
    }
    None
}

/// A host renderer of extension nodes (C2-X-033, C2-X-034): trusted code
/// whose HTML is written unchanged.
pub trait Renderer: Send + Sync {
    /// Writes the HTML of one extension node, given as its JSON value.
    ///
    /// # Errors
    ///
    /// Any error; the engine writes the fallback and reports
    /// `extension-renderer-failed` (C2-X-050).
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error>;
}

/// A document transform (C2-X-053): takes a copy of the resolved render view
/// and returns a view.
pub trait DocumentTransform: Send + Sync {
    /// # Errors
    ///
    /// Any error; the engine keeps the input view and reports
    /// `extension-transform-failed` (C2-X-055).
    fn transform(&self, view: Tree) -> Result<Tree, Error>;
}

/// One registration: a declaration with its renderers and an optional transform.
pub struct Extension {
    pub declaration: ExtensionDeclaration,
    pub renderers: Vec<(Channel, Box<dyn Renderer>)>,
    pub transform: Option<Box<dyn DocumentTransform>>,
}

impl std::fmt::Debug for Extension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extension")
            .field("declaration", &self.declaration)
            .field("renderers", &self.renderers.len())
            .field("transform", &self.transform.is_some())
            .finish()
    }
}

/// The registered extensions of a host, in registration order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Registry {
    list: Vec<Arc<Extension>>,
}

impl Registry {
    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub(crate) fn declaration(&self, name: &str) -> Option<ExtensionDeclaration> {
        self.list
            .iter()
            .find(|e| e.declaration.name == name)
            .map(|e| e.declaration.clone())
    }

    pub(crate) fn register(&mut self, extension: Extension) -> Result<(), Error> {
        let d = &extension.declaration;
        if !d.name.contains('.') || d.name.starts_with('.') || d.name.ends_with('.') {
            return Err(Error::option(
                "extensions",
                format!("{} is not a qualified extension name", d.name),
            ));
        }
        if d.version.is_empty() {
            return Err(Error::option(
                "extensions",
                format!("{} has no version", d.name),
            ));
        }
        if self
            .list
            .iter()
            .any(|e| e.declaration.name == d.name && e.declaration.version == d.version)
        {
            return Err(Error::option(
                "extensions",
                format!("{} {} is already registered", d.name, d.version),
            ));
        }
        for channel in [Channel::Interactive, Channel::BuildTime] {
            if extension
                .renderers
                .iter()
                .filter(|(c, _)| *c == channel)
                .count()
                > 1
            {
                return Err(Error::option(
                    "extensions",
                    format!("{} registers two renderers for one channel", d.name),
                ));
            }
        }
        self.list.push(Arc::new(extension));
        Ok(())
    }

    /// C2-X-053 to C2-X-055: runs the transforms in registration order, each
    /// on a copy of the previous view; a failed transform leaves its input.
    pub(crate) fn transform(&self, mut view: Tree) -> (Tree, Vec<Entry>) {
        let mut entries = Vec::new();
        for ext in &self.list {
            let Some(transform) = &ext.transform else {
                continue;
            };
            let failure = match transform.transform(view.clone()) {
                Ok(next) if duplicate_ids(&next) > duplicate_ids(&view) => {
                    Some("an inserted node takes an id a preceding node holds".to_string())
                }
                Ok(next) => {
                    view = next;
                    None
                }
                Err(e) => Some(e.to_string()),
            };
            if let Some(why) = failure {
                entries.push(failure_entry(
                    "extension-transform-failed",
                    &format!(
                        "The document transform of the extension \"{}\" failed: {why}",
                        ext.declaration.name
                    ),
                ));
            }
        }
        (view, entries)
    }
}

fn failure_entry(code: &str, message: &str) -> Entry {
    Entry {
        code: code.to_string(),
        severity: "warning".to_string(),
        channel: "render-loss".to_string(),
        message: message.to_string(),
        path: None,
        range: None,
        file: None,
        foreign_type: None,
    }
}

/// The authored ids of a view that a preceding node already holds.
fn duplicate_ids(tree: &Tree) -> usize {
    let Ok(value) = crate::api::deep_json(&super::encode_tree(tree)) else {
        return 0;
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut duplicates = 0;
    let mut pending = vec![&value];
    while let Some(v) = pending.pop() {
        match v {
            Value::Object(map) => {
                if let Some(id) = map
                    .get("attrs")
                    .and_then(|a| a.get("id"))
                    .and_then(Value::as_str)
                    && !seen.insert(id.to_string())
                {
                    duplicates += 1;
                }
                pending.extend(map.values().rev());
            }
            Value::Array(items) => pending.extend(items.iter().rev()),
            _ => {}
        }
    }
    duplicates
}

/// The renderers of one render call.
pub(crate) struct Host<'a> {
    registry: &'a Registry,
    standard: super::standard::Standard,
    fields: &'a super::extensions::HostFields,
    opts: &'a Options,
    failed: RefCell<Vec<String>>,
    ids: RefCell<super::standard::UsedIds>,
    /// The declarations of the view, so a slot's extension nodes read back.
    declared: Value,
}

impl<'a> Host<'a> {
    pub(crate) fn new(
        registry: &'a Registry,
        standard: super::standard::Standard,
        fields: &'a super::extensions::HostFields,
        opts: &'a Options,
        view: &Tree,
    ) -> Self {
        let value = crate::api::deep_json(&super::encode_tree(view)).unwrap_or(Value::Null);
        Self {
            registry,
            standard,
            fields,
            opts,
            failed: RefCell::new(Vec::new()),
            ids: RefCell::new(super::standard::UsedIds::of(&value)),
            declared: value.get("extensions").cloned().unwrap_or(Value::Null),
        }
    }

    /// The `extension-renderer-failed` entries of the call (C2-X-050).
    pub(crate) fn failures(&self) -> Vec<Entry> {
        self.failed
            .borrow()
            .iter()
            .map(|name| {
                failure_entry(
                    "extension-renderer-failed",
                    &format!(
                        "The renderer of the extension \"{name}\" failed; its fallback was written unresolved"
                    ),
                )
            })
            .collect()
    }

    fn render_list(&self, nodes: &Value) -> Result<String, Error> {
        let mut doc = serde_json::json!({
            "type": "document",
            "astVersion": "2.0",
            "children": nodes,
        });
        if let (true, Some(map)) = (self.declared.is_array(), doc.as_object_mut()) {
            map.insert("extensions".into(), self.declared.clone());
        }
        // C2-X-034: a renderer is trusted host code, and its HTML is written
        // unchanged, so the nodes it renders need no reader checks either.
        let tree = Tree::trusted(doc, None)?;
        Ok(crate::render_tree_hooked(tree.document(), self.opts, self).html)
    }
}

impl crate::html::ExtensionHook for Host<'_> {
    fn semantic_keys(&self) -> Option<super::extensions::SemanticKeys> {
        self.fields.semantic_keys
    }

    fn quotes(&self) -> Option<&[String; 4]> {
        self.fields.quotes.as_ref()
    }

    fn render(&self, node: &Value, block: bool, depth: usize) -> Option<Result<String, ()>> {
        let name = node.get("name").and_then(Value::as_str)?;
        let version = node.get("version").and_then(Value::as_str);
        if name == "carve.tabs" && self.standard.tabs_renderer && version == Some("1") {
            let cx = RenderContext {
                host: self,
                block,
                depth,
            };
            let group = || self.ids.borrow_mut().next("tabs");
            return Some(
                super::standard::render_tabs(node, &cx, &group, self.standard.tabs_aria).map_err(
                    |_| {
                        self.failed.borrow_mut().push(name.to_string());
                    },
                ),
            );
        }
        if self.standard.renders(name) && version == Some("1") {
            return Some(super::img_fence::render(node).ok_or_else(|| {
                self.failed.borrow_mut().push(name.to_string());
            }));
        }
        let channel = Channel::of(self.opts.mode);
        let renderer = self
            .registry
            .list
            .iter()
            .filter(|e| e.declaration.name == name)
            .filter(|e| version.is_none_or(|v| v == e.declaration.version))
            .find_map(|e| e.renderers.iter().find(|(c, _)| *c == channel))
            .map(|(_, r)| r)?;
        let cx = RenderContext {
            host: self,
            block,
            depth,
        };
        Some(renderer.render(node, &cx).map_err(|_| {
            self.failed.borrow_mut().push(name.to_string());
        }))
    }
}

/// A transform with no host code: inserts `node` after the node at the JSON
/// pointer `after` (the `documentTransforms` form of the conformance host).
#[derive(Debug, Clone, PartialEq)]
pub struct InsertTransform {
    pub after: String,
    pub node: Value,
}

impl DocumentTransform for InsertTransform {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        let mut value: Value = crate::api::deep_json(&super::encode_tree(&view))
            .map_err(|e| Error::option("documentTransforms", e.to_string()))?;
        let missing = || Error::option("documentTransforms", format!("no node at {}", self.after));
        let (parent, index) = self.after.rsplit_once('/').ok_or_else(missing)?;
        let index: usize = index.parse().map_err(|_| missing())?;
        let list = value
            .pointer_mut(parent)
            .and_then(Value::as_array_mut)
            .filter(|l| index < l.len())
            .ok_or_else(missing)?;
        list.insert(index + 1, self.node.clone());
        let mut next = Tree::from_value(&value)?;
        next.set_file(view.top.clone());
        Ok(next)
    }
}
