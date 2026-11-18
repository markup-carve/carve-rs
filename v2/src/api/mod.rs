//! The public library of the Carve 0.2 engine: the calls of `tooling.md`
//! (C2-T-005) in Rust spelling (C2-T-001). This module is the whole contract;
//! the other modules of the crate are engine internals.
//!
//! ```
//! use carve2::api::{self, Options};
//!
//! let html = api::render("Hello *world*", &Options::default())?;
//! assert_eq!(html, "<p>Hello <strong>world</strong></p>");
//!
//! let tree = api::parse("Hello", &Options::default())?;
//! assert!(api::encode_tree(&tree).starts_with(r#"{"type":"document""#));
//! # Ok::<(), api::Error>(())
//! ```
//!
//! | `tooling.md` call | Rust |
//! |---|---|
//! | `parse` | [`parse`], [`Carve::parse`] |
//! | `parseWithDiagnostics` | [`parse_with_diagnostics`] |
//! | `parseWithLayout` | [`parse_with_layout`] (not yet available) |
//! | `render` | [`render`], [`render_tree`] |
//! | `renderWithDiagnostics` | [`render_with_diagnostics`], [`render_tree_with_diagnostics`] |
//! | `format`, `formatWithDiagnostics` | [`Carve::format`], [`Carve::format_with_diagnostics`] |
//! | `write`, `writeWithDiagnostics` | [`Carve::write`], [`Carve::write_with_diagnostics`], [`Carve::write_to`] |
//! | `lint` | [`lint`] |
//! | `importDocument` | [`import_document`] (not yet available) |
//! | `includeDependencies` | [`include_dependencies`] |
//! | `encodeTree`, `encodeEnvelope` | [`encode_tree`], [`encode_tree_with`], [`encode_envelope`] |
//! | `decodeTree`, `validateTree` | [`decode_tree`], [`validate_tree`] |
//! | `migrateTree` | [`migrate_tree`] (not yet available) |

mod error;
pub mod extension;
pub mod json;
mod options;
mod record;
pub mod writer;

pub use crate::ast::owned::Document;

/// The node types of the owned 2.0 tree (`ast.md`), generated from
/// `data/ast-schema.json`.
pub mod ast {
    pub use crate::ast::Pos;
    pub use crate::ast::owned::*;
}
pub use crate::include::{Cause, Dependency, Includes, Request, Resolver};
pub use crate::lint::LintOptions;
pub use crate::options::{HeadingIds, Mode, Options, Profile, Social, UrlPolicy};
pub use error::Error;
pub use extension::{Channel, Extension, ExtensionDeclaration};
pub use record::{Entry, Range};
pub use writer::{HtmlWriter, WriteOptions, Writer, Writers, Written};

use crate::Arena;
use serde_json::Value;

/// Carve source as text or as bytes. Bytes are never refused for their
/// encoding: malformed UTF-8 is replaced and offsets count the input bytes
/// (C2-B-218).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input<'a> {
    Text(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> From<&'a str> for Input<'a> {
    fn from(s: &'a str) -> Self {
        Self::Text(s)
    }
}

impl<'a> From<&'a String> for Input<'a> {
    fn from(s: &'a String) -> Self {
        Self::Text(s)
    }
}

impl<'a> From<&'a [u8]> for Input<'a> {
    fn from(b: &'a [u8]) -> Self {
        Self::Bytes(b)
    }
}

impl<'a> From<&'a Vec<u8>> for Input<'a> {
    fn from(b: &'a Vec<u8>) -> Self {
        Self::Bytes(b)
    }
}

impl Input<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Bytes(b) => b.len(),
        }
    }

    /// The text of the input; malformed UTF-8 is replaced.
    fn text(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Text(s) => std::borrow::Cow::Borrowed(s),
            Self::Bytes(b) => String::from_utf8_lossy(b),
        }
    }
}

/// A resolved 2.0 tree that passed the reader checks (C2-A-188), with its
/// JSON wire form.
#[derive(Debug, Clone)]
pub struct Tree {
    document: Document,
    json: String,
    top: Option<String>,
}

impl Tree {
    /// The owned AST of the tree.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Takes the owned AST.
    #[must_use]
    pub fn into_document(self) -> Document {
        self.document
    }

