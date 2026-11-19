//! PROVISIONAL: the standard `imgFence` matchers and their trusted SVG
//! sanitizer (C2-X-059, C2-X-060).

use crate::import::dom::{Dom, Id, Kind};
use serde_json::{Map, Value, json};
use std::fmt::Write as _;

/// The fence languages and the extensions that claim them.
pub(crate) const MATCHERS: &[(&str, &str)] = &[
    ("image", "carve.imgFence.image"),
    ("img", "carve.imgFence.img"),
];

/// C2-X-030: the bound of the encoded image URL.
const MAX_URL: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "title",
    "desc",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "path",
    "text",
    "tspan",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
];

const ATTRIBUTES: &[&str] = &[
    "id",
    "class",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "width",
    "height",
    "d",
    "points",
    "viewBox",
    "preserveAspectRatio",
    "transform",
    "version",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-miterlimit",
    "opacity",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-anchor",
    "dominant-baseline",
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientUnits",
    "gradientTransform",
    "clip-path",
    "clip-rule",
];

/// C2-X-010, C2-X-059: claims each `code_block` of an enabled language whose
/// body is SVG source. Returns the qualified names it claimed.
pub(crate) fn claim(
    doc: &mut Value,
    enabled: &[(&'static str, &'static str)],
) -> Vec<&'static str> {
    let mut claimed = Vec::new();
    let mut pending: Vec<&mut Value> = vec![doc];
    while let Some(v) = pending.pop() {
        match v {
            Value::Array(items) => pending.extend(items.iter_mut()),
            Value::Object(map) => {
                if let Some(&(_, qualified)) = matcher(map, enabled) {
                    *map = node(map, qualified);
                    if !claimed.contains(&qualified) {
                        claimed.push(qualified);
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
    claimed
}

fn matcher<'m>(
    map: &Map<String, Value>,
    enabled: &'m [(&'static str, &'static str)],
) -> Option<&'m (&'static str, &'static str)> {
    if map.get("type").and_then(Value::as_str) != Some("code_block") {
        return None;
    }
    let syntax = map.get("syntax").and_then(Value::as_str)?;
    let body = map.get("value").and_then(Value::as_str)?;
    if !body.trim_start().starts_with("<svg") {
        return None;
    }
    enabled.iter().find(|(name, _)| *name == syntax)
}

fn node(code: &Map<String, Value>, qualified: &str) -> Map<String, Value> {
    let syntax = code.get("syntax").cloned().unwrap_or(Value::Null);
    let body = code.get("value").cloned().unwrap_or_default();
    let mut fallback = code.clone();
    fallback.remove("pos");
    let mut out = Map::new();
    out.insert("type".into(), "block_extension".into());
    out.insert("name".into(), qualified.into());
    out.insert("version".into(), "1".into());
    out.insert("children".into(), json!([]));
    out.insert("props".into(), json!({ "language": syntax }));
    out.insert("fallback".into(), json!([Value::Object(fallback)]));
    out.insert(
        "source".into(),
        json!({"form": "fence", "name": syntax, "body": body}),
    );
    for key in ["pos", "attrs"] {
        if let Some(v) = code.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    out
}

/// The `<img>` of a claimed node, or `None` when the sanitizer refuses its
/// body and the fallback is written (C2-X-050, C2-X-059).
pub(crate) fn render(node: &Value) -> Option<String> {
    let body = node.pointer("/source/body").and_then(Value::as_str)?;
    let url = image_url(body)?;
    Some(format!("<img src=\"{url}\" alt=\"\">"))
}

/// C2-X-059, C2-X-060: the sanitized SVG as a `data:image/svg+xml` URL.
pub(crate) fn image_url(body: &str) -> Option<String> {
    let dom = crate::import::parse::fragment(body);
    let mut root = None;
    for &id in dom.children(dom.root) {
        match &dom.node(id).kind {
            Kind::Text(t) if t.trim().is_empty() => {}
            Kind::Comment(_) => {}
            Kind::Element {
                name, html: false, ..
            } if name == "svg" && root.is_none() => {
                root = Some(id);
            }
            _ => return None,
        }
    }
    let mut svg = String::new();
    write_element(&dom, root?, true, 0, &mut svg)?;
    let url = format!("data:image/svg+xml,{}", encode(&svg));
    (url.len() <= MAX_URL).then_some(url)
}

fn write_element(dom: &Dom, id: Id, root: bool, depth: usize, out: &mut String) -> Option<()> {
    if depth > MAX_DEPTH {
        return None;
    }
    let Kind::Element { name, html, attrs } = &dom.node(id).kind else {
        return Some(());
    };
    if *html || !ELEMENTS.contains(&name.as_str()) {
        return Some(());
    }
    let _ = write!(out, "<{name}");
    if root {
        out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
    }
    for (key, value) in attrs {
        if ATTRIBUTES.contains(&key.as_str()) && safe_value(value) {
            let _ = write!(out, " {key}=\"{}\"", escape(value));
        }
    }
    let children = dom.children(id);
    let mut inner = String::new();
    for &child in children {
        match &dom.node(child).kind {
            Kind::Text(t) => inner.push_str(&escape(t)),
            Kind::Element { .. } => write_element(dom, child, false, depth + 1, &mut inner)?,
            _ => {}
        }
    }
    if inner.is_empty() {
        out.push_str("/>");
    } else {
        let _ = write!(out, ">{inner}</{name}>");
    }
    Some(())
}

/// No external or active reference: `url(...)` only to a local `#id`.
fn safe_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("url(") {
        let after = rest.get(at + 4..).unwrap_or("");
        let target: String = after.chars().take_while(|&c| c != ')').collect();
        let local = target.strip_prefix('#').is_some_and(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        });
        if !local {
            return false;
        }
        rest = after;
    }
    !lower.contains("javascript:") && !lower.contains("data:")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Percent-encoding as `encodeURIComponent` writes it.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}
