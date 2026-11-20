//! `details` and `spoiler` (carve-js#5070).

use super::{
    Bundle, Claim, Engine, Form, Matcher, authored, classes, declaration, key_values, with_attrs,
};
use crate::api::{Channel, Error, Extension, RenderContext, Renderer};
use serde_json::{Value, json};

/// C2-X-042 precedent: the title as a `strong` paragraph, then the body.
fn titled_fallback(title: &[Value], body: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    if !title.is_empty() {
        out.push(json!({"type": "paragraph", "children": [{"type": "strong", "children": title}]}));
    }
    out.extend(body.iter().cloned());
    out
}

fn disclosure(name: &str, claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Div {
        return None;
    }
    let title = claim
        .node
        .get("title")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let body = claim
        .node
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Some(json!({
        "type": "block_extension",
        "name": name,
        "version": "1",
        "children": [],
        "slots": {"summary": title, "body": body},
        "fallback": titled_fallback(&title, &body),
    }))
}

fn claim_details(claim: &Claim<'_>) -> Option<Value> {
    disclosure("carve.details", claim)
}

fn claim_spoiler_block(claim: &Claim<'_>) -> Option<Value> {
    disclosure("carve.spoilerBlock", claim)
}

fn claim_spoiler(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Inline {
        return None;
    }
    let children = claim
        .node
        .get("children")
        .cloned()
        .unwrap_or_else(|| json!([]));
    Some(json!({
        "type": "inline_extension",
        "name": "carve.spoiler",
        "version": "1",
        "children": [],
        "slots": {"content": children},
        "fallback": children,
    }))
}

/// `node` with `base` ahead of its classes, written as attributes.
fn attrs_with(node: &Value, base: &[&str], cx: &RenderContext<'_>) -> String {
    let attrs = node.get("attrs");
    let mut cls: Vec<Value> = base.iter().map(|c| Value::from(*c)).collect();
    cls.extend(classes(attrs));
    cx.attributes(&with_attrs(node, cls, key_values(attrs)))
}

fn has_raw_block(node: &Value) -> bool {
    let mut pending = vec![node];
    while let Some(value) = pending.pop() {
        if value.get("type").and_then(Value::as_str) == Some("raw_block") {
            return true;
        }
        match value {
            Value::Array(items) => pending.extend(items),
            Value::Object(fields) => pending.extend(fields.values()),
            _ => {}
        }
    }
    false
}

/// Indent ordinary markup while preserving the bytes of literal bodies.
fn container(
    head: &str,
    inner: &str,
    body: &str,
    tail: &str,
    raw_body: bool,
    cx: &RenderContext<'_>,
) -> String {
    let indent = "  ".repeat(cx.block_depth());
    let mut lines = vec![head.to_string(), format!("{indent}  {inner}")];
    if raw_body || body.contains("<textarea") || (body.contains("<pre") && !body.contains("<code")) {
        lines.push(body.to_string());
    } else if !body.is_empty() {
        let mut in_code = false;
        for line in body.split('\n') {
            // Core code blocks put their literal value between these tags.
            // Indenting a continuation line, including the closing-tag line,
            // would change the value of the code element.
            let starts_code = line.contains("<pre") && line.contains("<code");
            if in_code {
                lines.push(line.to_string());
            } else if line.is_empty() {
                lines.push(indent.clone());
            } else {
                lines.push(format!("{indent}  {line}"));
            }
            if starts_code {
                in_code = true;
            }
            if in_code && line.contains("</code>") {
                in_code = false;
            }
        }
    }
    lines.push(format!("{indent}{tail}"));
    lines.join("\n")
}

fn summary(node: &Value, label: &str, cx: &RenderContext<'_>) -> Result<String, Error> {
    let slot = node
        .pointer("/slots/summary")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let text = if slot.as_array().is_some_and(|s| !s.is_empty()) {
        cx.render_inlines(&slot)?.trim().to_string()
    } else {
        String::new()
    };
    Ok(if text.is_empty() {
        cx.escape(label)
    } else {
        text
    })
}

