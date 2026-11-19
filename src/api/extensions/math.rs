//! `mathBlock` (carve-js#5068).

use super::{
    Bundle, Claim, Form, Matcher, bare_fallback, classes, declaration, fence_source, key_values,
    payload_text, with_attrs,
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
    format!(
        "<div{}>",
        cx.attributes(&with_attrs(node, cls, key_values(attrs)))
    )
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
        matchers: vec![Matcher {
            extension: NAME.into(),
            form: Form::Fence,
            name: options.language.unwrap_or_else(|| "math".into()),
            claimer: Box::new(claim),
        }],
    }
}
