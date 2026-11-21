//! PROVISIONAL: host matchers (C2-X-005 to C2-X-031) and the standard
//! extensions built on them, ported from the carve-js reference. A factory
//! returns a [`Bundle`]; [`super::Carve::register`] adds it to a host, and
//! [`super::Carve::enable_standard`] selects one by qualified name with its
//! default options, which is also how the JSON boundary selects them.
//!
//! ```
//! use carve::api::{Carve, Options, extensions};
//!
//! let mut carve = Carve::new();
//! carve.register(extensions::mermaid(extensions::PresetOptions::default()))?;
//! let html = carve.render("```mermaid\ngraph TD; A-->B\n```", &Options::default())?;
//! assert_eq!(html, r#"<pre class="mermaid" role="img" aria-label="mermaid">graph TD; A-->B</pre>"#);
//! # Ok::<(), carve::api::Error>(())
//! ```

mod apparatus;
mod callouts;
mod citations;
mod code_group;
mod color_swatch;
mod details;
mod fenced;
mod glossary;
mod index_terms;
mod list_table;
mod math;
mod navigation;
mod semantic_span;
mod smart_quotes;
mod transforms;
mod wikilinks;

pub use callouts::code_callouts;
pub use citations::{CitationMode, CitationsOptions, citations};
pub use code_group::{CodeGroupMode, CodeGroupOptions, Highlighter, code_group};
pub use color_swatch::{ColorSwatchOptions, SwatchPosition, SwatchShape, color_swatch};
pub use details::{details, spoiler};
pub use fenced::{
    ContentMode, DiagramBuilder, FencedRenderOptions, PresetBuilders, PresetOptions, Tag, abc,
    chart, d2, fenced_render, fenced_render_name, graphviz, mermaid, plantuml, presets, vega_lite,
    wavedrom,
};
pub use glossary::glossary;
pub use index_terms::{IndexOptions, index};
pub use list_table::list_table;
pub use math::{MathBlockOptions, MathBuilder, math_block};
pub use navigation::{
    AutolinkOptions, CrossrefStyle, ExternalLinksOptions, HeadingLevelShiftOptions,
    HeadingNumbersOptions, HeadingPermalinksOptions, HeadingReferenceOptions, LinkPolicyOptions,
    ListType, Position, TableOfContentsOptions, autolink, external_links, heading_level_shift,
    heading_numbers, heading_permalinks, heading_reference, link_policy, table_of_contents,
    toc_placement,
};
pub use semantic_span::{SEMANTIC_SPAN_KEYS, semantic_span};
pub use smart_quotes::{
    QuoteCharacters, SMART_QUOTE_LOCALES, SmartQuotesOptions, is_smart_quote_locale_supported,
    smart_quote_locales, smart_quotes,
};
pub use transforms::{
    DefaultAttributes, default_attributes, inline_footnotes_to_parentheses, tab_normalize,
};
pub use wikilinks::{UrlGenerator, WikilinksOptions, wikilinks};

use super::{Entry, Error, Extension, ExtensionDeclaration};
use serde_json::{Map, Value, json};
use std::sync::Arc;

/// The source form a matcher claims (C2-X-008 to C2-X-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A fenced code block of a language word.
    Fence,
    /// A div of a source name (the word after the colon run).
    Div,
    /// An unqualified inline extension `:name[...]`.
    Inline,
}

impl Form {
    const fn word(self) -> &'static str {
        match self {
            Self::Fence => "fence",
            Self::Div => "div",
            Self::Inline => "inline",
        }
    }
}

/// What a claimer receives: the core node and its source record.
pub struct Claim<'a> {
    pub form: Form,
    /// The source name the matcher claims.
    pub name: &'a str,
    /// The core node, as its JSON value.
    pub node: &'a Value,
    /// The source record the claimed node carries (C2-X-024).
    pub source: &'a Value,
    names: &'a dyn Fn(&Value) -> Option<String>,
}

impl Claim<'_> {
    /// The source name of a div or fence in the claimed node's body.
    #[must_use]
    pub fn source_name(&self, node: &Value) -> Option<String> {
        (self.names)(node)
    }
}

/// Host code that turns a claim into an extension node, or declines it.
pub trait Claimer: Send + Sync {
    /// The extension node (`block_extension` for a fence or div,
    /// `inline_extension` for an inline form) without `source`, `pos` and
    /// `attrs`, which the engine attaches; `None` keeps the core node.
    ///
    /// # Errors
    ///
    /// Any error; the core node stays and `extension-transform-failed` is
    /// reported.
    fn claim(&self, claim: &Claim<'_>) -> Result<Option<Value>, Error>;

    /// Whether the claimer is engine code whose nodes need no read-back.
    #[doc(hidden)]
    fn engine(&self) -> bool {
        false
    }

    /// The typed form of an engine claimer, which claims in the arena.
    #[doc(hidden)]
    fn typed(&self) -> Option<&Typed> {
        None
    }
}

/// An engine claimer on the arena tree, with the result of the JSON one
/// (carve-rs#4366): given the source name, the candidate and the arena, it
/// builds the parts of the extension node and moves the core node into its
/// fallback, or declines and leaves the candidate as it was.
#[doc(hidden)]
pub struct Typed(
    pub(crate) for<'a> fn(&str, &mut crate::ast::Node<'a>, &'a crate::Arena) -> Option<Made<'a>>,
);

/// What a [`Typed`] claimer makes; the engine attaches name, version,
/// source, attributes and position.
#[doc(hidden)]
pub struct Made<'a> {
    pub(crate) props: Option<Value>,
    pub(crate) payload: Option<Value>,
    pub(crate) slots: Option<Value>,
    pub(crate) fallback: &'a mut [crate::ast::Node<'a>],
}

