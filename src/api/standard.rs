//! PROVISIONAL: the standard extensions of `data/extensions.json`, the
//! `carve.tabs` and `carve.tab` pair (C2-X-039 to C2-X-048, C2-X-056). The
//! claim runs as a separate pass over the parsed tree (C2-X-005).

use super::Error;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Write as _;

/// The standard extensions a host enables, and whether their standard
/// renderers are registered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one flag per extension and renderer
pub(crate) struct Standard {
    pub tabs: bool,
    pub tabs_renderer: bool,
    pub tabs_aria: bool,
    pub wiki: bool,
    pub cite: bool,
    /// C2-X-059: `carve.imgFence.img` and `carve.imgFence.image`, and
    /// whether their standard renderer is registered.
    pub img: bool,
    pub image: bool,
    pub img_renderer: bool,
    pub image_renderer: bool,
}

impl Standard {
    pub(crate) fn enable(&mut self, name: &str, renderer: bool) -> Result<(), Error> {
        let slot = match name {
            "carve.tabs" => {
                self.tabs_renderer |= renderer;
                &mut self.tabs
            }
            // C2-X-057, C2-X-058: matchers with the core-text fallback and no
            // standard renderer.
            "carve.wiki" | "carve.cite" if renderer => {
                return Err(Error::option(
                    "renderers",
                    format!("{name} has no standard renderer in this engine"),
                ));
            }
            "carve.wiki" => &mut self.wiki,
            "carve.cite" => &mut self.cite,
            "carve.imgFence.img" => {
                self.img_renderer |= renderer;
                &mut self.img
            }
            "carve.imgFence.image" => {
                self.image_renderer |= renderer;
                &mut self.image
            }
            _ => {
                return Err(Error::option(
                    "extensions",
                    format!("{name} is not a standard extension of this engine"),
                ));
            }
        };
        *slot = true;
        Ok(())
    }

    pub(crate) const fn is_empty(self) -> bool {
        !(self.tabs || self.wiki || self.cite || self.img || self.image)
    }

    /// The fence matchers enabled: language and qualified name.
    pub(crate) fn fence_matchers(self) -> Vec<(&'static str, &'static str)> {
        super::img_fence::MATCHERS
            .iter()
            .filter(|(name, _)| if *name == "img" { self.img } else { self.image })
            .copied()
            .collect()
    }

    /// Whether the standard renderer of the claimed extension `name` is registered.
    pub(crate) fn renders(self, name: &str) -> bool {
        match name {
            "carve.imgFence.img" => self.img_renderer,
            "carve.imgFence.image" => self.image_renderer,
            _ => false,
        }
    }

    /// The inline matchers enabled: source name and qualified name.
    pub(crate) fn inline_matchers(self) -> Vec<(&'static str, &'static str)> {
        let mut out = Vec::new();
        if self.cite {
            out.push(("cite", "carve.cite"));
        }
        if self.wiki {
            out.push(("wiki", "carve.wiki"));
        }
        out
    }
}

/// C2-X-006, C2-X-057, C2-X-058: claims each unqualified `:name[...]` of an
/// enabled inline matcher as its extension node, the plain text of the
/// content as fallback. Returns the extensions whose claim source could not
/// be written; those core nodes stay (C2-X-051).
pub(crate) fn claim_inline(
    doc: &mut Value,
    matchers: &[(&'static str, &'static str)],
) -> Vec<&'static str> {
    let mut claimed = Vec::new();
    let mut failed = Vec::new();
    let mut pending: Vec<&mut Value> = vec![doc];
    while let Some(v) = pending.pop() {
        match v {
            Value::Array(items) => pending.extend(items.iter_mut()),
            Value::Object(map) => {
                if let Some(&(source, qualified)) = inline_matcher(map, matchers) {
                    match inline_node(map, source, qualified) {
                        Some(node) => {
                            *map = node;
                            if !claimed.contains(&qualified) {
                                claimed.push(qualified);
                            }
                        }
                        None if !failed.contains(&qualified) => failed.push(qualified),
                        None => {}
                    }
                    continue;
                }
                let extension = matches!(
                    map.get("type").and_then(Value::as_str),
                    Some("block_extension" | "inline_extension")
                );
                for (key, child) in map.iter_mut() {
                    if (extension && key == "fallback") || key == "source" || key == "pos" {
                        continue;
                    }
                    pending.push(child);
                }
            }
            _ => {}
        }
    }
    let declared: Vec<(&str, Value)> = claimed.into_iter().map(|n| (n, json!([]))).collect();
    if !declared.is_empty() {
        declare_names(doc, &declared);
    }
    failed
}

