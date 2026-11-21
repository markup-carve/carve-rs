//! The public library of the Carve 0.2 engine: the calls of `tooling.md`
//! (C2-T-005) in Rust spelling (C2-T-001). This module is the whole contract;
//! the other modules of the crate are engine internals.
//!
//! ```
//! use carve::api::{self, Options};
//!
//! let html = api::render("Hello *world*", &Options::default())?;
//! assert_eq!(html, "<p>Hello <strong>world</strong></p>");
//!
//! let tree = api::parse("Hello", &Options::default())?;
//! let encoded = api::encode_tree(&tree);
//! let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
//! assert_eq!(encoded, value.to_string()); // C2-A-217: canonical sorted members.
//! # Ok::<(), api::Error>(())
//! ```
//!
//! | `tooling.md` call | Rust |
//! |---|---|
//! | `parse` | [`parse`], [`Carve::parse`] |
//! | `parseWithDiagnostics` | [`parse_with_diagnostics`] |
//! | `parseWithLayout` | [`parse_with_layout`] |
//! | `render` | [`render`], [`render_tree`] |
//! | `renderWithDiagnostics` | [`render_with_diagnostics`], [`render_tree_with_diagnostics`] |
//! | `format`, `formatWithDiagnostics` | [`Carve::format`], [`Carve::format_with_diagnostics`] |
//! | `write`, `writeWithDiagnostics` | [`Carve::write`], [`Carve::write_with_diagnostics`], [`Carve::write_to`] |
//! | `lint` | [`lint`] |
//! | `importDocument` | [`import_document`] |
//! | `includeDependencies` | [`include_dependencies`] |
//! | `encodeTree`, `encodeEnvelope` | [`encode_tree`], [`encode_tree_with`], [`encode_envelope`] |
//! | `decodeTree`, `validateTree` | [`decode_tree`], [`validate_tree`] |
//! | `migrateTree` | [`migrate_tree`], [`migrate_tree_with`] |
//! | `diffTree`, `applyPatch` | [`diff_tree`], [`apply_patch`] |
//! | `sourceFingerprint`, `treeFingerprint` | [`source_fingerprint`], [`tree_fingerprint`] |
//! | `headingId` | [`heading_id`] |
//! | `positionOf` | [`position_of`] (C2-T-041) |
//! | `VERSION`, `SPEC_VERSION`, `NODE_TYPES` | [`VERSION`], [`SPEC_VERSION`], [`NODE_TYPES`] |

mod error;
pub mod extension;
pub mod extensions;
mod hub;
mod img_fence;
mod import;
pub mod incremental;
mod info;
pub mod json;
pub(crate) mod layout;
mod migrate;
mod options;
mod patch;
mod record;
mod standard;
pub mod stream;
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
pub use crate::options::{HeadingIds, Mode, Options, Profile, Social, SoftBreak, UrlPolicy};
pub use error::Error;
pub use extension::{
    Channel, DocumentTransform, Extension, ExtensionDeclaration, RenderContext, Renderer,
};
pub use hub::ImportFormat;
pub use import::{ImportAdapter, ImportOptions, Imported, LossOptions, import_document};
pub use incremental::{
    IncrementalParse, IncrementalParseError, NodeIdentity, NodeIdentityEntry, ParserSnapshot,
    TextChange, parse_snapshot, parse_snapshot_with_identity, reparse,
};
pub use info::{
    NODE_TYPES, NodeTypes, Position, SPEC_VERSION, SpecVersion, VERSION, heading_id, position_of,
};
pub use migrate::{MigrateOptions, Migrated, migrate_tree_with};
pub use patch::{apply_patch, diff_tree, source_fingerprint, tree_fingerprint};
pub use record::{Entry, Range};
pub use stream::{StreamOutcome, try_render_html_streaming};
pub use writer::{CarveWriter, HtmlWriter, WriteOptions, Writer, Writers, Written};

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

    /// C2-B-218: moves the ranges of entries found in [`Input::text`] back
    /// to offsets of the input bytes.
    fn remap(&self, entries: &mut [Entry]) {
        let Self::Bytes(bytes) = self else { return };
        if std::str::from_utf8(bytes).is_ok() {
            return;
        }
        // (offset in the replaced text, offset in the input) at each chunk start
        let mut marks = Vec::new();
        let (mut text, mut raw) = (0usize, 0usize);
        for chunk in bytes.utf8_chunks() {
            marks.push((text, raw));
            text += chunk.valid().len();
            raw += chunk.valid().len();
            if !chunk.invalid().is_empty() {
                marks.push((text, raw));
                text += '\u{FFFD}'.len_utf8();
                raw += chunk.invalid().len();
            }
        }
        marks.push((text, raw));
        let map = |at: usize| {
            let i = marks.partition_point(|m| m.0 <= at).saturating_sub(1);
            marks.get(i).map_or(at, |&(t, r)| {
                let next = marks.get(i + 1).map_or(usize::MAX, |m| m.1);
                (r + (at - t)).min(next)
            })
        };
        for range in entries.iter_mut().filter_map(|e| e.range.as_mut()) {
            range.start_offset = map(range.start_offset);
            range.end_offset = map(range.end_offset);
        }
    }
}

/// A resolved 2.0 tree that passed the reader checks (C2-A-188), with its
/// JSON wire form.
#[derive(Debug, Clone)]
pub struct Tree {
    document: Document,
    /// Built on first use from `document` where the parse did not give it.
    json: std::sync::OnceLock<String>,
    top: Option<String>,
    sources: std::collections::HashMap<String, String>,
}