    /// Reads JSON text; a bare tree keeps the text as its wire form, an
    /// envelope keeps its `document`.
    fn read(json: &str, top: Option<String>) -> Result<Self, Error> {
        let read = crate::reader::read_tree_json(json)?;
        let bare = match serde_json::from_str::<Value>(json) {
            Ok(Value::Object(mut envelope)) if envelope.contains_key("document") => envelope
                .remove("document")
                .map_or_else(String::new, |d| d.to_string()),
            _ => json.trim().to_string(),
        };
        Ok(Self {
            document: read.into_document(),
            json: bare,
            top,
        })
    }

    pub(crate) fn from_value(value: &Value) -> Result<Self, Error> {
        Self::read(&value.to_string(), None)
    }
}

/// How [`encode_tree_with`] writes a tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    /// Writes `pos` on the nodes; on by default.
    pub positions: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self { positions: true }
    }
}

/// A tree with the record of its parse (C2-A-172, C2-T-024).
#[derive(Debug, Clone)]
pub struct Parsed {
    pub tree: Tree,
    pub diagnostics: Vec<Entry>,
    pub suppressed_diagnostics: usize,
}

/// HTML with the record of its parse and render (C2-A-172, C2-H-130).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    pub html: String,
    pub diagnostics: Vec<Entry>,
    pub suppressed_diagnostics: usize,
}

/// A host: the registered writers and, provisionally, extensions. The free
/// functions of this module are the calls of `Carve::default()`.
#[derive(Debug, Clone, Default)]
pub struct Carve {
    writers: Writers,
}

impl Carve {
    /// A host with the built-in writers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The writer registry.
    #[must_use]
    pub fn writers(&self) -> &Writers {
        &self.writers
    }

    /// Adds a writer; see [`Writers::register`].
    ///
    /// # Errors
    ///
    /// [`Error::Option`] when the target is already registered.
    pub fn register_writer(&mut self, writer: std::sync::Arc<dyn Writer>) -> Result<(), Error> {
        self.writers.register(writer)
    }

    /// PROVISIONAL (C2-X-033, C2-X-053): registers an extension. This engine
    /// does not run host renderers or transforms yet.
    ///
    /// # Errors
    ///
    /// Always [`Error::Unsupported`] in this engine.
    #[allow(clippy::needless_pass_by_value)] // a host hands the registration over
    pub fn register_extension(&mut self, extension: Extension) -> Result<(), Error> {
        let _ = extension;
        Err(Error::Unsupported {
            call: "registerExtension",
        })
    }

    /// `write`: the Carve source of a tree.
    ///
    /// # Errors
    ///
    /// [`Error::Unsupported`] while no `carve` writer is registered;
    /// [`Error::Write`] for a writer loss (C2-W-134).
    pub fn write(&self, tree: &Tree, opts: &WriteOptions) -> Result<String, Error> {
        Ok(self.write_with_diagnostics(tree, opts)?.output)
    }

    /// `writeWithDiagnostics`: the Carve source of a tree and the record.
    ///
    /// # Errors
    ///
    /// As [`Carve::write`].
    pub fn write_with_diagnostics(
        &self,
        tree: &Tree,
        opts: &WriteOptions,
    ) -> Result<Written, Error> {
        self.write_to("carve", tree, opts)
            .map_err(|e| unsupported_as(e, "write"))
    }

    /// Writes a tree with the writer of `target` (`html`, and the targets
    /// registered on this host).
    ///
    /// # Errors
    ///
    /// [`Error::Unsupported`] for a target with no writer; the writer's error.
    pub fn write_to(
        &self,
        target: &str,
        tree: &Tree,
        opts: &WriteOptions,
    ) -> Result<Written, Error> {
        let Some(writer) = self.writers.get(target) else {
            return Err(Error::Unsupported { call: "write" });
        };
        writer.write(tree, opts)
    }

    /// `format`: the canonical Carve source of a source (C2-W-001).
    ///
    /// # Errors
    ///
    /// As [`Carve::write`], and [`Error::Input`] for an input too long.
    pub fn format<'a>(
        &self,
        input: impl Into<Input<'a>>,
        opts: &WriteOptions,
    ) -> Result<String, Error> {
        Ok(self.format_with_diagnostics(input, opts)?.output)
    }

    /// `formatWithDiagnostics`.
    ///
    /// # Errors
    ///
    /// As [`Carve::format`].
    pub fn format_with_diagnostics<'a>(
        &self,
        input: impl Into<Input<'a>>,
        opts: &WriteOptions,
    ) -> Result<Written, Error> {
        if self.writers.get("carve").is_none() {
            return Err(Error::Unsupported { call: "format" });
        }
        let tree = parse(input, &opts.render)?;
        self.write_with_diagnostics(&tree, opts)
    }
}