fn inline_matcher<'m>(
    map: &Map<String, Value>,
    matchers: &'m [(&'static str, &'static str)],
) -> Option<&'m (&'static str, &'static str)> {
    if map.get("type").and_then(Value::as_str) != Some("inline_extension")
        || map.get("version").is_some()
    {
        return None;
    }
    let name = map.get("name").and_then(Value::as_str)?;
    matchers.iter().find(|(source, _)| *source == name)
}

fn inline_node(
    node: &Map<String, Value>,
    source: &str,
    qualified: &str,
) -> Option<Map<String, Value>> {
    let children = node
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let body = inline_body(node, source)?;
    let mut out = Map::new();
    out.insert("type".into(), "inline_extension".into());
    out.insert("name".into(), qualified.into());
    out.insert("version".into(), "1".into());
    out.insert("children".into(), json!([]));
    let text = plain_text(&children);
    let text = if source == "wiki" {
        text.split_once('|')
            .map_or(text.as_str(), |(_, display)| display)
            .to_string()
    } else {
        text
    };
    let fallback = if text.is_empty() {
        json!([])
    } else {
        json!([{"type": "text", "value": text}])
    };
    out.insert("fallback".into(), fallback);
    out.insert(
        "source".into(),
        json!({"form": "inline", "name": source, "body": body}),
    );
    if let Some(pos) = node.get("pos") {
        out.insert("pos".into(), pos.clone());
    }
    if let Some(attrs) = node.get("attrs") {
        out.insert("attrs".into(), attrs.clone());
    }
    Some(out)
}

/// C2-X-039 to C2-X-042: claims each `tabs` div whose body is one or more
/// `tab` divs with a nonempty quoted title. `opener` gives the source name of
/// the div at a file id and byte offset; `false` means the claim source could not be
/// written and the core node stays (C2-X-051).
pub(crate) fn claim_tabs(
    doc: &mut Value,
    opener: &dyn Fn(Option<&str>, usize) -> Option<String>,
) -> bool {
    let cx = Claim {
        opener,
        keys: crate::write::link_keys(at(doc, "/children").as_array().into_iter().flatten()),
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
    opener: &'a dyn Fn(Option<&str>, usize) -> Option<String>,
    keys: crate::write::LinkKeys,
    failed: RefCell<bool>,
}

impl Claim<'_> {
    fn name(&self, node: &Value) -> Option<String> {
        if at(node, "/type") != "div" {
            return None;
        }
        let offset = at(node, "/pos/startOffset").as_u64()?;
        (self.opener)(
            at(node, "/pos/file").as_str(),
            usize::try_from(offset).ok()?,
        )
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
        div_source(node, name, &self.keys)
    }
}

/// `Carve::default().write` of a document of the engine's own claim input,
/// as its JSON value: the reader checks and the typed decode are skipped,
/// the writer reads the value (carve-rs#4366).
fn write_claim_source(doc: &Value, settings: &crate::write::Settings) -> Option<String> {
    let (text, entries) = crate::write::write_value(doc, settings);
    (!crate::write::has_loss(&entries)).then_some(text)
}

/// The settings of the write of a source body: the link keys of the document
/// around it stand in for its definitions, which a copy per body would make
/// quadratic (carve-rs#4564).
fn nested_settings(keys: &crate::write::LinkKeys) -> crate::write::Settings {
    crate::write::Settings {
        link_keys: Some(std::sync::Arc::clone(keys)),
        ..crate::write::Settings::default()
    }
}