impl Tree {
    /// The owned AST of the tree.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The JSON wire form of the tree.
    pub(crate) fn json(&self) -> &str {
        self.json.get_or_init(|| crate::owned_json(&self.document))
    }

    /// The wire form as a JSON value, without a round trip through text
    /// where the wire form was not built yet.
    pub(crate) fn value(&self) -> Result<Value, Error> {
        if let Some(json) = self.json.get() {
            return writer::tree_value(json);
        }
        let mut value = serde_json::to_value(&self.document).map_err(|e| Error::Write {
            target: "carve".into(),
            message: e.to_string(),
            diagnostics: Vec::new(),
        })?;
        if let Some(map) = value.as_object_mut() {
            map.insert("type".into(), "document".into());
        }
        Ok(value)
    }

    /// A tree of the engine's own parse, whose wire form is the owned
    /// tree's encoding unless the parse gave one.
    fn parsed(
        document: Document,
        json: Option<String>,
        top: Option<String>,
        sources: std::collections::HashMap<String, String>,
    ) -> Self {
        Self {
            document,
            json: json.map_or_else(std::sync::OnceLock::new, std::sync::OnceLock::from),
            top,
            sources,
        }
    }

    /// The id of the top-level document, which [`include_dependencies`]
    /// gives as `from` of a top-level include. [`parse`] takes it from
    /// `Includes::file`; a decoded tree has none until it is set here.
    pub fn set_file(&mut self, file: Option<String>) {
        self.top = file;
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
        let bare = match crate::api::deep_json(json) {
            Ok(Value::Object(mut envelope)) if envelope.contains_key("document") => envelope
                .remove("document")
                .map_or_else(String::new, |d| d.to_string()),
            _ => json.trim().to_string(),
        };
        Ok(Self {
            document: read.into_document(),
            json: bare.into(),
            top,
            sources: std::collections::HashMap::new(),
        })
    }

    pub(crate) fn from_value(value: &Value) -> Result<Self, Error> {
        Self::read(&value.to_string(), None)
    }

    /// A tree the engine built from checked parts, decoded without the
    /// reader checks, which cost far more than the parse (carve-rs#4336).
    pub(crate) fn trusted(mut value: Value, top: Option<String>) -> Result<Self, Error> {
        let json = value.to_string();
        if let Some(map) = value.as_object_mut() {
            map.remove("type");
        }
        let document = serde_json::from_value(value).map_err(|e| {
            Error::Tree(crate::reader::TreeError::Decode {
                path: String::new(),
                reason: e.to_string(),
            })
        })?;
        Ok(Self {
            document,
            json: json.into(),
            top,
            sources: std::collections::HashMap::new(),
        })
    }

    /// Rebuild the wire form after an engine transform changed the owned AST.
    /// The host needs the tree as an AST for rendering and as canonical JSON
    /// for the public boundary; this avoids decoding that JSON back to an AST.
    pub(crate) fn from_owned(document: Document, top: Option<String>) -> Result<Self, Error> {
        let mut value =
            serde_json::to_value(&document).map_err(|e| Error::option("tree", e.to_string()))?;
        if let Some(map) = value.as_object_mut() {
            map.insert("type".into(), "document".into());
        }
        Ok(Self {
            document,
            json: value.to_string().into(),
            top,
            sources: std::collections::HashMap::new(),
        })
    }

