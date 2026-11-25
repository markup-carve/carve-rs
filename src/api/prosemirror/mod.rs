//! The ProseMirror / Tiptap bridge: a 0.2 tree to a ProseMirror document and
//! back, in the schema of the carve-grammars Tiptap kit (`tiptap/tree.js`).
//! A port of the carve-php bridge, so the engines and the kit read what each
//! other saved. The README section "ProseMirror / Tiptap bridge" has the calls.
//!
//! ```
//! use carve::api::{self, Options, WriteOptions};
//! use carve::api::prosemirror::{prosemirror_to_carve, to_prosemirror};
//!
//! let tree = api::parse("Some *bold* text.\n", &Options::default())?;
//! let doc = to_prosemirror(&tree)?;
//! let back = prosemirror_to_carve(&doc.json, &WriteOptions::default())?;
//! assert_eq!(back, "Some *bold* text.\n");
//! # Ok::<(), carve::api::prosemirror::ProseMirrorError>(())
//! ```

// ProseMirror and Tiptap are product names, not code.
#![allow(clippy::doc_markdown)]

mod kit;
mod render;
mod to_tree;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde_json::{Map, Value};

use super::{Error, Tree, UrlPolicy, WriteOptions};

/// Why the bridge refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProseMirrorError {
    /// The input is not a ProseMirror document of the kit's schema, or a
    /// `mapping` entry is malformed.
    Invalid(String),
    /// An engine call refused (the tree reader, the Carve writer).
    Carve(Error),
}

impl fmt::Display for ProseMirrorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Carve(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ProseMirrorError {}

impl From<Error> for ProseMirrorError {
    fn from(error: Error) -> Self {
        Self::Carve(error)
    }
}

/// Where a mapped node stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Block,
    Inline,
}

/// What a mapping callback can call back into: the conversion of the node's
/// content, `blocks` and `inlines`.
pub trait Context {
    /// Where the node stands.
    fn position(&self) -> Position;
    /// The children converted as blocks. Saving, they are ProseMirror nodes
    /// and the result tree nodes; loading, the other way round.
    ///
    /// # Errors
    ///
    /// As the conversion.
    fn blocks(&mut self, children: &Value) -> Result<Vec<Value>, ProseMirrorError>;
    /// The children converted as inlines.
    ///
    /// # Errors
    ///
    /// As the conversion.
    fn inlines(&mut self, children: &Value) -> Result<Vec<Value>, ProseMirrorError>;
}

/// A `toTree` callback: the ProseMirror node (or mark) to a tree node. For
/// a mark it returns the inline container without `children`.
pub type ToTree =
    Arc<dyn Fn(&Value, &mut dyn Context) -> Result<Value, ProseMirrorError> + Send + Sync>;
/// A `fromTree` callback: the tree node to the ProseMirror node (or mark),
/// or `None` for a tree node that is not its own.
pub type FromTree =
    Arc<dyn Fn(&Value, &mut dyn Context) -> Result<Option<Value>, ProseMirrorError> + Send + Sync>;

/// One entry of [`Mapping`].
#[derive(Clone)]
pub enum MappingEntry {
    /// A node or mark saved as a span or div of one class and key/value
    /// pairs.
    Declared {
        /// `span` (inline) or `div` (block). A mark is always a `span`.
        as_: String,
        /// The one class that names the type in Carve.
        class: String,
        /// The ProseMirror attributes to keep.
        attributes: Vec<String>,
        /// Whether the node has content; without it the node is an atom.
        content: bool,
        /// Whether the editor's node type has a `carve` attribute.
        carve: bool,
    },
    /// A pair of callbacks; either may be missing.
    Callback {
        to_tree: Option<ToTree>,
        from_tree: Option<FromTree>,
        carve: bool,
    },
}

impl fmt::Debug for MappingEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declared {
                as_,
                class,
                attributes,
                content,
                carve,
            } => f
                .debug_struct("Declared")
                .field("as", as_)
                .field("class", class)
                .field("attributes", attributes)
                .field("content", content)
                .field("carve", carve)
                .finish(),
            Self::Callback {
                to_tree,
                from_tree,
                carve,
            } => f
                .debug_struct("Callback")
                .field("to_tree", &to_tree.is_some())
                .field("from_tree", &from_tree.is_some())
                .field("carve", carve)
                .finish(),
        }
    }
}

impl MappingEntry {
    fn carve(&self) -> bool {
        match self {
            Self::Declared { carve, .. } | Self::Callback { carve, .. } => *carve,
        }
    }

    fn declared_as(&self) -> Option<&str> {
        match self {
            Self::Declared { as_, .. } => Some(as_),
            Self::Callback { .. } => None,
        }
    }
}

