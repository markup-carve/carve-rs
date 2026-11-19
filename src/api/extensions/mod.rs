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

mod callouts;
mod code_group;
mod color_swatch;
mod details;
mod fenced;
mod list_table;
mod math;
mod transforms;
mod wikilinks;

pub use callouts::code_callouts;
pub use code_group::{CodeGroupMode, CodeGroupOptions, Highlighter, code_group};
pub use color_swatch::{ColorSwatchOptions, SwatchPosition, SwatchShape, color_swatch};
pub use details::{details, spoiler};
pub use fenced::{
    ContentMode, DiagramBuilder, FencedRenderOptions, PresetBuilders, PresetOptions, Tag, abc,
    chart, d2, fenced_render, fenced_render_name, graphviz, mermaid, plantuml, presets, vega_lite,
    wavedrom,
};
pub use list_table::list_table;
pub use math::{MathBlockOptions, MathBuilder, math_block};
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
    match name {
        "carve.mathBlock" => Some(math_block(MathBlockOptions::default())),
        "carve.codeGroup" => Some(code_group(CodeGroupOptions::default())),
        "carve.codeCallouts" => Some(code_callouts()),
        "carve.details" => Some(details()),
        "carve.spoiler" => Some(spoiler()),
        "carve.listTable" => Some(list_table()),
        "carve.colorSwatch" => Some(color_swatch(ColorSwatchOptions::default())),
        "carve.tabNormalize" => tab_normalize(2).ok(),
        "carve.inlineFootnotesToParentheses" => Some(inline_footnotes_to_parentheses()),
        _ => None,
    }
}

/// The registered matchers of a host.
#[derive(Debug, Clone, Default)]
pub(crate) struct Matchers {
    list: Vec<Arc<Matcher>>,
}

impl Matchers {
    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty()
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
    /// `opener` gives the source name of the div at a byte offset;
    /// `declared` the declaration of a qualified name.
    pub(crate) fn apply(
        &self,
        doc: &mut Value,
        opener: &dyn Fn(usize) -> Option<String>,
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
    opener: &'a dyn Fn(usize) -> Option<String>,
    declared: &'a dyn Fn(&str) -> Option<ExtensionDeclaration>,
    definitions: Vec<Value>,
    used: Vec<ExtensionDeclaration>,
    entries: Vec<Entry>,
}

impl Pass<'_> {
    /// The source name of a code block or div.
    fn source_name(&self, node: &Map<String, Value>) -> Option<String> {
        match node.get("type").and_then(Value::as_str)? {
            "code_block" => node
                .get("syntax")
                .and_then(Value::as_str)
                .map(str::to_string),
            "div" => {
                let at = node.get("pos")?.get("startOffset")?.as_u64()?;
                (self.opener)(usize::try_from(at).ok()?)
            }
            _ => None,
        }
    }

    fn fail(&mut self, code: &str, message: &str) {
        self.entries.push(entry(code, message));
    }

    fn claim(&mut self, map: &Map<String, Value>) -> Option<Map<String, Value>> {
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
            let node = node.get_or_insert_with(|| Value::Object(map.clone()));
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

/// `node` with its attributes replaced by `id` (kept), `classes` and `key_values`.
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
    let mut out = node.clone();
    if let Some(map) = out.as_object_mut() {
        if attrs.is_empty() {
            map.remove("attrs");
        } else {
            map.insert("attrs".into(), Value::Object(attrs));
        }
    }
    out
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