    /// An internal render view. Only the renderer reads this tree; it is not
    /// returned by a public tree call and therefore needs no wire encoding.
    fn render_view(document: Document, top: Option<String>) -> Self {
        Self {
            document,
            json: String::new().into(),
            top,
            sources: std::collections::HashMap::new(),
        }
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

/// A parsed tree and its version-2 source-layout sidecar (C2-A-003).
#[derive(Debug, Clone)]
pub struct ParsedLayout {
    pub tree: Tree,
    pub diagnostics: Vec<Entry>,
    pub suppressed_diagnostics: usize,
    pub layout: Value,
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
    extensions: extension::Registry,
    standard: standard::Standard,
    matchers: extensions::Matchers,
    host: extensions::HostFields,
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

    /// PROVISIONAL (C2-X-033, C2-X-052, C2-X-053): registers an extension. Its
    /// renderers and transform take part in the render calls of this host.
    ///
    /// # Errors
    ///
    /// [`Error::Option`] for an unqualified name, an empty version, a name and
    /// version already registered, or two renderers for one channel.
    pub fn register_extension(&mut self, extension: Extension) -> Result<(), Error> {
        self.extensions.register(extension)
    }

    /// PROVISIONAL: enables a standard extension of `data/extensions.json`.
    /// This engine has `carve.tabs` (C2-X-039): its matcher claims `tabs`
    /// divs in [`Carve::parse`] and the render calls, and with `renderer` its
    /// standard renderers render it in both channels (C2-X-043, C2-X-052).
    ///
    /// # Errors
    ///
    /// [`Error::Option`] for a name this engine has no standard extension for.
    pub fn enable_standard(&mut self, name: &str, renderer: bool) -> Result<(), Error> {
        // C2-X-057: with its renderer, `carve.wiki` is the wikilinks bundle.
        if name == "carve.wiki" && renderer {
            return self.register(extensions::wikilinks(
                extensions::WikilinksOptions::default(),
            ));
        }
        if let Some(bundle) = extensions::standard(name) {
            return self.register(if renderer {
                bundle
            } else {
                bundle.without_renderers()
            });
        }
        self.standard.enable(name, renderer)
    }

    /// Registers the standard tabs renderer with its host mode, `css` or `aria`
    /// (C2-X-043, C2-X-048, C2-X-056).
    ///
    /// # Errors
    ///
    /// [`Error::Option`] for a mode other than `css` or `aria`.
    pub fn enable_standard_tabs_renderer(&mut self, mode: &str) -> Result<(), Error> {
        let aria = match mode {
            "css" => false,
            "aria" => true,
            _ => {
                return Err(Error::option(
                    "mode",
                    "tabs renderer mode must be css or aria",
                ));
            }
        };
        self.enable_standard("carve.tabs", true)?;
        self.standard.tabs_aria = aria;
        Ok(())
    }

    /// PROVISIONAL: registers a bundle of [`extensions`]: its extensions as
    /// [`Carve::register_extension`] does, and its matchers, which claim in
    /// [`Carve::parse`] and the render calls (C2-X-005).
    ///
    /// # Errors
    ///
    /// [`Error::Option`] as [`Carve::register_extension`], for a matcher of an
    /// extension the bundle does not declare, and for a second claim of one
    /// source name (C2-X-014). Nothing is registered then.
    pub fn register(&mut self, bundle: extensions::Bundle) -> Result<(), Error> {
        let mut next = self.clone();
        let mut matchers = next.matchers.clone();
        for m in &bundle.matchers {
            if !bundle
                .extensions
                .iter()
                .any(|e| e.declaration.name == m.extension)
            {
                return Err(Error::option(
                    "extensions",
                    format!(
                        "the matcher of {} names no extension of its bundle",
                        m.extension
                    ),
                ));
            }
            matchers.check(m)?;
            matchers.push(extensions::Matcher {
                extension: String::new(),
                form: m.form,
                name: m.name.clone(),
                claimer: Box::new(|_: &extensions::Claim<'_>| None),
            });
        }
        for e in bundle.extensions {
            next.extensions.register(e)?;
        }
        for m in bundle.matchers {
            next.matchers.push(m);
        }
        next.host.merge(bundle.host);
        *self = next;
        Ok(())
    }

    fn plain(&self) -> bool {
        self.extensions.is_empty()
            && self.standard.is_empty()
            && self.matchers.is_empty()
            && self.host.is_empty()
    }

    /// `parse` with the matchers of the enabled standard extensions
    /// (C2-X-005). Without them it is [`parse`].
    ///
    /// # Errors
    ///
    /// As [`parse`].
    pub fn parse<'a>(&self, input: impl Into<Input<'a>>, opts: &Options) -> Result<Tree, Error> {
        Ok(self.parse_with_diagnostics(input, opts)?.tree)
    }

    /// `parseWithDiagnostics` with the matchers of the enabled standard
    /// extensions. A claim whose source record cannot be written keeps the
    /// core node and reports `extension-transform-failed` (C2-X-051).
    ///
    /// # Errors
    ///
    /// As [`parse`].
    pub fn parse_with_diagnostics<'a>(
        &self,
        input: impl Into<Input<'a>>,
        opts: &Options,
    ) -> Result<Parsed, Error> {
        let input = input.into();
        let mut parsed = parse_with_diagnostics(input, opts)?;
        if !self.may_claim(&parsed.tree) {
            return Ok(parsed);
        }
        let opener = |file: Option<&str>, at: usize| match file {
            Some(id) => parsed
                .tree
                .sources
                .get(id)
                .and_then(|source| standard::opener_name(source, at)),
            None => match input {
                Input::Text(s) => standard::opener_name(s, at),
                Input::Bytes(b) => {
                    let line = b
                        .get(at..)?
                        .split(|&c| c == b'\n')
                        .map(String::from_utf8_lossy)
                        .find(|l| !standard::is_attribute_line(l))?;
                    standard::opener_name(&line, 0)
                }
            },
        };
        if self.standard.is_empty() && parsed.tree.sources.is_empty() {
            let claimed =
                self.matchers
                    .apply_owned(&mut parsed.tree.document, &opener, &|name| {
                        self.extensions.declaration(name)
                    })?;
            let top = parsed.tree.top.take();
            parsed.tree = Tree::from_owned(std::mem::take(&mut parsed.tree.document), top)?;
            parsed.diagnostics.extend(claimed);
            return Ok(parsed);
        }
        let mut value = parsed.tree.value()?;
        let failed = self.claim_standard(&mut value, &opener);
        let claimed = if self.matchers.is_empty() {
            Vec::new()
        } else {
            self.matchers.apply(&mut value, &opener, &|name| {
                self.extensions.declaration(name)
            })
        };
        let top = parsed.tree.top.take();
        // The standard claims are engine code and each host claim was read
        // back on its own (C2-X-031), so the rest of the tree needs no checks.
        parsed.tree = Tree::trusted(value, top)?;
        parsed.diagnostics.extend(claimed);
        for name in failed {
            parsed.diagnostics.push(Entry {
                code: "extension-transform-failed".into(),
                severity: "warning".into(),
                channel: "render-loss".into(),
                message: format!(
                    "The extension {name} could not write its claim source; the core node was retained"
                ),
                path: None,
                range: None,
                file: None,
                foreign_type: None,
            });
        }
        Ok(parsed)
    }

    /// Whether the tree holds a node of a type an enabled claim reads, so the
    /// JSON pass of the claims can change it.
    /// Whether no matcher of the host can claim a node of any tree.
    fn claims_nothing(&self) -> bool {
        !(self.standard.tabs || self.matchers.has_form(extensions::Form::Div))
            && self.standard.inline_matchers().is_empty()
            && !self.matchers.has_form(extensions::Form::Inline)
            && self.standard.fence_matchers().is_empty()
            && self
                .matchers
                .names(extensions::Form::Fence)
                .next()
                .is_none()
    }

    fn may_claim(&self, tree: &Tree) -> bool {
        if self.claims_nothing() {
            return false;
        }
        let div = self.standard.tabs || self.matchers.has_form(extensions::Form::Div);
        let inline = !self.standard.inline_matchers().is_empty()
            || self.matchers.has_form(extensions::Form::Inline);
        let json = tree.json();
        // A fence is claimed by its language, the `syntax` of the code block.
        let syntax = |name: &str| json.contains(&format!(r#""syntax":{}"#, Value::from(name)));
        let fence = self
            .standard
            .fence_matchers()
            .iter()
            .any(|(name, _)| syntax(name))
            || self.matchers.names(extensions::Form::Fence).any(syntax);
        (div && json.contains(r#""type":"div""#))
            || fence
            || (inline && json.contains(r#""type":"inline_extension""#))
    }

    /// The claims of the enabled standard extensions; returns those whose
    /// claim source could not be written (C2-X-051).
    fn claim_standard(
        &self,
        value: &mut Value,
        opener: &dyn Fn(Option<&str>, usize) -> Option<String>,
    ) -> Vec<&'static str> {
        let mut failed = Vec::new();
        if self.standard.tabs && !standard::claim_tabs(value, opener) {
            failed.push("carve.tabs");
        }
        let matchers: Vec<_> = self
            .standard
            .inline_matchers()
            .into_iter()
            .filter(|(source, _)| {
                !self
                    .matchers
                    .names(extensions::Form::Inline)
                    .any(|name| name == *source)
            })
            .collect();
        if !matchers.is_empty() {
            failed.extend(standard::claim_inline(value, &matchers));
        }
        let fences = self.standard.fence_matchers();
        if !fences.is_empty() {
            let claimed = img_fence::claim(value, &fences);
            let declared: Vec<(&str, Value)> = claimed
                .into_iter()
                .map(|n| (n, serde_json::json!([])))
                .collect();
            if !declared.is_empty() {
                standard::declare_names(value, &declared);
            }
        }
        failed
    }

    /// `render` with the registered extensions: the source is parsed, the
    /// document transforms run on the resolved view (C2-X-053), and the view
    /// is rendered with the renderers of the channel of `opts.mode`
    /// (C2-X-052). With no extension registered it is [`render`].
    ///
    /// # Errors
    ///
    /// As [`render`].
    pub fn render<'a>(&self, input: impl Into<Input<'a>>, opts: &Options) -> Result<String, Error> {
        if self.plain() {
            return render(input, opts);
        }
        Ok(self
            .render_with_diagnostics_for(input.into(), opts, true)?
            .html)
    }

    /// `renderWithDiagnostics` with the registered extensions.
    ///
    /// # Errors
    ///
    /// As [`render_with_diagnostics`].
    pub fn render_with_diagnostics<'a>(
        &self,
        input: impl Into<Input<'a>>,
        opts: &Options,
    ) -> Result<Rendered, Error> {
        self.render_with_diagnostics_for(input.into(), opts, false)
    }

    fn render_with_diagnostics_for(
        &self,
        input: Input<'_>,
        opts: &Options,
        refuse_loss: bool,
    ) -> Result<Rendered, Error> {
        if self.plain() {
            return render_with_diagnostics(input, opts);
        }
        check_length(input.len(), opts)?;
        if let Input::Text(text) = input
            && self.standard.is_empty()
            && opts.includes.is_none()
            && (!self.extensions.has_transforms() || self.extensions.owned_transforms_only())
            && let Some(rendered) = self.render_text(text, opts, refuse_loss)
        {
            return rendered;
        }
        let parsed = self.parse_with_diagnostics(input, opts)?;
        let mut rendered = self.render_view(parsed.tree, opts, refuse_loss)?;
        let mut diagnostics = parsed.diagnostics;
        diagnostics.append(&mut rendered.diagnostics);
        rendered.diagnostics = diagnostics;
        rendered.suppressed_diagnostics += parsed.suppressed_diagnostics;
        Ok(rendered)
    }

    /// The claims of the host's matchers on a tree of the top file in `arena`;
    /// true in the second place when a claim changed the tree.
    fn claim_arena<'a>(
        &self,
        doc: &mut crate::ast::Document<'a>,
        arena: &'a crate::Arena,
        text: &str,
    ) -> Result<(Vec<Entry>, bool), Error> {
        if self.claims_nothing() {
            return Ok((Vec::new(), false));
        }
        // Without includes every node is of the top file.
        let opener = |file: Option<&str>, at: usize| {
            file.is_none()
                .then(|| standard::opener_name(text, at))
                .flatten()
        };
        self.matchers.apply_arena(doc, arena, &opener, &|name| {
            self.extensions.declaration(name)
        })
    }

    /// The render of a source in one arena: parse, claims, transforms and
    /// render, with no owned or JSON copy of the whole tree (carve-rs#4366).
    /// `None` when the parse refuses the source, which the general path reports.
    fn render_text(
        &self,
        text: &str,
        opts: &Options,
        refuse_loss: bool,
    ) -> Option<Result<Rendered, Error>> {
        let arena = crate::Arena::for_input(text);
        let mut found = Vec::new();
        let mut claimed: Result<(Vec<Entry>, bool), Error> = Ok((Vec::new(), false));
        // C2-X-005: engine claimers read no resolution result, so they run on
        // the core parse and one resolve of the claimed tree gives what
        // resolving it again after the claims gave; so does a transform that
        // reads none. A host claimer sees the resolved tree, as in `parse`.
        let early_claims = self.matchers.engine_only();
        let early = early_claims && self.extensions.parsed_arena_transforms_only();
        let (mut doc, _, _) = crate::prepare_with(text, opts, &arena, &mut found, &mut |doc| {
            if early_claims {
                claimed = self.claim_arena(doc, &arena, text);
                crate::heading_shift::apply(doc.children, opts.heading_shift);
            }
            if early && claimed.is_ok() {
                self.extensions.transform_parsed_arena(doc, &arena);
            }
        })
        .ok()?;
        if !early_claims {
            claimed = self.claim_arena(&mut doc, &arena, text);
            crate::heading_shift::apply(doc.children, opts.heading_shift);
        }
        let (mut diagnostics, suppressed) = record::cap_includes(&found);
        let changed = match claimed {
            Ok((entries, changed)) => {
                diagnostics.extend(entries);
                changed && !early_claims
            }
            Err(e) => return Some(Err(e)),
        };
        let (doc, files) = if early {
            (doc, None)
        } else if self.extensions.has_transforms() {
            // C2-X-053: a transform sees the resolved view, which is resolved again after it.
            if changed {
                let _ = crate::resolve_tree_unflattened(&mut doc, opts, &arena);
            }
            let mut owned = doc.owned();
            self.extensions.transform_owned(&mut owned);
            let mut doc = owned.in_arena(&arena);
            let files = crate::resolve_tree(&mut doc, opts, &arena);
            (doc, files)
        } else if changed {
            let files = crate::resolve_tree(&mut doc, opts, &arena);
            (doc, files)
        } else {
            (doc, None)
        };
        Some(
            self.render_arena(&doc, files.as_ref(), opts, Vec::new(), refuse_loss)
                .map(|mut rendered| {
                    diagnostics.append(&mut rendered.diagnostics);
                    rendered.diagnostics = diagnostics;
                    rendered.suppressed_diagnostics += suppressed;
                    rendered
                }),
        )
    }

    /// `render` of a supplied tree with the registered extensions.
    ///
    /// # Errors
    ///
    /// As [`render_tree`].
    pub fn render_tree(&self, tree: &Tree, opts: &Options) -> Result<String, Error> {
        if self.plain() {
            let rendered = crate::render_tree_with_diagnostics(tree.document(), opts)?;
            return Ok(finish_render(rendered, opts, true)?.html);
        }
        Ok(self.render_view(tree.clone(), opts, true)?.html)
    }

    /// `renderWithDiagnostics` of a supplied tree with the registered extensions.
    ///
    /// # Errors
    ///
    /// As [`render_tree`].
    pub fn render_tree_with_diagnostics(
        &self,
        tree: &Tree,
        opts: &Options,
    ) -> Result<Rendered, Error> {
        if self.plain() {
            return render_tree_with_diagnostics(tree, opts);
        }
        self.render_view(tree.clone(), opts, false)
    }

    fn render_view(
        &self,
        tree: Tree,
        opts: &Options,
        refuse_loss: bool,
    ) -> Result<Rendered, Error> {
        let mut tree = tree;
        if opts.heading_shift != 0 {
            // C2-H-208: the shift comes before the first document transform.
            crate::heading_shift::apply_owned(&mut tree.document.children, opts.heading_shift);
            tree.json = std::sync::OnceLock::new();
        }
        // C2-X-053: transforms see the resolved view; without one the render resolves the tree itself.
        let (view, transform_entries) = if self.extensions.owned_transforms_only() {
            let mut resolved = crate::resolved_owned(tree.document(), opts);
            self.extensions.transform_owned(&mut resolved);
            (Tree::render_view(resolved, tree.top), Vec::new())
        } else if self.extensions.has_transforms() {
            let mut view = Tree::from_owned(crate::resolved_owned(tree.document(), opts), None)?;
            view.top = tree.top;
            self.extensions.transform(view)
        } else {
            (tree, Vec::new())
        };
        let arena = crate::Arena::new();
        let mut doc = view.document().in_arena(&arena);
        let files = crate::resolve_tree(&mut doc, opts, &arena);
        self.render_arena(&doc, files.as_ref(), opts, transform_entries, refuse_loss)
    }

    /// Renders a tree in an arena that [`crate::resolve_tree`] resolved with
    /// the registered renderers, and finishes the record.
    fn render_arena<'a>(
        &self,
        doc: &crate::ast::Document<'a>,
        files: Option<&crate::resolve::files::Files<'a>>,
        opts: &Options,
        transform_entries: Vec<Entry>,
        refuse_loss: bool,
    ) -> Result<Rendered, Error> {
        let used_ids = || standard::UsedIds::of_arena(doc);
        let host = extension::Host::new(
            &self.extensions,
            self.standard,
            &self.host,
            opts,
            &used_ids,
            doc.extensions,
        );
        let rendered = crate::render_arena_hooked(doc, opts, files, &host);
        let (entries, suppressed) = record::cap_includes(&rendered.diagnostics);
        let mut diagnostics = transform_entries;
        diagnostics.extend(entries);
        diagnostics.extend(host.failures());
        let (dropped, lost) = limit(
            opts,
            &mut diagnostics,
            &rendered.dropped_losses,
            refuse_loss,
        )?;
        Ok(Rendered {
            html: if opts.strict_losses && lost {
                String::new()
            } else {
                rendered.html
            },
            diagnostics,
            suppressed_diagnostics: suppressed + dropped,
        })
    }

    /// `write`: the Carve source of a tree.
    ///
    /// # Errors
    ///
    /// [`Error::Write`] for a writer loss (C2-W-134).
    pub fn write(&self, tree: &Tree, opts: &WriteOptions) -> Result<String, Error> {
        refuse_losses(self.write_with_diagnostics(tree, opts)?)
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

    /// Writes a tree with the writer of `target`: `html`, `carve`, the
    /// exports `markdown`, `plain` and `ansi`, and the targets registered on
    /// this host.
    ///
    /// # Errors
    ///
    /// [`Error::Option`] for a target with no writer, naming the known
    /// targets; the writer's error.
    pub fn write_to(
        &self,
        target: &str,
        tree: &Tree,
        opts: &WriteOptions,
    ) -> Result<Written, Error> {
        let Some(writer) = self.writers.get(target) else {
            return Err(Error::option(
                "target",
                format!(
                    "unknown writer target \"{target}\"; known targets: {}",
                    self.writers.targets().join(", ")
                ),
            ));
        };
        writer.write(tree, &self.with_standard(opts))
    }

    /// `opts` with the standard extensions this host enables, so the writer's
    /// readback claims what this host's parse claims (C2-X-005).
    fn with_standard<'o>(&self, opts: &'o WriteOptions) -> std::borrow::Cow<'o, WriteOptions> {
        let tabs = "carve.tabs";
        if !self.standard.tabs || opts.extensions.iter().any(|e| e == tabs) {
            return std::borrow::Cow::Borrowed(opts);
        }
        let mut owned = opts.clone();
        owned.extensions.push(tabs.into());
        std::borrow::Cow::Owned(owned)
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
        refuse_losses(self.format_with_diagnostics(input, opts)?)
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
        // C2-H-137: format reads no render option.
        if opts.render.profile != Profile::default() {
            return Err(Error::option("profile", "not an option of format"));
        }
        writer::check_extensions(&opts.extensions)?;
        let mut host = self.clone();
        if opts.extensions.iter().any(|e| e == "carve.tabs") {
            host.enable_standard("carve.tabs", false)?;
        }
        let opts = &*host.with_standard(opts);
        let input = input.into();
        let valid = match input {
            Input::Text(_) => true,
            Input::Bytes(b) => std::str::from_utf8(b).is_ok(),
        };
        let settings = crate::write::Settings {
            align_tables: opts.align_tables,
            extensions: opts.extensions.clone(),
            layout: None,
            source: valid.then(|| input.text().to_string()),
        };
        if valid && let Some(written) = format_arena(&host, input, opts, &settings)? {
            return Ok(written);
        }
        let parsed = host.parse_with_diagnostics(input, &opts.render)?;
        let doc = parsed.tree.value()?;
        // C2-W-127: `normalize` writes as if there were no sidecar. Positions of a
        // byte input with malformed UTF-8 do not index the replaced text.
        let text = input.text();
        let src = (valid && !opts.normalize).then_some(text.as_ref());
        let (output, written) = crate::write::format_tree(&doc, src, &settings);
        Ok(written_with(parsed.diagnostics, output, written))
    }
}

/// `format` of a valid text input on the arena parse, where the owned tree would be the
/// same: no include and no matcher of `host` that could claim a node. `None` otherwise.
fn format_arena(
    host: &Carve,
    input: Input<'_>,
    opts: &WriteOptions,
    settings: &crate::write::Settings,
) -> Result<Option<Written>, Error> {
    if opts.render.includes.is_some() || !host.claims_nothing() {
        return Ok(None);
    }
    check_length(input.len(), &opts.render)?;
    let pool = crate::write::Pool::new();
    let text = input.text();
    let arena = Arena::for_input(&text);
    let mut found = Vec::new();
    let Ok((doc, _, None)) = crate::prepare(&text, &opts.render, &arena, &mut found) else {
        return Ok(None);
    };
    let doc = crate::view::V::Doc(arena.alloc(doc));
    let src = (!opts.normalize).then_some(text.as_ref());
    let (output, written) = crate::write::format_view(doc, &pool, src, settings);
    let (mut diagnostics, _) = record::cap_includes(&found);
    input.remap(&mut diagnostics);
    Ok(Some(written_with(diagnostics, output, written)))
}

/// The result of `format`: the parse record, then the writer's.
fn written_with(parsed: Vec<Entry>, output: String, written: Vec<Entry>) -> Written {
    // C2-N-005: a processor whose output is Carve source expands nothing.
    let mut diagnostics: Vec<Entry> = parsed
        .into_iter()
        .filter(|e| e.code != "include-no-resolver")
        .collect();
    diagnostics.extend(written);
    Written {
        output,
        diagnostics,
    }
}

/// C2-W-134: a call that returns only source refuses a result with a writer loss.
fn refuse_losses(written: Written) -> Result<String, Error> {
    if !crate::write::has_loss(&written.diagnostics) {
        return Ok(written.output);
    }
    let found: Vec<String> = written
        .diagnostics
        .iter()
        .filter(|e| crate::write::has_loss(std::slice::from_ref(e)))
        .map(|e| format!("{} at {}", e.code, e.path.as_deref().unwrap_or("")))
        .collect();
    Err(Error::Write {
        target: "carve".into(),
        message: format!(
            "the source cannot be returned without its loss report: {}",
            found.join(", ")
        ),
        diagnostics: written.diagnostics,
    })
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

/// A tree's JSON text as a value; trees nest past `serde_json`'s default
/// recursion limit of 128.
pub(crate) fn deep_json(text: &str) -> serde_json::Result<Value> {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let value = <Value as serde::Deserialize>::deserialize(&mut de)?;
    de.end()?;
    Ok(value)
}

/// `parse`: the resolved tree of a source, includes expanded when
/// `opts.includes` is set (C2-A-106, C2-B-218).
///
/// # Errors
///
/// [`Error::Input`] for a source longer than its profile accepts.
pub fn parse<'a>(input: impl Into<Input<'a>>, opts: &Options) -> Result<Tree, Error> {
    Ok(parse_collecting(input.into(), opts, &mut Vec::new())?.0)
}

/// [`parse`], adding the parse diagnostics to `diags` where the tree came
/// from the same pass; the flag says whether it did.
fn parse_collecting(
    input: Input<'_>,
    opts: &Options,
    diags: &mut Vec<crate::diagnostic::Diagnostic>,
) -> Result<(Tree, bool), Error> {
    check_length(input.len(), opts)?;
    let top = opts.includes.as_ref().and_then(|i| i.file.clone());
    // The engine's own tree skips the reader checks, which cost about 70
    // times the parse (carve-rs#4336); a decoded tree still runs them.
    match input {
        Input::Bytes(bytes) if std::str::from_utf8(bytes).is_err() => {
            let arena = Arena::new();
            let doc = crate::try_parse_resolved_bytes_in(bytes, opts, &arena)?;
            let tree = Tree::parsed(doc.owned(), None, top, std::collections::HashMap::new());
            Ok((tree, false))
        }
        _ => match crate::tree_parts_with_sources(&input.text(), opts, diags) {
            Some((json, document, sources)) => {
                Ok((Tree::parsed(document, json, top, sources), true))
            }
            None => Ok((Tree::read("", top)?, true)),
        },
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
    let mut found = Vec::new();
    let (tree, collected) = parse_collecting(input, opts, &mut found)?;
    if !collected {
        found = crate::parse_diagnostics(&input.text(), opts);
    }
    let (mut diagnostics, suppressed_diagnostics) = record::cap_includes(&found);
    input.remap(&mut diagnostics);
    Ok(Parsed {
        tree,
        diagnostics,
        suppressed_diagnostics,
    })
}

/// `parseWithLayout` (C2-A-003): the tree, diagnostics and source layout.
///
/// # Errors
///
/// As [`parse_with_diagnostics`].
pub fn parse_with_layout<'a>(
    input: impl Into<Input<'a>>,
    opts: &Options,
) -> Result<ParsedLayout, Error> {
    parse_with_layout_for(&Carve::default(), input, opts)
}

fn parse_with_layout_for<'a>(
    host: &Carve,
    input: impl Into<Input<'a>>,
    opts: &Options,
) -> Result<ParsedLayout, Error> {
    let input = input.into();
    let text = input.text();
    let source = text.as_ref();
    let parsed = host.parse_with_diagnostics(input, opts)?;
    let layout = layout::build(source, &parsed.tree, input)?;
    Ok(ParsedLayout {
        tree: parsed.tree,
        diagnostics: parsed.diagnostics,
        suppressed_diagnostics: parsed.suppressed_diagnostics,
        layout,
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
/// Input and option errors can refuse; a strict render loss returns an empty
/// HTML string and the error entries of the record.
pub fn render_with_diagnostics<'a>(
    input: impl Into<Input<'a>>,
    opts: &Options,
) -> Result<Rendered, Error> {
    let input = input.into();
    check_length(input.len(), opts)?;
    let rendered = crate::render_with_diagnostics(&input.text(), opts);
    let mut rendered = finish_render(rendered, opts, false)?;
    input.remap(&mut rendered.diagnostics);
    Ok(rendered)
}

/// `renderWithDiagnostics` of a supplied tree.
///
/// # Errors
///
/// As [`render_tree`].
pub fn render_tree_with_diagnostics(tree: &Tree, opts: &Options) -> Result<Rendered, Error> {
    let rendered = crate::render_tree_with_diagnostics(tree.document(), opts)?;
    finish_render(rendered, opts, false)
}

fn finish_render(
    rendered: crate::Rendered,
    opts: &Options,
    refuse_loss: bool,
) -> Result<Rendered, Error> {
    let (mut diagnostics, suppressed) = record::cap_includes(&rendered.diagnostics);
    let (dropped, lost) = limit(
        opts,
        &mut diagnostics,
        &rendered.dropped_losses,
        refuse_loss,
    )?;
    Ok(Rendered {
        html: if opts.strict_losses && lost {
            String::new()
        } else {
            rendered.html
        },
        diagnostics,
        suppressed_diagnostics: suppressed + dropped,
    })
}

/// C2-H-204, C2-H-205, C2-H-130: leaves allowed losses out, keeps the first
/// `max_losses` of the others. Returns the number not kept and whether any
/// unallowed loss occurred. A source-only render may request the typed refusal.
fn limit(
    opts: &Options,
    diagnostics: &mut Vec<Entry>,
    dropped: &[&str],
    refuse_loss: bool,
) -> Result<(usize, bool), Error> {
    let loss = |e: &Entry| e.channel == "render-loss";
    diagnostics.retain(|e| !(loss(e) && opts.allows(&e.code)));
    let mut codes: Vec<String> = Vec::new();
    let mut kept = 0usize;
    let mut cut = dropped.len();
    diagnostics.retain_mut(|e| {
        if !loss(e) {
            return true;
        }
        codes.push(e.code.clone());
        kept += 1;
        cut += usize::from(kept > opts.max_losses);
        if opts.strict_losses {
            e.severity = "error".into();
        }
        kept <= opts.max_losses
    });
    codes.extend(dropped.iter().map(|c| (*c).to_string()));
    if refuse_loss && opts.strict_losses && !codes.is_empty() {
        return Err(Error::Render { codes });
    }
    Ok((cut, !codes.is_empty()))
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
    let mut entries: Vec<Entry> = crate::lint::lint(&input.text(), opts)
        .iter()
        .map(Entry::from)
        .collect();
    input.remap(&mut entries);
    Ok(entries)
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
    // serde_json's object map orders keys lexically without preserve_order.
    // Parsing the accepted tree removes input whitespace and insertion order.
    deep_json(tree.json()).map_or_else(|_| tree.json().to_string(), |value| value.to_string())
}

/// `encodeTree` with `positions` off or on.
#[must_use]
pub fn encode_tree_with(tree: &Tree, opts: EncodeOptions) -> String {
    if opts.positions {
        return encode_tree(tree);
    }
    let Ok(mut value) = crate::api::deep_json(tree.json()) else {
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
    let document: Value = crate::api::deep_json(tree.json()).unwrap_or(Value::Null);
    let version = document.get("astVersion").cloned().unwrap_or(Value::Null);
    let extensions = document
        .get("extensions")
        .map(|e| format!(r#","extensions":{e}"#))
        .unwrap_or_default();
    format!(
        r#"{{"astVersion":{version}{extensions},"document":{}}}"#,
        tree.json()
    )
}

/// `decodeTree`: reads the JSON text of a 2.0 tree or envelope (C2-A-188).
///
/// # Errors
///
/// [`Error::Tree`] for a tree the schema or the reader refuses.
pub fn decode_tree(json: &str) -> Result<Tree, Error> {
    decode_tree_with(json, TreeReadOptions::default())
}

/// Optional sidecar passed to a tree reader (C2-T-005, C2-A-203).
#[derive(Debug, Clone, Copy, Default)]
pub struct TreeReadOptions<'a> {
    /// Annotation ranges to check against this tree.
    pub annotations: Option<&'a Value>,
}

/// Reads a tree and optionally checks its annotation ranges.
///
/// # Errors
///
/// [`Error::Tree`] for a refused tree or sidecar.
pub fn decode_tree_with(json: &str, options: TreeReadOptions<'_>) -> Result<Tree, Error> {
    let tree = Tree::read(json, None)?;
    if let Some(annotations) = options.annotations {
        let mut root: Value = deep_json(tree.json())
            .map_err(|e: serde_json::Error| crate::reader::TreeError::Json(e.to_string()))?;
        let supplied: Value = deep_json(json)
            .map_err(|e: serde_json::Error| crate::reader::TreeError::Json(e.to_string()))?;
        if root.get("extensions").is_none()
            && let Some(extensions) = supplied.get("extensions")
            && supplied.get("document").is_some()
            && let Some(fields) = root.as_object_mut()
        {
            fields.insert("extensions".to_string(), extensions.clone());
        }
        crate::reader::check_annotations(&root, annotations)?;
    }
    Ok(tree)
}

/// `validateTree`: runs the reader checks on a tree (C2-A-188).
///
/// # Errors
///
/// [`Error::Tree`] for the refusal.
pub fn validate_tree(tree: &Tree) -> Result<(), Error> {
    validate_tree_with(tree, TreeReadOptions::default())
}

/// Validates a tree and optionally checks its annotation ranges.
///
/// # Errors
///
/// [`Error::Tree`] for a refused tree or sidecar.
pub fn validate_tree_with(tree: &Tree, options: TreeReadOptions<'_>) -> Result<(), Error> {
    crate::reader::read_tree_json(tree.json())?;
    if let Some(annotations) = options.annotations {
        let root: Value = deep_json(tree.json())
            .map_err(|e: serde_json::Error| crate::reader::TreeError::Json(e.to_string()))?;
        crate::reader::check_annotations(&root, annotations)?;
    }
    Ok(())
}

/// Validates a version-2 source-layout record independently of a tree (C2-L-058).
///
/// # Errors
///
/// [`Error::Layout`] with a JSON Pointer to the rejected field.
pub fn validate_layout(layout: &Value) -> Result<(), Error> {
    layout::validate(layout)
}

/// `migrateTree` (C2-A-178): a 1.x or draft tree, bare or in an envelope,
/// upgraded to contract 2.0.
///
/// # Errors
///
/// As [`migrate_tree_with`].
pub fn migrate_tree(json: &Value) -> Result<Tree, Error> {
    migrate_tree_with(json, &MigrateOptions::default()).map(|m| m.tree)
}
