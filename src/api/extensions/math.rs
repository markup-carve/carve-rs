//! `mathBlock` (carve-js#5068).

use super::{
    Bundle, Claim, Form, Matcher, Parts, bare_fallback, classes, declaration, fence_source,
    key_values, payload_text, typed, typed_payload, with_attrs,
};
use crate::api::{Channel, Error, Extension, RenderContext, Renderer};
use serde_json::{Value, json};
use std::sync::Arc;

/// Server-side math for static mode: TeX and whether it is display math in,
/// `MathML` or HTML out.
pub type MathBuilder = Arc<dyn Fn(&str, bool) -> String + Send + Sync>;

/// The options of [`math_block`].
#[derive(Clone, Default)]
pub struct MathBlockOptions {
    /// The fence language of a display-math block; default `math`.
    pub language: Option<String>,
    /// C2-X-052: without it static mode writes the fallback.
    pub build: Option<MathBuilder>,
}

const NAME: &str = "carve.mathBlock";

fn open(node: &Value, cx: &RenderContext<'_>) -> String {
    let attrs = node.get("attrs");
    let mut cls = vec![Value::from("math"), Value::from("display")];
    cls.extend(classes(attrs));
    let mut keys = key_values(attrs);
    keys.retain(|pair| {
        !pair
            .get(0)
            .and_then(Value::as_str)
            .is_some_and(|name| name.eq_ignore_ascii_case("role"))
    });
    keys.push(json!(["role", "math"]));
    format!("<div{}>", cx.attributes(&with_attrs(node, cls, keys)))
}

/// [`open`] of a typed node.
fn typed_open(node: &crate::ast::BlockExtension<'_>, cx: &RenderContext<'_>) -> String {
    let mut parts = Parts::of(node.attrs).with_classes(&["math", "display"]);
    parts
        .key_values
        .retain(|(name, _)| !name.eq_ignore_ascii_case("role"));
    parts.key_values.push(("role", "math"));
    format!("<div{}>", cx.write_parts(&parts, node.pos.as_ref()))
}

struct Interactive;

impl Renderer for Interactive {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        Ok(format!(
            "{}\\[{}\\]</div>",
            open(node, cx),
            cx.escape(fence_source(payload_text(node)))
        ))
    }

    fn render_typed(
        &self,
        node: &crate::ast::Node<'_>,
        cx: &RenderContext<'_>,
    ) -> Option<Result<String, Error>> {
        let node = typed(node)?;
        Some(Ok(format!(
            "{}\\[{}\\]</div>",
            typed_open(node, cx),
            cx.escape(fence_source(typed_payload(node)))
        )))
    }
}

struct BuildTime(MathBuilder);

impl Renderer for BuildTime {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        Ok(format!(
            "{}{}</div>",
            open(node, cx),
            (self.0)(fence_source(payload_text(node)), true)
        ))
    }

    fn render_typed(
        &self,
        node: &crate::ast::Node<'_>,
        cx: &RenderContext<'_>,
    ) -> Option<Result<String, Error>> {
        let node = typed(node)?;
        Some(Ok(format!(
            "{}{}</div>",
            typed_open(node, cx),
            (self.0)(fence_source(typed_payload(node)), true)
        )))
    }
}

fn claim(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Fence {
        return None;
    }
    let value = claim
        .node
        .get("value")
        .cloned()
        .unwrap_or_else(|| "".into());
    Some(json!({
        "type": "block_extension",
        "name": NAME,
        "version": "1",
        "children": [],
        "payload": [{"format": "application/x-tex", "value": value}],
        "fallback": [bare_fallback(claim.node)],
    }))
}

/// A `math` fence as `<div class="math display">\[...\]</div>`, the
/// delimiters `KaTeX` and `MathJax` read for display math.
#[must_use]
pub fn math_block(options: MathBlockOptions) -> Bundle {
    let mut renderers: Vec<(Channel, Box<dyn Renderer>)> =
        vec![(Channel::Interactive, Box::new(Interactive))];
    if let Some(build) = options.build {
        renderers.push((Channel::BuildTime, Box::new(BuildTime(build))));
    }
    Bundle {
        extensions: vec![Extension {
            declaration: declaration(NAME, &[]),
            renderers,
            transform: None,
        }],
        host: super::HostFields::default(),
        matchers: vec![Matcher {
            extension: NAME.into(),
            form: Form::Fence,
            name: options.language.unwrap_or_else(|| "math".into()),
            claimer: Box::new(super::Engine(claim)),
        }],
    }
}