fn unsupported_as(e: Error, call: &'static str) -> Error {
    match e {
        Error::Unsupported { .. } => Error::Unsupported { call },
        other => other,
    }
}

/// C2-H-163: a source longer than the `max_length` of its profile is
/// refused before it is parsed; any source past `u32::MAX` bytes too.
fn check_length(len: usize, opts: &Options) -> Result<(), Error> {
    let profile = opts.profile.name();
    let limit = crate::tables::PROFILES
        .iter()
        .find(|p| p.name == profile)
        .and_then(|p| p.max_length)
        .unwrap_or(u32::MAX as usize)
        .min(u32::MAX as usize);
    if len > limit {
        return Err(Error::Input {
            profile: profile.to_string(),
            limit,
        });
    }
    Ok(())
}

/// `parse`: the resolved tree of a source, includes expanded when
/// `opts.includes` is set (C2-A-106, C2-B-218).
///
/// # Errors
///
/// [`Error::Input`] for a source longer than its profile accepts.
pub fn parse<'a>(input: impl Into<Input<'a>>, opts: &Options) -> Result<Tree, Error> {
    let input = input.into();
    check_length(input.len(), opts)?;
    let top = opts.includes.as_ref().and_then(|i| i.file.clone());
    match input {
        Input::Bytes(bytes) if std::str::from_utf8(bytes).is_err() => {
            let arena = Arena::new();
            let doc = crate::try_parse_resolved_bytes_in(bytes, opts, &arena)?;
            Tree::read(&crate::to_json(&doc), top)
        }
        _ => Tree::read(&crate::tree_json(&input.text(), opts), top),
    }
}

/// `parseWithDiagnostics`: the tree, the parse record and the count of
/// include entries held back past `include_diagnostics` (C2-T-024).
///
/// # Errors
///
/// As [`parse`].
pub fn parse_with_diagnostics<'a>(
    input: impl Into<Input<'a>>,
    opts: &Options,
) -> Result<Parsed, Error> {
    let input = input.into();
    let tree = parse(input, opts)?;
    let (diagnostics, suppressed_diagnostics) =
        record::cap_includes(&crate::parse_diagnostics(&input.text(), opts));
    Ok(Parsed {
        tree,
        diagnostics,
        suppressed_diagnostics,
    })
}

/// `parseWithLayout` (C2-A-003): not available in this engine yet.
///
/// # Errors
///
/// Always [`Error::Unsupported`].
pub fn parse_with_layout<'a>(input: impl Into<Input<'a>>, opts: &Options) -> Result<Tree, Error> {
    let _ = (input.into(), opts);
    Err(Error::Unsupported {
        call: "parseWithLayout",
    })
}

/// `render`: the HTML of a source (C2-H-137).
///
/// # Errors
///
/// [`Error::Input`] for a source longer than its profile accepts;
/// [`Error::Render`] for a loss under `strictLosses`.
pub fn render<'a>(input: impl Into<Input<'a>>, opts: &Options) -> Result<String, Error> {
    let input = input.into();
    check_length(input.len(), opts)?;
    Ok(match input {
        Input::Text(s) => crate::try_render(s, opts)?,
        Input::Bytes(b) => crate::try_render_bytes(b, opts)?,
    })
}

/// `render` of a supplied tree, which is resolved anew (C2-A-135).
///
/// # Errors
///
/// [`Error::Render`] for a loss under `strictLosses`.
pub fn render_tree(tree: &Tree, opts: &Options) -> Result<String, Error> {
    Ok(crate::render_tree(tree.document(), opts)?)
}

/// `renderWithDiagnostics`: the HTML and the record of the parse and the
/// render.
///
/// # Errors
///
/// As [`render`]; under `strictLosses` a loss is [`Error::Render`].
pub fn render_with_diagnostics<'a>(
    input: impl Into<Input<'a>>,
    opts: &Options,
) -> Result<Rendered, Error> {
    let input = input.into();
    check_length(input.len(), opts)?;
    let rendered = crate::render_with_diagnostics(&input.text(), opts);
    finish_render(rendered, opts)
}

/// `renderWithDiagnostics` of a supplied tree.
///
/// # Errors
///
/// As [`render_tree`].
pub fn render_tree_with_diagnostics(tree: &Tree, opts: &Options) -> Result<Rendered, Error> {
    let rendered = crate::render_tree_with_diagnostics(tree.document(), opts)?;
    finish_render(rendered, opts)
}