/// The host's own ProseMirror nodes and marks, each name to an entry, in
/// the order they are tried.
#[derive(Debug, Clone, Default)]
pub struct Mapping {
    pub nodes: Vec<(String, MappingEntry)>,
    pub marks: Vec<(String, MappingEntry)>,
}

impl Mapping {
    /// The mapping of its JSON form, `{"nodes": {...}, "marks": {...}}`,
    /// with declared entries only (`as`, `class`, `attributes`, `content`,
    /// `carve`).
    ///
    /// # Errors
    ///
    /// [`ProseMirrorError::Invalid`] for a malformed entry.
    pub fn from_json(value: &Value) -> Result<Self, ProseMirrorError> {
        let mut out = Self::default();
        if value.is_null() {
            return Ok(out);
        }
        let Value::Object(root) = value else {
            return Err(invalid(
                "mapping: expected an object with \"nodes\" and \"marks\"",
            ));
        };
        for kind in ["nodes", "marks"] {
            let entries = match root.get(kind) {
                None | Some(Value::Null) => continue,
                // PHP writes an empty map as `[]`.
                Some(Value::Array(a)) if a.is_empty() => continue,
                Some(Value::Object(m)) => m,
                Some(_) => return Err(invalid(format!("mapping: \"{kind}\" must be an object"))),
            };
            for (name, entry) in entries {
                let entry = declared_entry(kind, name, entry)?;
                if kind == "nodes" {
                    out.nodes.push((name.clone(), entry));
                } else {
                    out.marks.push((name.clone(), entry));
                }
            }
        }
        Ok(out)
    }

    /// Checks every declared entry, as [`Mapping::from_json`] does.
    ///
    /// # Errors
    ///
    /// [`ProseMirrorError::Invalid`] for a malformed entry.
    pub fn check(&self) -> Result<(), ProseMirrorError> {
        for (kind, entries) in [("nodes", &self.nodes), ("marks", &self.marks)] {
            for (name, entry) in entries {
                if let MappingEntry::Declared { as_, class, .. } = entry {
                    let allowed: &[&str] = if kind == "marks" {
                        &["span"]
                    } else {
                        &["span", "div"]
                    };
                    if !allowed.contains(&as_.as_str()) || class.is_empty() {
                        return Err(entry_error(kind, name));
                    }
                }
            }
        }
        Ok(())
    }

    fn node(&self, name: &str) -> Option<&MappingEntry> {
        self.nodes.iter().find(|(n, _)| n == name).map(|(_, e)| e)
    }

    fn mark(&self, name: &str) -> Option<&MappingEntry> {
        self.marks.iter().find(|(n, _)| n == name).map(|(_, e)| e)
    }
}

fn entry_error(kind: &str, name: &str) -> ProseMirrorError {
    let (what, allowed) = if kind == "marks" {
        ("mark", "span")
    } else {
        ("node", "span or div")
    };
    invalid(format!(
        "mapping: the {what} \"{name}\" needs \"as\" ({allowed}), a \"class\" and string \"attributes\", or \"toTree\" and \"fromTree\""
    ))
}

fn declared_entry(kind: &str, name: &str, entry: &Value) -> Result<MappingEntry, ProseMirrorError> {
    let empty = Map::new();
    let entry = entry.as_object().unwrap_or(&empty);
    let carve = match entry.get("carve") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            return Err(invalid(format!(
                "mapping: \"carve\" of \"{name}\" must be a boolean"
            )));
        }
    };
    let allowed: &[&str] = if kind == "marks" {
        &["span"]
    } else {
        &["span", "div"]
    };
    let as_ = entry.get("as").and_then(Value::as_str);
    let class = entry.get("class").and_then(Value::as_str);
    let attributes = match entry.get("attributes") {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>(),
        Some(_) => None,
    };
    let content = match entry.get("content") {
        None | Some(Value::Null) => Some(false),
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => None,
    };
    match (as_, class, attributes, content) {
        (Some(as_), Some(class), Some(attributes), Some(content))
            if allowed.contains(&as_) && !class.is_empty() =>
        {
            Ok(MappingEntry::Declared {
                as_: as_.to_string(),
                class: class.to_string(),
                attributes,
                content,
                carve,
            })
        }
        _ => Err(entry_error(kind, name)),
    }
}

fn invalid(message: impl Into<String>) -> ProseMirrorError {
    ProseMirrorError::Invalid(message.into())
}