/// C2-X-026, C2-W-089: the source record of a claimed div, its body written
/// with the link reference definitions of the document around it.
pub(crate) fn div_source(node: &Value, name: &str, keys: &crate::write::LinkKeys) -> Option<Value> {
    let doc = json!({"type": "document", "astVersion": "2.0", "children": [node]});
    let text = write_claim_source(&doc, &nested_settings(keys))?;
    let title = at(node, "/title").as_array().map(|title| plain_text(title));
    div_record(&text, name, title, node.get("groupLabel").cloned())
}

/// [`div_source`] of a div in an arena, written from a typed copy of it; the
/// same as the write of its JSON value.
pub(crate) fn div_source_typed(
    node: &crate::ast::Node<'_>,
    name: &str,
    keys: &crate::write::LinkKeys,
) -> Option<Value> {
    let crate::ast::Node::Div(div) = node else {
        return None;
    };
    let children = vec![node.owned()];
    let arena = crate::Arena::new();
    let doc = crate::ast::owned::Document {
        ast_version: "2.0".into(),
        children,
        ..Default::default()
    }
    .in_arena(&arena);
    let doc = &*arena.alloc(doc);
    let pool = crate::write::Pool::new();
    let (text, entries) =
        crate::write::write_view(crate::view::V::Doc(doc), &pool, &nested_settings(keys));
    if crate::write::has_loss(&entries) {
        return None;
    }
    let title = match &div.title {
        Some(title) => Some(plain_text(serde_json::to_value(&**title).ok()?.as_array()?)),
        None => None,
    };
    div_record(&text, name, title, div.group_label.map(Value::from))
}

/// The source record of a claimed div from the written document.
fn div_record(
    text: &str,
    name: &str,
    title: Option<String>,
    group_label: Option<Value>,
) -> Option<Value> {
    let lines: Vec<&str> = text.split('\n').collect();
    let open = lines.iter().position(|l| l.starts_with(":::"))?;
    let opener = lines.get(open)?;
    let run = opener.get(..opener.bytes().take_while(|&b| b == b':').count())?;
    let close = lines.iter().rposition(|l| *l == run)?;
    let body = lines.get(open + 1..close)?.join("\n");
    let mut source = Map::new();
    source.insert("form".into(), "div".into());
    source.insert("name".into(), name.into());
    if let Some(title) = title {
        source.insert("title".into(), title.into());
    }
    if let Some(label) = group_label {
        source.insert("groupLabel".into(), label);
    }
    let body = if body.is_empty() { body } else { body + "\n" };
    source.insert("body".into(), body.into());
    Some(Value::Object(source))
}

