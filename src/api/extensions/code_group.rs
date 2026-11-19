//! `codeGroup` (carve-js#5068).

use super::{Bundle, Claim, Form, Matcher, authored, classes, declaration, key_values, with_attrs};
use crate::api::{Channel, Error, Extension, RenderContext, Renderer};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::sync::Arc;

/// How the interactive channel switches panels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CodeGroupMode {
    /// Radio inputs, no script.
    #[default]
    Css,
    /// Tab roles; a script switches the panels.
    Aria,
}

/// Replaces the core code block HTML of a panel: code and language in.
pub type Highlighter = Arc<dyn Fn(&str, Option<&str>) -> String + Send + Sync>;

/// The options of [`code_group`].
#[derive(Clone, Default)]
pub struct CodeGroupOptions {
    pub mode: CodeGroupMode,
    pub wrapper_class: Option<String>,
    pub panel_class: Option<String>,
    pub label_class: Option<String>,
    pub radio_class: Option<String>,
    /// The prefix of the generated ids; default `codegroup`.
    pub id_prefix: Option<String>,
    /// The name of the group; default the `codeGroup` label (C2-H-125).
    pub group_label: Option<String>,
    pub highlighter: Option<Highlighter>,
}

const NAME: &str = "carve.codeGroup";

struct Look {
    mode: CodeGroupMode,
    wrapper: String,
    panel: String,
    label: String,
    radio: String,
    prefix: String,
    group_label: Option<String>,
    highlighter: Option<Highlighter>,
}

/// `node` is what the panel renders: the code block, or a node a document
/// transform put in its place.
struct Panel {
    block: Value,
    node: Value,
    label: String,
    selected: bool,
}

fn has_selected(attrs: Option<&Value>) -> bool {
    key_values(attrs)
        .iter()
        .any(|kv| kv.get(0).and_then(Value::as_str) == Some("selected"))
}

/// The code block of a panel; a transform such as codeCallouts may have
/// replaced it by a node with it as its one fallback block.
fn code_of(node: &Value) -> Option<Value> {
    match node.get("type").and_then(Value::as_str)? {
        "code_block" => Some(node.clone()),
        "block_extension" => {
            let fallback = node.get("fallback")?.as_array()?;
            let [code] = fallback.as_slice() else {
                return None;
            };
            if code.get("type").and_then(Value::as_str) != Some("code_block") {
                return None;
            }
            let mut code = code.clone();
            if let (Some(attrs), Some(map)) = (node.get("attrs"), code.as_object_mut()) {
                map.insert("attrs".into(), attrs.clone());
            }
            Some(code)
        }
        _ => None,
    }
}

fn panels(node: &Value, cx: &RenderContext<'_>) -> Vec<Panel> {
    let mut out: Vec<Panel> = Vec::new();
    for child in node
        .pointer("/slots/panels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(block) = code_of(child) else {
            continue;
        };
        let title = block
            .get("title")
            .and_then(Value::as_array)
            .map(|t| cx.plain_text(t).trim().to_string())
            .unwrap_or_default();
        let group = block
            .get("groupLabel")
            .and_then(Value::as_str)
            .map_or("", str::trim);
        let syntax = block.get("syntax").and_then(Value::as_str).unwrap_or("");
        let label = [group, title.as_str(), syntax]
            .into_iter()
            .find(|s| !s.is_empty())
            .map_or_else(|| format!("Code {}", out.len() + 1), str::to_string);
        out.push(Panel {
            block,
            node: child.clone(),
            label,
            selected: false,
        });
    }
    let marked = out
        .iter()
        .position(|p| has_selected(p.block.get("attrs")))
        .unwrap_or(0);
    if let Some(p) = out.get_mut(marked) {
        p.selected = true;
    }
    out
}

fn without_selected(node: &Value) -> Value {
    let attrs = node.get("attrs");
    if !has_selected(attrs) {
        return node.clone();
    }
    let rest: Vec<Value> = key_values(attrs)
        .into_iter()
        .filter(|kv| kv.get(0).and_then(Value::as_str) != Some("selected"))
        .collect();
    let mut out = node.clone();
    let Some(map) = out.as_object_mut() else {
        return out;
    };
    if let Some(a) = map.get_mut("attrs").and_then(Value::as_object_mut) {
        if rest.is_empty() {
            a.remove("keyValues");
        } else {
            a.insert("keyValues".into(), Value::Array(rest));
        }
        if a.is_empty() {
            map.remove("attrs");
        }
    }
    out
}

impl Look {
    fn wrapper(&self, node: &Value, cx: &RenderContext<'_>, role: &str) -> String {
        let attrs = node.get("attrs");
        let name = self
            .group_label
            .clone()
            .or_else(|| cx.label("codeGroup").map(str::to_string))
            .unwrap_or_else(|| "Code examples".to_string());
        let mut kv = key_values(attrs);
        if !authored(attrs, "role") {
            kv.push(json!(["role", role]));
        }
        if !name.is_empty() && !authored(attrs, "aria-label") && !authored(attrs, "aria-labelledby")
        {
            kv.push(json!(["aria-label", name]));
        }
        let mut cls = vec![Value::from(self.wrapper.as_str())];
        cls.extend(
            classes(attrs)
                .into_iter()
                .filter(|c| c.as_str() != Some(&self.wrapper)),
        );
        format!("<div{}>", cx.attributes(&with_attrs(node, cls, kv)))
    }