/// The bridge options.
#[derive(Debug, Clone, Default)]
pub struct ProseMirrorOptions {
    /// The URL policy of the editor's link and image attributes; a denied
    /// destination is null there and kept in `carve` (carve#6009).
    pub url_policy: UrlPolicy,
    /// The host's own nodes and marks.
    pub mapping: Mapping,
}

impl ProseMirrorOptions {
    /// The options of their JSON form, for bindings: `urlPolicy` as the
    /// render option spells it (`{"deny": [...]}` or `{"allow": [...]}`) and
    /// `mapping` as [`Mapping::from_json`] reads it. `null` is the default.
    ///
    /// # Errors
    ///
    /// [`ProseMirrorError::Carve`] for a refused `urlPolicy` or an unknown
    /// key; [`ProseMirrorError::Invalid`] for a malformed mapping.
    pub fn from_json(value: &Value) -> Result<Self, ProseMirrorError> {
        let mut out = Self::default();
        let fields = match value {
            Value::Null => return Ok(out),
            Value::Object(fields) => fields,
            _ => return Err(Error::option("options", "expected an object").into()),
        };
        for (key, field) in fields {
            match key.as_str() {
                "urlPolicy" if !field.is_null() => {
                    out.url_policy = super::options::url_policy(field)?;
                }
                "mapping" => out.mapping = Mapping::from_json(field)?,
                "urlPolicy" => {}
                other => {
                    return Err(
                        Error::option(other, "not an option of the ProseMirror bridge").into(),
                    );
                }
            }
        }
        Ok(out)
    }
}

/// A rendered ProseMirror document.
#[derive(Debug, Clone, PartialEq)]
pub struct ProseMirrorDoc {
    /// The ProseMirror `doc` node.
    pub doc: Value,
    /// The same, as JSON for the editor's `setContent()`.
    pub json: String,
    /// The denied destinations kept out of the editor's link and image
    /// attributes.
    pub blanked: Vec<String>,
}

/// An attribute the save did not read.
#[derive(Debug, Clone, PartialEq)]
pub struct IgnoredAttribute {
    /// The JSON pointer of its node or mark in the saved document.
    pub path: String,
    /// The node or mark type.
    pub type_: String,
    pub attribute: String,
    pub value: Value,
}

/// A saved ProseMirror document as a tree, with the report.
#[derive(Debug, Clone)]
pub struct ProseMirrorImport {
    /// The tree, read and checked.
    pub tree: Tree,
    /// For each top-level block, the index of the loaded block it came from;
    /// `None` for a block the editor added.
    pub origins: Vec<Option<i64>>,
    /// By tree path, the JSON pointer of the ProseMirror node or mark each
    /// tree node came from.
    pub pointers: BTreeMap<String, String>,
    /// The attributes the save did not read.
    pub ignored: Vec<IgnoredAttribute>,
}

impl ProseMirrorImport {
    /// The pointer of the ProseMirror node or mark the tree node at `path`
    /// came from, or of its nearest ancestor that came from one.
    #[must_use]
    pub fn pointer_for(&self, path: &str) -> Option<&str> {
        pointer_for(&self.pointers, path)
    }
}

fn pointer_for<'p>(pointers: &'p BTreeMap<String, String>, path: &str) -> Option<&'p str> {
    let mut at = path;
    loop {
        if let Some(p) = pointers.get(at) {
            return Some(p);
        }
        if at.is_empty() {
            return None;
        }
        at = at.rfind('/').map_or("", |i| at.get(..i).unwrap_or(""));
    }
}

/// The ProseMirror document of a tree.
///
/// # Errors
///
/// [`ProseMirrorError::Carve`] when the tree cannot be resolved.
pub fn to_prosemirror(tree: &Tree) -> Result<ProseMirrorDoc, ProseMirrorError> {
    to_prosemirror_with(tree, &ProseMirrorOptions::default())
}

/// [`to_prosemirror`] with options.
///
/// # Errors
///
/// As [`to_prosemirror`], and [`ProseMirrorError::Invalid`] for a malformed
/// mapping or a callback that refuses.
pub fn to_prosemirror_with(
    tree: &Tree,
    options: &ProseMirrorOptions,
) -> Result<ProseMirrorDoc, ProseMirrorError> {
    options.mapping.check()?;
    let value = render::resolved_value(tree)?;
    let (doc, blanked) = render::Renderer::new(options).render(&value)?;
    let json = doc.to_string();
    Ok(ProseMirrorDoc { doc, json, blanked })
}

/// The tree of a ProseMirror document (the editor's `getJSON()`, as JSON).
///
/// # Errors
///
/// [`ProseMirrorError::Invalid`] for a node or mark with no Carve node;
/// [`ProseMirrorError::Carve`] for a tree the reader refuses.
pub fn from_prosemirror(json: &str) -> Result<Tree, ProseMirrorError> {
    Ok(from_prosemirror_with_report(json, &ProseMirrorOptions::default())?.tree)
}