/// The body of the source form `:name[body]` of an inline extension node.
pub(crate) fn inline_body(node: &Map<String, Value>, source: &str) -> Option<String> {
    let mut bare = node.clone();
    bare.remove("attrs");
    let doc = json!({"type": "document", "astVersion": "2.0", "children": [
        {"type": "paragraph", "children": [Value::Object(bare)]},
    ]});
    let text = write_claim_source(&doc, &crate::write::Settings::default())?;
    text.trim_end_matches('\n')
        .strip_prefix(&format!(":{source}["))?
        .strip_suffix(']')
        .map(str::to_string)
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

fn declare(doc: &mut Value) {
    declare_names(
        doc,
        &[
            ("carve.tab", json!(["title", "body"])),
            ("carve.tabs", json!(["tabs"])),
        ],
    );
}

/// C2-A-017: the root declares each qualified extension once, sorted by name.
pub(crate) fn declare_names(doc: &mut Value, names: &[(&str, Value)]) {
    let mut list: Vec<Value> = at(doc, "/extensions")
        .as_array()
        .cloned()
        .unwrap_or_default();
    for (name, slots) in names {
        if !list.iter().any(|d| at(d, "/name") == *name) {
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

/// Authored and generated heading IDs reserved in the render document (C2-X-048).
#[derive(Debug, Default)]
pub(crate) struct UsedIds {
    ids: HashSet<String>,
    count: usize,
}

impl UsedIds {
    /// The ids of a resolved render view in an arena.
    pub(crate) fn of_arena(view: &crate::ast::Document<'_>) -> Self {
        fn walk(node: &crate::ast::Node<'_>, used: &mut UsedIds, depth: usize) {
            if depth > crate::resolve::WALK_CAP {
                return;
            }
            if let Some(id) = node.attrs().and_then(|attrs| attrs.id) {
                used.ids.insert(id.to_string());
            }
            if let crate::ast::Node::Heading(heading) = node
                && let Some(id) = heading.generated_id
            {
                used.ids.insert(id.to_string());
            }
            node.for_each_child(&mut |child| walk(child, used, depth + 1));
            let slots = match node {
                crate::ast::Node::BlockExtension(ext) => ext.slots,
                crate::ast::Node::InlineExtension(ext) => ext.slots,
                _ => None,
            };
            if let Some(slots) = slots {
                used.scan_json(slots);
            }
        }
        let mut used = Self::default();
        if let Some(id) = view.attrs.and_then(|attrs| attrs.id) {
            used.ids.insert(id.to_string());
        }
        for node in &*view.children {
            walk(node, &mut used, 0);
        }
        used
    }

    fn scan_json(&mut self, root: &Value) {
        let mut pending = vec![root];
        while let Some(value) = pending.pop() {
            match value {
                Value::Object(map) => {
                    if let Some(id) = map
                        .get("attrs")
                        .and_then(|attrs| attrs.get("id"))
                        .and_then(Value::as_str)
                    {
                        self.ids.insert(id.to_string());
                    }
                    if map.get("type").is_some_and(|ty| ty == "heading") {
                        if let Some(id) = map.get("generatedId").and_then(Value::as_str) {
                            self.ids.insert(id.to_string());
                        }
                    }
                    pending.extend(map.values());
                }
                Value::Array(items) => pending.extend(items),
                _ => {}
            }
        }
    }

    pub(crate) fn next(&mut self, prefix: &str) -> String {
        loop {
            self.count += 1;
            let id = format!("{prefix}-{}", self.count);
            if self.ids.insert(id.clone()) {
                return id;
            }
        }
    }

    pub(crate) fn next_tabs(&mut self, count: usize, aria: bool) -> String {
        loop {
            self.count += 1;
            let group = format!("tabs-{}", self.count);
            let controls = (0..count).map(|i| format!("{group}-{i}"));
            let panels = (0..count).map(|i| format!("{group}-{i}-panel"));
            if controls.clone().all(|id| !self.ids.contains(&id))
                && (!aria || panels.clone().all(|id| !self.ids.contains(&id)))
            {
                self.ids.extend(controls);
                if aria {
                    self.ids.extend(panels);
                }
                return group;
            }
        }
    }
}

/// Whether the tabs wrapper writes the authored attribute `key`.
fn wrapper_keeps(node: &Value, key: &str) -> bool {
    !(crate::html::url::dropped_name(key)
        || crate::html::attrs::RESERVED
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(key))
        || key.eq_ignore_ascii_case("class")
        || (at(node, "/attrs/id").is_string() && key.eq_ignore_ascii_case("id")))
}

/// Builds the tabs wrapper with the standard authored-attribute rules.
fn tabs_wrapper(node: &Value, cx: &super::RenderContext<'_>, aria: bool, label: &str) -> String {
    let mut wrapper = String::from("<div");
    if let Some(id) = at(node, "/attrs/id").as_str() {
        let _ = write!(wrapper, " id=\"{}\"", cx.escape_attribute(id));
    }
    let mut classes = String::from("tabs");
    if let Some(authored) = at(node, "/attrs/classes").as_array() {
        for class in authored.iter().filter_map(Value::as_str) {
            classes.push(' ');
            classes.push_str(class);
        }
    }
    let _ = write!(wrapper, " class=\"{}\"", cx.escape_attribute(&classes));
    let mut has_role = false;
    let mut has_name = false;
    if let Some(pairs) = at(node, "/attrs/keyValues").as_array() {
        for pair in pairs {
            let (Some(key), Some(value)) = (
                pair.get(0).and_then(Value::as_str),
                pair.get(1).and_then(Value::as_str),
            ) else {
                continue;
            };
            if !wrapper_keeps(node, key) {
                continue;
            }
            let value =
                crate::html::url::filter_value_in(key, value, Some(&cx.options().url_policy));
            has_role |= key.eq_ignore_ascii_case("role");
            has_name |= key.eq_ignore_ascii_case("aria-label")
                || key.eq_ignore_ascii_case("aria-labelledby");
            let _ = write!(wrapper, " {key}=\"{}\"", cx.escape_attribute(value));
        }
    }
    if !has_role {
        wrapper.push_str(if aria && cx.mode() != crate::options::Mode::Static {
            " role=\"tablist\""
        } else {
            " role=\"group\""
        });
    }
    if !has_name {
        let _ = write!(wrapper, " aria-label=\"{}\"", cx.escape_attribute(label));
    }
    wrapper.push('>');
    wrapper
}

fn selected_tab(tabs: &[Value]) -> usize {
    tabs.iter()
        .position(|t| {
            at(t, "/attrs/keyValues")
                .as_array()
                .is_some_and(|kv| kv.iter().any(|p| p.get(0).is_some_and(|k| k == "selected")))
        })
        .unwrap_or(0)
}

fn tab_label_html(
    title: &[Value],
    name: &str,
    cx: &super::RenderContext<'_>,
) -> Result<String, Error> {
    if title.is_empty() {
        Ok(cx.escape(name))
    } else {
        cx.render_inlines(&Value::Array(title.to_vec()))
    }
}

/// The standard tabs renderer: radio controls in `interactive` mode
/// (C2-X-043 to C2-X-048), sections in `static` mode (C2-X-056, ruling D6).
pub(crate) fn render_tabs(
    node: &Value,
    cx: &super::RenderContext<'_>,
    group: &dyn Fn() -> String,
    aria: bool,
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
    let mut out = tabs_wrapper(node, cx, aria, label);
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
    if cx.mode() == crate::options::Mode::Static {
        for (tab, (title, name)) in tabs.iter().zip(&titles) {
            let _ = write!(
                out,
                r#"<section class="tabs-panel"><h3 class="tabs-label">{}</h3>{}</section>"#,
                tab_label_html(title, name, cx)?,
                cx.render_blocks(at(tab, "/slots/body"))?
            );
        }
        out.push_str("</div>");
        return Ok(out);
    }
    let selected = selected_tab(&tabs);
    let group = group();
    let mut panels = String::new();
    for (i, (tab, (title, name))) in tabs.iter().zip(&titles).enumerate() {
        let id = format!("{group}-{i}");
        if aria {
            let panel = format!("{id}-panel");
            let selected_here = i == selected;
            let _ = write!(
                out,
                "<button type=\"button\" role=\"tab\" id=\"{id}\" aria-selected=\"{selected_here}\" aria-controls=\"{panel}\"{}>{}</button>",
                if selected_here {
                    ""
                } else {
                    " tabindex=\"-1\""
                },
                tab_label_html(title, name, cx)?
            );
            let _ = write!(
                panels,
                "<div class=\"tab-panel\" role=\"tabpanel\" id=\"{panel}\" aria-labelledby=\"{id}\"{}>{}</div>",
                if selected_here { "" } else { " hidden" },
                cx.render_blocks(at(tab, "/slots/body"))?
            );
        } else {
            let _ = write!(
                out,
                r#"<input type="radio" name="{group}" id="{id}"{}><label for="{id}">{}</label>"#,
                if i == selected { " checked" } else { "" },
                tab_label_html(title, name, cx)?
            );
            let _ = write!(
                panels,
                r#"<div class="tab-panel" role="group" aria-label="{}">{}</div>"#,
                cx.escape_attribute(name),
                cx.render_blocks(at(tab, "/slots/body"))?
            );
        }
    }
    out.push_str(&panels);
    out.push_str("</div>");
    Ok(out)
}