fn finish_render(rendered: crate::Rendered, opts: &Options) -> Result<Rendered, Error> {
    let (diagnostics, suppressed_diagnostics) = record::cap_includes(&rendered.diagnostics);
    if opts.strict_losses {
        let codes: Vec<String> = diagnostics
            .iter()
            .filter(|e| e.channel == "render-loss")
            .map(|e| e.code.clone())
            .collect();
        if !codes.is_empty() {
            return Err(Error::Render { codes });
        }
    }
    Ok(Rendered {
        html: rendered.html,
        diagnostics,
        suppressed_diagnostics,
    })
}

/// `lint`: the record of a lint run, parse entries and lint findings
/// (C2-W-103, C2-T-031).
///
/// # Errors
///
/// [`Error::Option`] for an unknown rule id, a setting other than `on` or
/// `off`, or an unknown platform (C2-W-109).
pub fn lint<'a>(input: impl Into<Input<'a>>, opts: &LintOptions) -> Result<Vec<Entry>, Error> {
    options::check_lint(opts)?;
    let input = input.into();
    Ok(crate::lint::lint(&input.text(), opts)
        .iter()
        .map(Entry::from)
        .collect())
}

/// `importDocument` (C2-M-001): not available in this engine yet.
///
/// # Errors
///
/// Always [`Error::Unsupported`].
pub fn import_document(html: &str, opts: &Options) -> Result<Tree, Error> {
    let _ = (html, opts);
    Err(Error::Unsupported {
        call: "importDocument",
    })
}

/// `includeDependencies`: the include nodes of an expanded tree in tree
/// order (C2-N-083).
#[must_use]
pub fn include_dependencies(tree: &Tree) -> Vec<Dependency> {
    let arena = Arena::new();
    let doc = tree.document().in_arena(&arena);
    let mut out = Vec::new();
    crate::include::dependencies(doc.children, tree.top.as_deref(), &mut out);
    out
}

/// `encodeTree`: the JSON text of the bare tree (C2-A-012).
#[must_use]
pub fn encode_tree(tree: &Tree) -> String {
    tree.json.clone()
}

/// `encodeTree` with `positions` off or on.
#[must_use]
pub fn encode_tree_with(tree: &Tree, opts: EncodeOptions) -> String {
    if opts.positions {
        return encode_tree(tree);
    }
    let Ok(mut value) = serde_json::from_str::<Value>(&tree.json) else {
        return encode_tree(tree);
    };
    strip_positions(&mut value);
    value.to_string()
}

fn strip_positions(value: &mut Value) {
    let mut pending = vec![value];
    while let Some(v) = pending.pop() {
        match v {
            Value::Object(map) => {
                if map.contains_key("type") {
                    map.remove("pos");
                }
                pending.extend(map.values_mut());
            }
            Value::Array(items) => pending.extend(items.iter_mut()),
            _ => {}
        }
    }
}

/// `encodeEnvelope`: the JSON text of the envelope of a tree (C2-A-012).
#[must_use]
pub fn encode_envelope(tree: &Tree) -> String {
    let document: Value = serde_json::from_str(&tree.json).unwrap_or(Value::Null);
    let version = document.get("astVersion").cloned().unwrap_or(Value::Null);
    let extensions = document
        .get("extensions")
        .map(|e| format!(r#","extensions":{e}"#))
        .unwrap_or_default();
    format!(
        r#"{{"astVersion":{version}{extensions},"document":{}}}"#,
        tree.json
    )
}

/// `decodeTree`: reads the JSON text of a 2.0 tree or envelope (C2-A-188).
///
/// # Errors
///
/// [`Error::Tree`] for a tree the schema or the reader refuses.
pub fn decode_tree(json: &str) -> Result<Tree, Error> {
    Tree::read(json, None)
}

/// `validateTree`: runs the reader checks on a tree (C2-A-188).
///
/// # Errors
///
/// [`Error::Tree`] for the refusal.
pub fn validate_tree(tree: &Tree) -> Result<(), Error> {
    crate::reader::read_tree_json(&tree.json)?;
    Ok(())
}

/// `migrateTree` (C2-A-178): not available in this engine yet.
///
/// # Errors
///
/// Always [`Error::Unsupported`].
pub fn migrate_tree(json: &Value) -> Result<Tree, Error> {
    let _ = json;
    Err(Error::Unsupported {
        call: "migrateTree",
    })
}
