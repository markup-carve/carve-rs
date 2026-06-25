//! Inline color swatches (Tier-3).
//!
//! Claims the reserved `color` inline role:
//!
//! - **Inline** `:color[value]` renders a swatch chip plus the value when the
//!   flattened inline content is a safe color token.
//! - Invalid / unrecognized values defer to the generic inline-extension
//!   fallback, so core emits `<span class="ext-color">...</span>`.

use crate::ast::{Attrs, InlineExtension, InlineNode};
use crate::escape::{escape_attr, escape_text};
use crate::extension::{CarveExtension, RenderContext};
use crate::render::render_attrs_after_class;

/// The inline extension role this extension claims.
const ROLE: &str = "color";

/// Render `:color[value]` inline roles as a color chip plus the value.
///
/// ```
/// use carve::{ColorSwatch, Options};
/// let ext = ColorSwatch::new();
/// let opts = Options::new().with_extension(&ext);
/// assert_eq!(
///     carve::to_html_with_options(":color[#ff8800]", &opts),
///     "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:#ff8800\"></span> #ff8800</span></p>"
/// );
/// ```
#[derive(Debug, Default, Clone)]
pub struct ColorSwatch;

impl ColorSwatch {
    /// Create a color swatch extension.
    pub fn new() -> Self {
        Self
    }
}

impl CarveExtension for ColorSwatch {
    fn name(&self) -> &'static str {
        "color"
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        _ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != ROLE {
            return None;
        }
        let value = inline_text(&node.children);
        let color = safe_color(&value)?;
        Some(format!(
            "<span{}><span class=\"swatch-chip\" style=\"background-color:{}\"></span> {}</span>",
            open_attrs_with_base(node.attrs.as_ref(), "swatch"),
            escape_attr(color),
            escape_text(color),
        ))
    }
}

/// Build the output element's attribute string with the base class before any
/// author classes, matching the spoiler extension's class merge behavior.
fn open_attrs_with_base(attrs: Option<&Attrs>, base: &str) -> String {
    match attrs {
        Some(a) => {
            let mut classes: Vec<String> = base.split(' ').map(str::to_string).collect();
            for class in &a.classes {
                if !classes.contains(class) {
                    classes.push(class.clone());
                }
            }
            format!(
                " class=\"{}\"{}",
                escape_attr(&classes.join(" ")),
                render_attrs_after_class(a),
            )
        }
        None => format!(" class=\"{}\"", escape_attr(base)),
    }
}

/// Return the trimmed value when it matches the safe color grammar.
fn safe_color(value: &str) -> Option<&str> {
    let value = value.trim();
    if is_hex_color(value) || is_color_function(value) || is_named_color(value) {
        Some(value)
    } else {
        None
    }
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_color_function(value: &str) -> bool {
    let Some(open) = value.find('(') else {
        return false;
    };
    if !value.ends_with(')') {
        return false;
    }
    let name = &value[..open];
    if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") {
        return false;
    }
    let inner = &value[open + 1..value.len() - 1];
    // At least one digit (rejects `rgb(/)` / empty args), only safe chars.
    inner.bytes().any(|b| b.is_ascii_digit())
        && inner.bytes().all(|b| {
            b.is_ascii_digit()
                || matches!(b, b'.' | b',' | b'%' | b'/' | b' ' | b'\t' | b'\n' | b'\r')
        })
}

fn is_named_color(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphabetic())
}

/// Flatten an inline tree to text so parsed tags such as `#ff8800` recover the
/// author-facing color value instead of rendered HTML.
fn inline_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(s),
            InlineNode::Code(s, _) => out.push_str(s),
            InlineNode::Emphasis(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::Link(l) => out.push_str(&inline_text(&l.children)),
            InlineNode::Span(s) => out.push_str(&inline_text(&s.children)),
            InlineNode::Extension(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::CriticInsert(c) => out.push_str(&inline_text(&c.children)),
            InlineNode::CriticDelete(c) => out.push_str(&inline_text(&c.children)),
            InlineNode::Mention(m) => {
                out.push('@');
                out.push_str(&m.user);
            }
            InlineNode::Tag(t) => {
                out.push('#');
                out.push_str(&t.name);
            }
            InlineNode::Emoji(e) => {
                out.push(':');
                out.push_str(&e.name);
                out.push(':');
            }
            InlineNode::SoftBreak | InlineNode::HardBreak => out.push('\n'),
            _ => {}
        }
    }
    out
}