    fn code(&self, panel: &Panel, cx: &RenderContext<'_>) -> Result<String, Error> {
        if let Some(h) = &self.highlighter {
            let value = panel
                .block
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("");
            return Ok(h(
                value.trim_end_matches('\n'),
                panel.block.get("syntax").and_then(Value::as_str),
            ));
        }
        cx.render_blocks(&Value::Array(vec![without_selected(&panel.node)]))
    }

    fn css(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let panels = panels(node, cx);
        let group = cx.next_id(&self.prefix);
        let a = |v: &str| cx.escape_attribute(v);
        let (mut controls, mut bodies) = (String::new(), String::new());
        for (i, p) in panels.iter().enumerate() {
            let id = format!("{group}-tab-{}", i + 1);
            let _ = write!(
                controls,
                r#"<input type="radio" name="{}" id="{}" class="{}"{}><label for="{}" class="{}">{}</label>"#,
                a(&group),
                a(&id),
                a(&self.radio),
                if p.selected { " checked" } else { "" },
                a(&id),
                a(&self.label),
                cx.escape(&p.label)
            );
            let _ = write!(
                bodies,
                r#"<div class="{}" role="group" aria-label="{}">{}</div>"#,
                a(&self.panel),
                a(&p.label),
                self.code(p, cx)?
            );
        }
        Ok(format!(
            "{}{controls}{bodies}</div>",
            self.wrapper(node, cx, "group")
        ))
    }

    fn aria(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let panels = panels(node, cx);
        let group = cx.next_id(&self.prefix);
        let a = |v: &str| cx.escape_attribute(v);
        let (mut controls, mut bodies) = (String::new(), String::new());
        for (i, p) in panels.iter().enumerate() {
            let tab = format!("{group}-tab-{}", i + 1);
            let body = format!("{group}-panel-{}", i + 1);
            // `type="button"`: a bare button inside a form would submit it.
            let _ = write!(
                controls,
                r#"<button type="button" role="tab" id="{}" aria-selected="{}" aria-controls="{}" class="{}"{}>{}</button>"#,
                a(&tab),
                p.selected,
                a(&body),
                a(&self.label),
                if p.selected { "" } else { r#" tabindex="-1""# },
                cx.escape(&p.label)
            );
            let _ = write!(
                bodies,
                r#"<div role="tabpanel" id="{}" aria-labelledby="{}" class="{}"{}>{}</div>"#,
                a(&body),
                a(&tab),
                a(&self.panel),
                if p.selected { "" } else { " hidden" },
                self.code(p, cx)?
            );
        }
        Ok(format!(
            "{}{controls}{bodies}</div>",
            self.wrapper(node, cx, "tablist")
        ))
    }

    /// C2-X-052: one section per panel headed by its label, no controls and
    /// no generated ids.
    fn build_time(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let a = |v: &str| cx.escape_attribute(v);
        let mut sections = String::new();
        for p in panels(node, cx) {
            let _ = write!(
                sections,
                r#"<section class="{}"><h3 class="{}">{}</h3>{}</section>"#,
                a(&self.panel),
                a(&self.label),
                cx.escape(&p.label),
                self.code(&p, cx)?
            );
        }
        Ok(format!(
            "{}{sections}</div>",
            self.wrapper(node, cx, "group")
        ))
    }
}

struct Interactive(Arc<Look>);

impl Renderer for Interactive {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        match self.0.mode {
            CodeGroupMode::Css => self.0.css(node, cx),
            CodeGroupMode::Aria => self.0.aria(node, cx),
        }
    }
}

struct BuildTime(Arc<Look>);

impl Renderer for BuildTime {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        self.0.build_time(node, cx)
    }
}

fn claim(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Div {
        return None;
    }
    let body = claim.node.get("children")?.as_array()?;
    let panels: Vec<Value> = body
        .iter()
        .filter(|n| n.get("type").and_then(Value::as_str) != Some("comment"))
        .cloned()
        .collect();
    // Like tabs (C2-X-040): other blocks keep the div, so no authored content is dropped.
    if panels.is_empty()
        || !panels
            .iter()
            .all(|n| n.get("type").and_then(Value::as_str) == Some("code_block"))
    {
        return None;
    }
    Some(json!({
        "type": "block_extension",
        "name": NAME,
        "version": "1",
        "children": [],
        "slots": {"panels": panels},
        "fallback": body,
    }))
}

/// `::: code-group` of code blocks as one switcher with a panel per block.
#[must_use]
pub fn code_group(options: CodeGroupOptions) -> Bundle {
    let look = Arc::new(Look {
        mode: options.mode,
        wrapper: options.wrapper_class.unwrap_or_else(|| "code-group".into()),
        panel: options
            .panel_class
            .unwrap_or_else(|| "code-group-panel".into()),
        label: options
            .label_class
            .unwrap_or_else(|| "code-group-label".into()),
        radio: options
            .radio_class
            .unwrap_or_else(|| "code-group-radio".into()),
        prefix: options.id_prefix.unwrap_or_else(|| "codegroup".into()),
        group_label: options.group_label,
        highlighter: options.highlighter,
    });
    Bundle {
        extensions: vec![Extension {
            declaration: declaration(NAME, &["panels"]),
            renderers: vec![
                (Channel::Interactive, Box::new(Interactive(look.clone()))),
                (Channel::BuildTime, Box::new(BuildTime(look))),
            ],
            transform: None,
        }],
        matchers: vec![Matcher {
            extension: NAME.into(),
            form: Form::Div,
            name: "code-group".into(),
            claimer: Box::new(claim),
        }],
    }
}