/// The core node moved out of `node` without its attributes, as the one
/// fallback block of a claim (C2-X-035).
pub(crate) fn moved_fallback<'a>(
    node: &mut crate::ast::Node<'a>,
    arena: &'a crate::Arena,
) -> &'a mut [crate::ast::Node<'a>] {
    let placeholder = crate::ast::Node::ThematicBreak(arena.alloc(Default::default()));
    let mut core = std::mem::replace(node, placeholder);
    if let crate::ast::Node::CodeBlock(code) = &mut core {
        code.attrs = None;
    }
    arena.one(core)
}

/// An engine claimer with its typed form.
pub(crate) struct TypedEngine(pub(crate) fn(&Claim<'_>) -> Option<Value>, pub(crate) Typed);

impl Claimer for TypedEngine {
    fn claim(&self, claim: &Claim<'_>) -> Result<Option<Value>, Error> {
        Ok((self.0)(claim))
    }

    fn engine(&self) -> bool {
        true
    }

    fn typed(&self) -> Option<&Typed> {
        Some(&self.1)
    }
}

/// A claimer of this crate's standard extensions.
pub(crate) struct Engine(pub(crate) fn(&Claim<'_>) -> Option<Value>);

impl Claimer for Engine {
    fn claim(&self, claim: &Claim<'_>) -> Result<Option<Value>, Error> {
        Ok((self.0)(claim))
    }

    fn engine(&self) -> bool {
        true
    }
}

impl<F> Claimer for F
where
    F: Fn(&Claim<'_>) -> Option<Value> + Send + Sync,
{
    fn claim(&self, claim: &Claim<'_>) -> Result<Option<Value>, Error> {
        Ok(self(claim))
    }
}

/// A matcher: the source form and name it claims, and the extension its
/// claims produce.
pub struct Matcher {
    /// The qualified name of the extension the claim produces.
    pub extension: String,
    pub form: Form,
    pub name: String,
    pub claimer: Box<dyn Claimer>,
}

impl std::fmt::Debug for Matcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Matcher")
            .field("extension", &self.extension)
            .field("form", &self.form)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The registrations of one or more standard extensions.
#[derive(Debug, Default)]
pub struct Bundle {
    pub extensions: Vec<Extension>,
    pub matchers: Vec<Matcher>,
    pub host: HostFields,
}

/// Span keys from inside out, each with the attribute its value fills.
pub type SemanticKeys = &'static [(&'static str, Option<&'static str>)];

/// What a bundle sets on the host beside its registrations; the last
/// registration that sets a field wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostFields {
    /// The C2-H-075 span keys, replacing `abbr`, `time` and `kbd`.
    pub semantic_keys: Option<SemanticKeys>,
    /// Open and close double, then open and close single quote glyphs.
    pub quotes: Option<[String; 4]>,
}

impl HostFields {
    pub(crate) const fn is_empty(&self) -> bool {
        self.semantic_keys.is_none() && self.quotes.is_none()
    }

    pub(crate) fn merge(&mut self, later: Self) {
        if later.semantic_keys.is_some() {
            self.semantic_keys = later.semantic_keys;
        }
        if later.quotes.is_some() {
            self.quotes = later.quotes;
        }
    }
}

impl Bundle {
    /// Merges bundles into one; two claims of one source name fail at
    /// registration (C2-X-014).
    #[must_use]
    pub fn merge(bundles: impl IntoIterator<Item = Self>) -> Self {
        let mut out = Self::default();
        for b in bundles {
            out.extensions.extend(b.extensions);
            out.matchers.extend(b.matchers);
            out.host.merge(b.host);
        }
        out
    }

    /// The bundle without its renderers: its nodes render their fallback.
    #[must_use]
    pub fn without_renderers(mut self) -> Self {
        for e in &mut self.extensions {
            e.renderers.clear();
        }
        self
    }
}

pub(crate) fn declaration(name: &str, slots: &[&str]) -> ExtensionDeclaration {
    ExtensionDeclaration {
        name: name.to_string(),
        version: "1".to_string(),
        required: false,
        slots: slots.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// The standard extensions of this module by qualified name, with default
/// options; `None` for a name that is not one.
pub(crate) fn standard(name: &str) -> Option<Bundle> {
    if let Some(word) = name.strip_prefix("carve.fencedRender.") {
        return fenced::preset_word(word);
    }
    apparatus_standard(name)
        .or_else(|| standard_other(name))
        .or_else(|| navigation_standard(name))
}

fn apparatus_standard(name: &str) -> Option<Bundle> {
    match name {
        "carve.glossary" | "carve.term" => Some(glossary()),
        "carve.index" | "carve.indexTerm" => Some(index(IndexOptions::default())),
        // The named carve.cite path is the C2-X-058 core-text matcher.
        // The full citation bundle is registered explicitly or by references.
        "carve.references" => Some(citations(CitationsOptions::default())),
        _ => None,
    }
}

fn standard_other(name: &str) -> Option<Bundle> {
    match name {
        "carve.mathBlock" => Some(math_block(MathBlockOptions::default())),
        "carve.codeGroup" => Some(code_group(CodeGroupOptions::default())),
        "carve.codeCallouts" => Some(code_callouts()),
        "carve.details" => Some(details()),
        "carve.spoiler" => Some(details::spoiler_inline()),
        "carve.spoilerBlock" => Some(details::spoiler_block()),
        "carve.listTable" => Some(list_table()),
        "carve.colorSwatch" => Some(color_swatch(ColorSwatchOptions::default())),
        "carve.tabNormalize" => tab_normalize(2).ok(),
        "carve.inlineFootnotesToParentheses" => Some(inline_footnotes_to_parentheses()),
        "carve.semanticSpan" => Some(semantic_span()),
        "carve.smartQuotes" => Some(smart_quotes(&SmartQuotesOptions::default())),
        _ => None,
    }
}

fn navigation_standard(name: &str) -> Option<Bundle> {
    match name {
        "carve.externalLinks" => Some(external_links(ExternalLinksOptions::default())),
        "carve.headingLevelShift" => Some(heading_level_shift(HeadingLevelShiftOptions::default())),
        "carve.headingPermalinks" => Some(heading_permalinks(HeadingPermalinksOptions::default())),
        "carve.headingNumbers" => Some(heading_numbers(HeadingNumbersOptions::default())),
        "carve.headingReference" => Some(heading_reference(HeadingReferenceOptions::default())),
        "carve.toc" => Some(table_of_contents(TableOfContentsOptions::default())),
        "carve.tocPlacement" => Some(toc_placement()),
        "carve.autolink" => Some(autolink(AutolinkOptions::default())),
        "carve.linkPolicy" => Some(link_policy(LinkPolicyOptions::default())),
        _ => None,
    }
}

/// The registered matchers of a host.
#[derive(Debug, Clone, Default)]
pub(crate) struct Matchers {
    list: Vec<Arc<Matcher>>,
}

impl Matchers {
    /// Claim on the owned AST, crossing into JSON only for a candidate node
    /// passed to host code. The source writer still sees the same node value.
    pub(crate) fn apply_owned(
        &self,
        doc: &mut crate::ast::owned::Document,
        opener: &dyn Fn(Option<&str>, usize) -> Option<String>,
        declared: &dyn Fn(&str) -> Option<ExtensionDeclaration>,
    ) -> Result<Vec<Entry>, Error> {
        let definitions = doc
            .children
            .iter()
            .filter(|node| matches!(node, crate::ast::owned::Node::LinkReferenceDefinition(_)))
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::option("extensions", e.to_string()))?;
        let mut pass = Pass {
            matchers: self,
            opener,
            declared,
            definitions,
            used: Vec::new(),
            entries: Vec::new(),
        };
        for node in &mut doc.children {
            owned_walk(node, &mut pass, 0)?;
        }
        let (used, entries) = (pass.used, pass.entries);
        if let Some(extensions) = declare_used(&used, doc.extensions.as_ref()) {
            doc.extensions = Some(extensions);
        }
        Ok(entries)
    }

    /// [`Matchers::apply_owned`] of a tree in an arena. Only a node a matcher
    /// can claim, and an extension node with slots, crosses into the owned
    /// tree; the rest of the tree is walked in place (carve-rs#4366). True in
    /// the second place when a claim changed the tree.
    pub(crate) fn apply_arena<'a>(
        &self,
        doc: &mut crate::ast::Document<'a>,
        arena: &'a crate::Arena,
        opener: &dyn Fn(Option<&str>, usize) -> Option<String>,
        declared: &dyn Fn(&str) -> Option<ExtensionDeclaration>,
    ) -> Result<(Vec<Entry>, bool), Error> {
        use crate::ast::Node;
        fn walk<'a>(
            node: &mut Node<'a>,
            arena: &'a crate::Arena,
            pass: &mut Pass<'_>,
            depth: usize,
        ) -> Result<(), Error> {
            if depth > crate::resolve::WALK_CAP {
                return Ok(());
            }
            let slots = match node {
                Node::BlockExtension(ext) => ext.slots.is_some(),
                Node::InlineExtension(ext) => ext.slots.is_some(),
                _ => false,
            };
            let candidate = !slots && pass.may_claim(node);
            let typed = if candidate {
                pass.claim_typed(node, arena, depth)?
            } else {
                None
            };
            match typed {
                Some(true) => return Ok(()),
                Some(false) => {}
                None if slots || candidate => {
                    let mut owned = node.owned();
                    owned_walk(&mut owned, pass, depth)?;
                    *node = owned.in_arena(arena);
                    return Ok(());
                }
                None => {}
            }
            let mut failure = None;
            let mut visit = |child: &mut Node<'a>| {
                if failure.is_none() {
                    failure = walk(child, arena, pass, depth + 1).err();
                }
            };
            match node {
                Node::BlockExtension(ext) => ext.children.iter_mut().for_each(&mut visit),
                Node::InlineExtension(ext) => ext.children.iter_mut().for_each(&mut visit),
                _ => node.for_each_child_mut(&mut visit),
            }
            failure.map_or(Ok(()), Err)
        }
        let definitions = doc
            .children
            .iter()
            .filter(|node| matches!(node, Node::LinkReferenceDefinition(_)))
            .map(|node| serde_json::to_value(node.owned()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::option("extensions", e.to_string()))?;
        let mut pass = Pass {
            matchers: self,
            opener,
            declared,
            definitions,
            used: Vec::new(),
            entries: Vec::new(),
        };
        for node in doc.children.iter_mut() {
            walk(node, arena, &mut pass, 0)?;
        }
        let (used, entries) = (pass.used, pass.entries);
        // Every claim declares the extension it used.
        let changed = !used.is_empty();
        if let Some(extensions) = declare_used(&used, doc.extensions) {
            doc.extensions = Some(arena.json(extensions));
        }
        Ok((entries, changed))
    }

    pub(crate) fn has_form(&self, form: Form) -> bool {
        self.list.iter().any(|m| m.form == form)
    }

    pub(crate) fn names(&self, form: Form) -> impl Iterator<Item = &str> {
        self.list
            .iter()
            .filter(move |m| m.form == form)
            .map(|m| m.name.as_str())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Whether every matcher's claimer is engine code.
    pub(crate) fn engine_only(&self) -> bool {
        self.list.iter().all(|m| m.claimer.engine())
    }

    pub(crate) fn check(&self, m: &Matcher) -> Result<(), Error> {
        if self
            .list
            .iter()
            .any(|o| o.form == m.form && o.name == m.name)
        {
            return Err(Error::option(
                "extensions",
                format!(
                    "duplicate claim of the {} name {} by {}",
                    m.form.word(),
                    m.name,
                    m.extension
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn push(&mut self, m: Matcher) {
        self.list.push(Arc::new(m));
    }

    /// C2-X-005: claims core nodes in one pass after the core parse, outer
    /// nodes first, and declares the extensions the claims use (C2-A-017).
    /// `opener` gives the source name of the div at a file id and byte offset;
    /// `declared` the declaration of a qualified name.
    pub(crate) fn apply(
        &self,
        doc: &mut Value,
        opener: &dyn Fn(Option<&str>, usize) -> Option<String>,
        declared: &dyn Fn(&str) -> Option<ExtensionDeclaration>,
    ) -> Vec<Entry> {
        let mut pass = Pass {
            matchers: self,
            opener,
            declared,
            definitions: super::standard::definitions(doc),
            used: Vec::new(),
            entries: Vec::new(),
        };
        let mut pending: Vec<&mut Value> = doc.get_mut("children").into_iter().collect();
        while let Some(v) = pending.pop() {
            match v {
                Value::Array(items) => pending.extend(items.iter_mut().rev()),
                Value::Object(map) => {
                    if let Some(next) = pass.claim(map) {
                        *map = next;
                    }
                    let extension = is_extension(map);
                    pending.extend(
                        map.iter_mut()
                            .filter(|(key, _)| descends(key, extension))
                            .map(|(_, child)| child),
                    );
                }
                _ => {}
            }
        }
        let Pass { used, entries, .. } = pass;
        if !used.is_empty() {
            let names: Vec<(&str, Value)> = used
                .iter()
                .map(|d| (d.name.as_str(), json!(d.slots)))
                .collect();
            super::standard::declare_names(doc, &names);
        }
        entries
    }
}

type OwnedNode = crate::ast::owned::Node;

/// The claims inside the JSON slots of an extension node.
fn slot(value: &mut Value, pass: &mut Pass<'_>, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                slot(item, pass, depth + 1);
            }
        }
        Value::Object(map) => {
            if let Some(next) = pass.claim(map) {
                *map = next;
            }
            let extension = is_extension(map);
            for (key, child) in map.iter_mut() {
                if descends(key, extension) {
                    slot(child, pass, depth + 1);
                }
            }
        }
        _ => {}
    }
}

fn claim_candidate(node: &mut OwnedNode, pass: &mut Pass<'_>) -> Result<(), Error> {
    let fits = match node {
        OwnedNode::CodeBlock(code) => pass.names(Form::Fence, code.syntax.as_deref()),
        OwnedNode::Div(div) => pass.div_fits(div.pos.as_ref()),
        OwnedNode::InlineExtension(ext) => {
            ext.version.is_none() && pass.names(Form::Inline, Some(&ext.name))
        }
        _ => false,
    };
    // Only a node a matcher takes crosses into JSON for its claimer.
    if !fits {
        return Ok(());
    }
    let value =
        serde_json::to_value(&*node).map_err(|e| Error::option("extensions", e.to_string()))?;
    if let Some(next) = pass.claim_value(&value) {
        *node = serde_json::from_value(Value::Object(next))
            .map_err(|e| Error::option("extensions", e.to_string()))?;
    }
    Ok(())
}

/// The claim walk of the owned tree: a candidate is claimed first, then the
/// walk enters the node that stands in its place.
fn owned_walk(node: &mut OwnedNode, pass: &mut Pass<'_>, depth: usize) -> Result<(), Error> {
    if depth > crate::resolve::WALK_CAP {
        return Ok(());
    }
    claim_candidate(node, pass)?;
    match node {
        OwnedNode::BlockExtension(ext) => {
            for child in &mut ext.children {
                owned_walk(child, pass, depth + 1)?;
            }
            if let Some(slots) = &mut ext.slots {
                slot(slots, pass, depth + 1);
            }
        }
        OwnedNode::InlineExtension(ext) => {
            for child in &mut ext.children {
                owned_walk(child, pass, depth + 1)?;
            }
            if let Some(slots) = &mut ext.slots {
                slot(slots, pass, depth + 1);
            }
        }
        _ => {
            let mut failure = None;
            node.for_each_child_mut(&mut |child| {
                if failure.is_none() {
                    failure = owned_walk(child, pass, depth + 1).err();
                }
            });
            if let Some(error) = failure {
                return Err(error);
            }
        }
    }
    Ok(())
}

/// The `extensions` of a tree with the declarations of the claims used
/// added (C2-A-017); `None` when no claim was used.
fn declare_used(used: &[ExtensionDeclaration], extensions: Option<&Value>) -> Option<Value> {
    if used.is_empty() {
        return None;
    }
    let names: Vec<(&str, Value)> = used
        .iter()
        .map(|d| (d.name.as_str(), json!(d.slots)))
        .collect();
    let mut declaration = json!({"extensions": extensions});
    super::standard::declare_names(&mut declaration, &names);
    declaration.get("extensions").cloned()
}

fn is_extension(map: &Map<String, Value>) -> bool {
    matches!(
        map.get("type").and_then(Value::as_str),
        Some("block_extension" | "inline_extension")
    )
}

/// Whether the claim pass walks into the member `key`.
fn descends(key: &str, extension: bool) -> bool {
    let skipped = matches!(
        key,
        "source" | "pos" | "attrs" | "props" | "payload" | "data"
    );
    !(skipped || (extension && key == "fallback"))
}

/// The state of one claim pass.
struct Pass<'a> {
    matchers: &'a Matchers,
    opener: &'a dyn Fn(Option<&str>, usize) -> Option<String>,
    declared: &'a dyn Fn(&str) -> Option<ExtensionDeclaration>,
    definitions: Vec<Value>,
    used: Vec<ExtensionDeclaration>,
    entries: Vec<Entry>,
}

impl Pass<'_> {
    /// Whether a matcher of `form` claims the source name `name`.
    fn names(&self, form: Form, name: Option<&str>) -> bool {
        name.is_some_and(|name| {
            self.matchers
                .list
                .iter()
                .any(|m| m.form == form && m.name == name)
        })
    }

    /// Whether a div matcher claims the div opened at `pos` of the top file.
    fn div_fits(&self, pos: Option<&crate::ast::Pos>) -> bool {
        self.matchers.has_form(Form::Div)
            && pos.is_some_and(|pos| {
                self.names(Form::Div, (self.opener)(None, pos.start()).as_deref())
            })
    }

    /// [`claim_candidate`]'s test of an arena node.
    fn may_claim(&self, node: &crate::ast::Node<'_>) -> bool {
        use crate::ast::Node;
        match node {
            Node::CodeBlock(code) => self.names(Form::Fence, code.syntax),
            Node::Div(div) => self.div_fits(div.pos.as_ref()),
            Node::InlineExtension(ext) => {
                ext.version.is_none() && self.names(Form::Inline, Some(ext.name))
            }
            _ => false,
        }
    }

    /// The claim of a candidate by a typed engine claimer: `None` when its
    /// matcher has none, so the owned claim runs; `Some(true)` when it was
    /// claimed, with the claims inside its slots made; `Some(false)` when the
    /// core node stays and the walk enters it.
    fn claim_typed<'a>(
        &mut self,
        node: &mut crate::ast::Node<'a>,
        arena: &'a crate::Arena,
        depth: usize,
    ) -> Result<Option<bool>, Error> {
        use crate::ast::Node;
        let (form, name) = match node {
            Node::CodeBlock(code) => (Form::Fence, code.syntax.map(str::to_string)),
            Node::Div(div) => (
                Form::Div,
                div.pos.and_then(|pos| (self.opener)(None, pos.start())),
            ),
            _ => return Ok(None),
        };
        let matchers = self.matchers;
        let Some(m) = name.and_then(|name| {
            matchers
                .list
                .iter()
                .find(|m| m.form == form && m.name == name)
        }) else {
            return Ok(None);
        };
        let (Some(typed), Some(decl)) = (m.claimer.typed(), (self.declared)(&m.extension)) else {
            return Ok(None);
        };
        let source = match node {
            Node::CodeBlock(code) => Some(typed_fence_source(&m.name, code)?),
            _ => {
                let value = serde_json::to_value(&*node)
                    .map_err(|e| Error::option("extensions", e.to_string()))?;
                super::standard::div_source(&value, &m.name, &self.definitions)
            }
        };
        let Some(source) = source else {
            self.fail(
                "extension-transform-failed",
                &format!(
                    "The extension {} could not write its claim source; the core node was retained",
                    m.extension
                ),
            );
            return Ok(Some(false));
        };
        let (attrs, pos) = match node {
            Node::CodeBlock(code) => (code.attrs, code.pos),
            Node::Div(div) => (div.attrs.and_then(|a| remaining_typed(a, arena)), div.pos),
            _ => return Ok(None),
        };
        let Some(made) = (typed.0)(&m.name, node, arena) else {
            return Ok(Some(false));
        };
        let mut slots = made.slots;
        if let Some(slots) = &mut slots {
            slot(slots, self, depth + 1);
        }
        let ext = crate::ast::BlockExtension {
            name: arena.str(&m.extension),
            version: arena.str(&decl.version),
            props: made.props.map(|v| arena.json(v)),
            payload: made.payload.map(|v| arena.json(v)),
            slots: slots.map(|v| arena.json(v)),
            fallback: made.fallback,
            source: Some(arena.json(source)),
            attrs,
            pos,
            ..Default::default()
        };
        *node = Node::BlockExtension(arena.alloc(ext));
        if !self.used.iter().any(|d| d.name == decl.name) {
            self.used.push(decl);
        }
        Ok(Some(true))
    }

    /// The source name of a code block or div.
    fn source_name(&self, node: &Map<String, Value>) -> Option<String> {
        match node.get("type").and_then(Value::as_str)? {
            "code_block" => node
                .get("syntax")
                .and_then(Value::as_str)
                .map(str::to_string),
            "div" => {
                let at = node.get("pos")?.get("startOffset")?.as_u64()?;
                (self.opener)(
                    node.get("pos")?.get("file").and_then(Value::as_str),
                    usize::try_from(at).ok()?,
                )
            }
            _ => None,
        }
    }

    fn fail(&mut self, code: &str, message: &str) {
        self.entries.push(entry(code, message));
    }

    fn claim(&mut self, map: &Map<String, Value>) -> Option<Map<String, Value>> {
        self.claim_in(map, None)
    }

    /// [`Pass::claim`] of a node the caller holds as a whole value, which the
    /// claim reads without a copy.
    fn claim_value(&mut self, node: &Value) -> Option<Map<String, Value>> {
        self.claim_in(node.as_object()?, Some(node))
    }

    fn claim_in(
        &mut self,
        map: &Map<String, Value>,
        whole: Option<&Value>,
    ) -> Option<Map<String, Value>> {
        let ty = map.get("type").and_then(Value::as_str)?;
        if !matches!(ty, "code_block" | "div" | "inline_extension") {
            return None;
        }
        let name = self.source_name(map);
        let mut node = None;
        let matchers = self.matchers;
        for m in &matchers.list {
            if !fits(m, ty, name.as_deref(), map) {
                continue;
            }
            let node: &Value = match whole {
                Some(whole) => whole,
                None => node.get_or_insert_with(|| Value::Object(map.clone())),
            };
            match self.try_claim(m, map, node) {
                Attempt::Declined => {}
                Attempt::Retained => return None,
                Attempt::Claimed(out) => return Some(out),
            }
        }
        None
    }

    fn try_claim(&mut self, m: &Matcher, map: &Map<String, Value>, node: &Value) -> Attempt {
        let Some(source) = source_record(m.form, &m.name, map, &self.definitions) else {
            self.fail(
                "extension-transform-failed",
                &format!(
                    "The extension {} could not write its claim source; the core node was retained",
                    m.extension
                ),
            );
            return Attempt::Retained;
        };
        let names = |n: &Value| n.as_object().and_then(|n| self.source_name(n));
        let claim = Claim {
            form: m.form,
            name: &m.name,
            node,
            source: &source,
            names: &names,
        };
        let made = match m.claimer.claim(&claim) {
            Ok(None) => return Attempt::Declined,
            Ok(Some(made)) => made,
            Err(e) => {
                self.fail(
                    "extension-transform-failed",
                    &format!(
                        "The extension {} transform failed: {e}; the core source node was retained",
                        m.extension
                    ),
                );
                return Attempt::Retained;
            }
        };
        let decl = (self.declared)(&m.extension);
        let Some((decl, out)) =
            decl.and_then(|d| finish(m, made, map, source, &d).map(|out| (d, out)))
        else {
            self.fail(
                "extension-fallback-rendered",
                &format!(
                    "Invalid transform of {}; the core source node was retained",
                    m.extension
                ),
            );
            return Attempt::Retained;
        };
        if !self.used.iter().any(|d| d.name == decl.name) {
            self.used.push(decl);
        }
        Attempt::Claimed(out)
    }
}

enum Attempt {
    Declined,
    Retained,
    Claimed(Map<String, Value>),
}

fn fits(m: &Matcher, ty: &str, name: Option<&str>, map: &Map<String, Value>) -> bool {
    match m.form {
        Form::Fence => ty == "code_block" && name == Some(m.name.as_str()),
        Form::Div => ty == "div" && name == Some(m.name.as_str()),
        Form::Inline => {
            ty == "inline_extension"
                && map.get("version").is_none()
                && map.get("name").and_then(Value::as_str) == Some(m.name.as_str())
        }
    }
}

/// C2-X-031: the claimed node of the right type, name and version, with the
/// source, attributes and position of the core node attached; `None` when
/// it is not valid.
fn finish(
    m: &Matcher,
    made: Value,
    map: &Map<String, Value>,
    source: Value,
    decl: &ExtensionDeclaration,
) -> Option<Map<String, Value>> {
    let want = if m.form == Form::Inline {
        "inline_extension"
    } else {
        "block_extension"
    };
    let text = |k: &str| made.get(k).and_then(Value::as_str);
    if text("type") != Some(want)
        || text("name") != Some(m.extension.as_str())
        || text("version") != Some(decl.version.as_str())
    {
        return None;
    }
    let Value::Object(mut out) = made else {
        return None;
    };
    out.insert("source".into(), source);
    let attrs = map.get("attrs").and_then(|a| {
        if m.form == Form::Div {
            remaining_attrs(a)
        } else {
            Some(a.clone())
        }
    });
    if let Some(attrs) = attrs {
        out.insert("attrs".into(), attrs);
    }
    if let Some(pos) = map.get("pos") {
        out.insert("pos".into(), pos.clone());
    }
    let out = Value::Object(out);
    if !m.claimer.engine() && !reads_back(&out, decl, m.form) {
        return None;
    }
    match out {
        Value::Object(out) => Some(out),
        _ => None,
    }
}

/// C2-X-031: the claimed node passes the reader checks in a document that
/// declares its extension.
fn reads_back(node: &Value, decl: &ExtensionDeclaration, form: Form) -> bool {
    let child = if form == Form::Inline {
        json!({"type": "paragraph", "children": [node]})
    } else {
        node.clone()
    };
    let doc = json!({
        "type": "document",
        "astVersion": "2.0",
        "extensions": [{"name": decl.name, "version": decl.version, "required": false, "slots": decl.slots}],
        "children": [child],
    });
    crate::reader::read_tree_json(&doc.to_string()).is_ok()
}

fn source_record(
    form: Form,
    name: &str,
    map: &Map<String, Value>,
    definitions: &[Value],
) -> Option<Value> {
    match form {
        Form::Div => super::standard::div_source(&Value::Object(map.clone()), name, definitions),
        Form::Inline => Some(json!({
            "form": "inline",
            "name": name,
            "body": super::standard::inline_body(map, name)?,
        })),
        Form::Fence => {
            let mut source = Map::new();
            source.insert("form".into(), "fence".into());
            source.insert("name".into(), name.into());
            source.insert(
                "body".into(),
                map.get("value").cloned().unwrap_or_else(|| "".into()),
            );
            if let Some(title) = map.get("title").and_then(Value::as_array) {
                source.insert("title".into(), super::standard::plain_text(title).into());
            }
            if let Some(label) = map.get("groupLabel") {
                source.insert("groupLabel".into(), label.clone());
            }
            Some(Value::Object(source))
        }
    }
}

/// [`source_record`] of a claimed fence.
fn typed_fence_source(name: &str, code: &crate::ast::CodeBlock<'_>) -> Result<Value, Error> {
    let mut source = Map::new();
    source.insert("form".into(), "fence".into());
    source.insert("name".into(), name.into());
    source.insert("body".into(), code.value.into());
    if let Some(title) = &code.title {
        let title = serde_json::to_value(&**title)
            .map_err(|e| Error::option("extensions", e.to_string()))?;
        let title = title.as_array().map_or(&[][..], Vec::as_slice);
        source.insert("title".into(), super::standard::plain_text(title).into());
    }
    if let Some(label) = code.group_label {
        source.insert("groupLabel".into(), label.into());
    }
    Ok(Value::Object(source))
}

/// [`remaining_attrs`] of typed attributes.
fn remaining_typed<'a>(
    attrs: &'a crate::ast::Attrs<'a>,
    arena: &'a crate::Arena,
) -> Option<&'a crate::ast::Attrs<'a>> {
    let rest = crate::ast::Attrs {
        classes: attrs.classes.get(1..).unwrap_or(&[]),
        ..*attrs
    };
    (!rest.is_empty()).then(|| &*arena.alloc(rest))
}

/// C2-X-009: the attributes of a claimed div after its source name class.
fn remaining_attrs(attrs: &Value) -> Option<Value> {
    let mut rest = attrs.as_object()?.clone();
    if let Some(classes) = rest.get_mut("classes").and_then(Value::as_array_mut) {
        if !classes.is_empty() {
            classes.remove(0);
        }
        if classes.is_empty() {
            rest.remove("classes");
        }
    }
    (!rest.is_empty()).then_some(Value::Object(rest))
}

fn entry(code: &str, message: &str) -> Entry {
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

/// Whether the authored attributes hold the key `name`, ASCII case ignored.
pub(crate) fn authored(attrs: Option<&Value>, name: &str) -> bool {
    key_values(attrs).iter().any(|kv| {
        kv.get(0)
            .and_then(Value::as_str)
            .is_some_and(|k| k.eq_ignore_ascii_case(name))
    })
}

pub(crate) fn key_values(attrs: Option<&Value>) -> Vec<Value> {
    attrs
        .and_then(|a| a.get("keyValues"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn classes(attrs: Option<&Value>) -> Vec<Value> {
    attrs
        .and_then(|a| a.get("classes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The `pos` of `node` and its attributes replaced by `id` (kept), `classes`
/// and `key_values`, for an element's attribute list.
pub(crate) fn with_attrs(node: &Value, classes: Vec<Value>, key_values: Vec<Value>) -> Value {
    let mut attrs = Map::new();
    if let Some(id) = node.pointer("/attrs/id") {
        attrs.insert("id".into(), id.clone());
    }
    if !classes.is_empty() {
        attrs.insert("classes".into(), Value::Array(classes));
    }
    if !key_values.is_empty() {
        attrs.insert("keyValues".into(), Value::Array(key_values));
    }
    // Every caller hands the result to `RenderContext::attributes`, which
    // reads only `attrs` and `pos`, so the rest of the node is not copied.
    let mut out = Map::new();
    if let Some(pos) = node.get("pos") {
        out.insert("pos".into(), pos.clone());
    }
    if !attrs.is_empty() {
        out.insert("attrs".into(), Value::Object(attrs));
    }
    Value::Object(out)
}

/// The code block a fence claim carries as its fallback, without the
/// attributes that move to the extension node (C2-X-035).
pub(crate) fn bare_fallback(node: &Value) -> Value {
    let mut copy = node.clone();
    if let Some(map) = copy.as_object_mut() {
        map.remove("attrs");
    }
    copy
}

/// The fence body without its final line break.
pub(crate) fn fence_source(value: &str) -> &str {
    value.strip_suffix('\n').unwrap_or(value)
}

pub(crate) fn payload_text(node: &Value) -> &str {
    node.pointer("/payload/0/value")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// The attribute parts of an extension element, read from the typed node.
pub(crate) struct Parts<'n> {
    pub(crate) id: Option<&'n str>,
    pub(crate) classes: Vec<&'n str>,
    pub(crate) key_values: Vec<(&'n str, &'n str)>,
}

impl<'n> Parts<'n> {
    pub(crate) fn of(attrs: Option<&'n crate::ast::Attrs<'n>>) -> Self {
        Self {
            id: attrs.and_then(|a| a.id),
            classes: attrs.map_or_else(Vec::new, |a| a.classes.to_vec()),
            key_values: attrs.map_or_else(Vec::new, |a| a.key_values.to_vec()),
        }
    }

    /// [`authored`] of the typed attributes.
    pub(crate) fn authored(&self, name: &str) -> bool {
        self.key_values
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
    }

    /// `base` ahead of the authored classes.
    pub(crate) fn with_classes(mut self, base: &[&'n str]) -> Self {
        let mut classes = base.to_vec();
        classes.append(&mut self.classes);
        self.classes = classes;
        self
    }
}

/// The block extension node a typed renderer reads.
pub(crate) fn typed<'n, 'a>(
    node: &'n crate::ast::Node<'a>,
) -> Option<&'n crate::ast::BlockExtension<'a>> {
    match node {
        crate::ast::Node::BlockExtension(e) => Some(&**e),
        _ => None,
    }
}

/// [`payload_text`] of a typed node.
pub(crate) fn typed_payload<'a>(node: &crate::ast::BlockExtension<'a>) -> &'a str {
    node.payload
        .and_then(|p| p.pointer("/0/value"))
        .and_then(Value::as_str)
        .unwrap_or("")
}