/// [`from_prosemirror`] with options, and the report: origins, pointers and
/// the ignored attributes.
///
/// # Errors
///
/// As [`from_prosemirror`].
pub fn from_prosemirror_with_report(
    json: &str,
    options: &ProseMirrorOptions,
) -> Result<ProseMirrorImport, ProseMirrorError> {
    let doc = super::deep_json(json).map_err(|e| invalid(format!("ProseMirror JSON: {e}")))?;
    from_prosemirror_value(&doc, options)
}

/// [`from_prosemirror_with_report`] of a decoded document.
///
/// # Errors
///
/// As [`from_prosemirror`].
pub fn from_prosemirror_value(
    doc: &Value,
    options: &ProseMirrorOptions,
) -> Result<ProseMirrorImport, ProseMirrorError> {
    options.mapping.check()?;
    let mut saved = to_tree::Saver::new(options).save(doc)?;
    // A loaded role stays in `carve` after an edit; the content decides it.
    to_tree::with_roles(&mut saved.tree);
    let tree = super::decode_tree(&saved.tree.to_string())?;
    Ok(ProseMirrorImport {
        tree,
        origins: saved.origins,
        pointers: saved.pointers,
        ignored: saved.ignored,
    })
}

/// The tree of a ProseMirror document as its JSON value, unchecked: what
/// [`from_prosemirror_value`] reads.
///
/// # Errors
///
/// As [`from_prosemirror`], without the reader.
pub fn prosemirror_tree_value(
    doc: &Value,
    options: &ProseMirrorOptions,
) -> Result<Value, ProseMirrorError> {
    options.mapping.check()?;
    Ok(to_tree::Saver::new(options).save(doc)?.tree)
}

/// The canonical Carve source of a ProseMirror document.
///
/// # Errors
///
/// As [`from_prosemirror`], and [`ProseMirrorError::Carve`] for a writer loss.
pub fn prosemirror_to_carve(json: &str, write: &WriteOptions) -> Result<String, ProseMirrorError> {
    prosemirror_to_carve_with(json, &ProseMirrorOptions::default(), write)
}

/// [`prosemirror_to_carve`] with options.
///
/// # Errors
///
/// As [`prosemirror_to_carve`].
pub fn prosemirror_to_carve_with(
    json: &str,
    options: &ProseMirrorOptions,
    write: &WriteOptions,
) -> Result<String, ProseMirrorError> {
    let doc = super::deep_json(json).map_err(|e| invalid(format!("ProseMirror JSON: {e}")))?;
    options.mapping.check()?;
    let mut tree = to_tree::Saver::new(options).save(&doc)?.tree;
    to_tree::with_roles(&mut tree);
    let tree = super::decode_tree(&tree.to_string())?;
    Ok(super::Carve::new().write(&tree, write)?)
}

/// The Carve tree type of a ProseMirror node or mark name of the kit, or
/// `None` when the kit does not map it. `carveBlock` and `carveInline` carry
/// a whole node of any type and give `None`.
#[must_use]
pub fn carve_type_for(name: &str) -> Option<&'static str> {
    prosemirror_names()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
}

/// Every ProseMirror node and mark name the kit maps to one tree type.
#[must_use]
pub fn prosemirror_names() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    for table in [kit::TEXTBLOCKS, kit::CONTAINERS, kit::MARKS] {
        for (tree, name, _) in table {
            out.push((name, tree));
        }
    }
    for (tree, name) in kit::CODE_BLOCKS {
        out.push((name, tree));
    }
    for pair in [
        ("codeBlock", "code_block"),
        ("horizontalRule", "thematic_break"),
        ("bulletList", "list"),
        ("orderedList", "list"),
        ("taskList", "list"),
        ("listItem", "list_item"),
        ("taskItem", "list_item"),
        ("carveDefinitionList", "definition_list"),
        ("carveLineBlock", "line_block"),
        ("table", "table"),
        ("tableRow", "table_row"),
        ("tableCell", "table_cell"),
        ("tableHeader", "table_cell"),
        ("text", "text"),
        ("code", "code"),
        ("hardBreak", "hard_break"),
        ("carveSoftBreak", "soft_break"),
        ("image", "image"),
        ("carveMath", "math"),
        ("carveFootnoteRef", "footnote_ref"),
    ] {
        if !out.iter().any(|(n, _)| *n == pair.0) {
            out.push(pair);
        }
    }
    out
}