fn body(node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
    cx.render_blocks(
        &node
            .pointer("/slots/body")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
}

/// The disclosure of a titled div; `open` in static mode unless authored.
fn render_disclosure(
    node: &Value,
    cx: &RenderContext<'_>,
    label: &str,
    base: &[&str],
) -> Result<String, Error> {
    let open = cx.mode() == crate::options::Mode::Static && !authored(node.get("attrs"), "open");
    Ok(container(
        &format!(
            "<details{}{}>",
            if open { " open" } else { "" },
            attrs_with(node, base, cx)
        ),
        &format!("<summary>{}</summary>", summary(node, label, cx)?),
        &body(node, cx)?,
        "</details>",
        node.pointer("/slots/body").is_some_and(has_raw_block),
        cx,
    ))
}

struct Details;

impl Renderer for Details {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let label = cx.label("details").unwrap_or("Details").to_string();
        render_disclosure(node, cx, &label, &[])
    }
}

/// `::: details "Title"` as a native disclosure; static mode writes it expanded.
#[must_use]
pub fn details() -> Bundle {
    Bundle {
        extensions: vec![Extension {
            declaration: declaration("carve.details", &["summary", "body"]),
            renderers: vec![
                (Channel::Interactive, Box::new(Details)),
                (Channel::BuildTime, Box::new(Details)),
            ],
            transform: None,
        }],
        host: super::HostFields::default(),
        matchers: vec![Matcher {
            extension: "carve.details".into(),
            form: Form::Div,
            name: "details".into(),
            claimer: Box::new(Engine(claim_details)),
        }],
    }
}

struct SpoilerInline(bool);

impl Renderer for SpoilerInline {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        // Hiding means nothing offline, so static output is revealed.
        let base: &[&str] = if self.0 {
            &["spoiler", "spoiler-revealed"]
        } else {
            &["spoiler"]
        };
        let content = node
            .pointer("/slots/content")
            .cloned()
            .unwrap_or_else(|| json!([]));
        Ok(format!(
            "<span{}>{}</span>",
            attrs_with(node, base, cx),
            cx.render_inlines(&content)?
        ))
    }
}

struct SpoilerBlock(bool);

impl Renderer for SpoilerBlock {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let label = cx.label("spoiler").unwrap_or("Spoiler").to_string();
        if !self.0 {
            return render_disclosure(node, cx, &label, &["spoiler"]);
        }
        Ok(container(
            &format!(
                "<section{}>",
                attrs_with(node, &["spoiler", "spoiler-revealed"], cx)
            ),
            &format!(
                r#"<h3 class="spoiler-title">{}</h3>"#,
                summary(node, &label, cx)?
            ),
            &body(node, cx)?,
            "</section>",
            node.pointer("/slots/body").is_some_and(has_raw_block),
            cx,
        ))
    }
}

/// `:spoiler[text]` and `::: spoiler "Title"`: hidden by the host
/// stylesheet or a disclosure, revealed in static mode.
#[must_use]
pub fn spoiler() -> Bundle {
    Bundle {
        extensions: vec![
            Extension {
                declaration: declaration("carve.spoiler", &["content"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(SpoilerInline(false))),
                    (Channel::BuildTime, Box::new(SpoilerInline(true))),
                ],
                transform: None,
            },
            Extension {
                declaration: declaration("carve.spoilerBlock", &["summary", "body"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(SpoilerBlock(false))),
                    (Channel::BuildTime, Box::new(SpoilerBlock(true))),
                ],
                transform: None,
            },
        ],
        host: super::HostFields::default(),
        matchers: vec![
            Matcher {
                extension: "carve.spoiler".into(),
                form: Form::Inline,
                name: "spoiler".into(),
                claimer: Box::new(Engine(claim_spoiler)),
            },
            Matcher {
                extension: "carve.spoilerBlock".into(),
                form: Form::Div,
                name: "spoiler".into(),
                claimer: Box::new(Engine(claim_spoiler_block)),
            },
        ],
    }
}

pub(super) fn spoiler_inline() -> Bundle {
    spoiler_part("carve.spoiler")
}

pub(super) fn spoiler_block() -> Bundle {
    spoiler_part("carve.spoilerBlock")
}

fn spoiler_part(name: &str) -> Bundle {
    let mut bundle = spoiler();
    bundle.extensions.retain(|ext| ext.declaration.name == name);
    bundle.matchers.retain(|matcher| matcher.extension == name);
    bundle
}
