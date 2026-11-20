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

fn mark_raw_blocks(node: &mut Value, start: &str, end: &str) -> bool {
    let mut marked = false;
    let mut pending = vec![node];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(items.iter_mut()),
            Value::Object(fields) => {
                if fields.get("type").and_then(Value::as_str) == Some("raw_block") {
                    if fields.get("format").and_then(Value::as_str) == Some("html")
                        && let Some(Value::String(raw)) = fields.get_mut("value")
                    {
                        let (value, final_line) = if let Some(value) = raw.strip_suffix("\r\n") {
                            (value, "\r\n")
                        } else if let Some(value) = raw.strip_suffix('\n') {
                            (value, "\n")
                        } else {
                            (raw.as_str(), "")
                        };
                        *raw = format!("{start}{value}{end}{final_line}");
                        marked = true;
                    }
                } else if fields.contains_key("type") {
                    pending.extend(
                        fields
                            .iter_mut()
                            .filter(|(key, _)| matches!(key.as_str(), "children" | "slots"))
                            .map(|(_, value)| value),
                    );
                } else {
                    pending.extend(fields.values_mut());
                }
            }
            _ => {}
        }
    }
    marked
}

fn strip_raw_markers(
    rendered: &str,
    start: &str,
    end: &str,
) -> Option<(String, Vec<(usize, usize)>)> {
    let mut body = String::with_capacity(rendered.len());
    let mut spans = Vec::new();
    let mut rest = rendered;
    while let Some(at) = rest.find(start) {
        body.push_str(&rest[..at]);
        rest = &rest[at + start.len()..];
        let close = rest.find(end)?;
        let begin = body.len();
        body.push_str(&rest[..close]);
        spans.push((begin, body.len()));
        rest = &rest[close + end.len()..];
    }
    body.push_str(rest);
    Some((body, spans))
}

#[derive(Default)]
struct LiteralState {
    code: bool,
    pre: bool,
    textarea: bool,
}

impl LiteralState {
    fn advance(&mut self, line: &str) {
        let starts_code = line.contains("<pre") && line.contains("<code");
        if starts_code {
            self.code = true;
        }
        if line.contains("</code>") {
            self.code = false;
        }
        if line.contains("<pre") && !starts_code {
            self.pre = true;
        }
        if line.contains("</pre>") {
            self.pre = false;
        }
        if line.contains("<textarea") {
            self.textarea = true;
        }
        if line.contains("</textarea>") {
            self.textarea = false;
        }
    }

    fn contains_line(&self, offset: usize, raw: &[(usize, usize)]) -> bool {
        self.code
            || self.pre
            || self.textarea
            || raw
                .iter()
                .any(|&(start, end)| start <= offset && offset < end)
    }
}

fn indented_body_lines(body: &str, indent: &str, raw: &[(usize, usize)]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut state = LiteralState::default();
    let mut offset = 0;
    for line in body.split('\n') {
        if state.contains_line(offset, raw) {
            lines.push(line.to_string());
        } else if line.is_empty() {
            lines.push(indent.to_string());
        } else {
            lines.push(format!("{indent}  {line}"));
        }
        state.advance(line);
        offset += line.len() + 1;
    }
    lines
}

/// Indent ordinary markup while preserving the bytes of literal bodies.
fn container(
    head: &str,
    inner: &str,
    body: &str,
    tail: &str,
    raw: &[(usize, usize)],
    cx: &RenderContext<'_>,
) -> String {
    let indent = "  ".repeat(cx.block_depth());
    let mut lines = vec![head.to_string(), format!("{indent}  {inner}")];
    if !body.is_empty() {
        // Raw blocks stand at column zero; sibling markup keeps its depth.
        lines.extend(indented_body_lines(body, &indent, raw));
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

fn body(node: &Value, cx: &RenderContext<'_>) -> Result<(String, Vec<(usize, usize)>), Error> {
    let mut slot = node
        .pointer("/slots/body")
        .cloned()
        .unwrap_or_else(|| json!([]));
    if !cx.options().raw_html {
        return Ok((cx.render_blocks(&slot)?, Vec::new()));
    }
    let authored = slot.to_string();
    let mut nonce = 0;
    let (start, end) = loop {
        let start = format!("<!--carve-raw-{nonce}-start-->");
        let end = format!("<!--carve-raw-{nonce}-end-->");
        if !authored.contains(&start) && !authored.contains(&end) {
            break (start, end);
        }
        nonce += 1;
    };
    if !mark_raw_blocks(&mut slot, &start, &end) {
        return Ok((cx.render_blocks(&slot)?, Vec::new()));
    }
    let rendered = cx.render_blocks(&slot)?;
    if let Some((body, spans)) = strip_raw_markers(&rendered, &start, &end) {
        return Ok((body, spans));
    }
    Ok((
        cx.render_blocks(node.pointer("/slots/body").unwrap_or(&json!([])))?,
        Vec::new(),
    ))
}

/// The disclosure of a titled div; `open` in static mode unless authored.
fn render_disclosure(
    node: &Value,
    cx: &RenderContext<'_>,
    label: &str,
    base: &[&str],
) -> Result<String, Error> {
    let open = cx.mode() == crate::options::Mode::Static && !authored(node.get("attrs"), "open");
    let (body, raw) = body(node, cx)?;
    Ok(container(
        &format!(
            "<details{}{}>",
            if open { " open" } else { "" },
            attrs_with(node, base, cx)
        ),
        &format!("<summary>{}</summary>", summary(node, label, cx)?),
        &body,
        "</details>",
        &raw,
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
        let (body, raw) = body(node, cx)?;
        Ok(container(
            &format!(
                "<section{}>",
                attrs_with(node, &["spoiler", "spoiler-revealed"], cx)
            ),
            &format!(
                r#"<h3 class="spoiler-title">{}</h3>"#,
                summary(node, &label, cx)?
            ),
            &body,
            "</section>",
            &raw,
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
