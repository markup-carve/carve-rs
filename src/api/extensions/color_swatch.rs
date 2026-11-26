//! `colorSwatch` (carve-js#5070): `:color[value]` with a swatch renderer.

use super::{Bundle, Claim, Engine, Form, Matcher, classes, declaration, key_values, with_attrs};
use crate::api::{Channel, Error, Extension, RenderContext, Renderer};
use serde_json::{Value, json};

/// Where the chip stands beside the value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SwatchPosition {
    #[default]
    Before,
    After,
    /// The chip alone, the value as its `title`.
    None,
}

/// The shape of the chip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SwatchShape {
    #[default]
    Square,
    Round,
    Ring,
}

/// The options of [`color_swatch`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorSwatchOptions {
    pub position: SwatchPosition,
    pub shape: SwatchShape,
    /// A light background of the color behind the swatch.
    pub tint: bool,
    /// The value in its own element, revealed on focus.
    pub reveal: bool,
}

const NAMED_COLORS: &[&str] = &[
    "transparent",
    "currentcolor",
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkvioletred",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

/// JavaScript's `\s`, which `trim` also strips.
fn js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn text_of(nodes: &[Value], out: &mut String) {
    for node in nodes {
        match node.get("type").and_then(Value::as_str) {
            Some("text" | "code" | "literal_inline") => {
                out.push_str(node.get("value").and_then(Value::as_str).unwrap_or(""));
            }
            Some("tag") => {
                out.push('#');
                out.push_str(node.get("name").and_then(Value::as_str).unwrap_or(""));
            }
            _ => {
                if let Some(children) = node.get("children").and_then(Value::as_array) {
                    text_of(children, out);
                }
            }
        }
    }
}

fn hex_digits(value: &str) -> Option<&str> {
    let digits = value.strip_prefix('#')?;
    (matches!(digits.len(), 3 | 4 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(digits)
}

fn color_function(value: &str) -> bool {
    let Some(open) = value.find('(') else {
        return false;
    };
    if !matches!(&value[..open], "rgb" | "rgba" | "hsl" | "hsla") {
        return false;
    }
    let Some(inner) = value[open + 1..].strip_suffix(')') else {
        return false;
    };
    inner
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '%' | '/') || js_space(c))
        && inner.chars().any(|c| c.is_ascii_digit())
}

fn safe_color(input: &str) -> Option<String> {
    let value = input.trim_matches(js_space);
    if value.encode_utf16().count() > 256 {
        return None;
    }
    let named = !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_alphabetic())
        && NAMED_COLORS.contains(&value.to_ascii_lowercase().as_str());
    (hex_digits(value).is_some() || color_function(value) || named).then(|| value.to_string())
}

/// Splits on runs of whitespace, commas and slashes, as `split(/[\s,/]+/)`.
fn channel_parts(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_run = false;
    for (i, c) in text.char_indices() {
        let sep = js_space(c) || c == ',' || c == '/';
        if sep && !in_run {
            parts.push(&text[start..i]);
        }
        if !sep && in_run {
            start = i;
        }
        in_run = sep;
    }
    parts.push(if in_run { "" } else { &text[start..] });
    parts
}

fn channels(value: &str) -> Option<[f64; 3]> {
    if let Some(hex) = hex_digits(value) {
        let pair = |s: &str| f64::from(u8::from_str_radix(s, 16).unwrap_or(0));
        let digit = |i: usize, n: usize| hex.get(i..i + n).unwrap_or("0");
        return Some(if hex.len() < 5 {
            [0, 1, 2].map(|i| pair(&digit(i, 1).repeat(2)))
        } else {
            [0, 2, 4].map(|i| pair(digit(i, 2)))
        });
    }
    let inner = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts = channel_parts(inner.trim_matches(js_space));
    let [r, g, b, ..] = parts.as_slice() else {
        return None;
    };
    let channel = |p: &str| {
        (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse::<f64>().unwrap_or(f64::INFINITY))
    };
    Some([channel(r)?, channel(g)?, channel(b)?])
}

fn contrast_text(value: &str) -> Option<&'static str> {
    if let Some(hex) = hex_digits(value) {
        let alpha = match hex.len() {
            4 => &hex[3..],
            8 => &hex[6..],
            _ => "",
        };
        if !alpha.is_empty() && alpha.bytes().all(|b| b == b'0') {
            return None;
        }
    }
    let [r, g, b] = channels(value)?;
    let brightness = (r * 299.0 + g * 587.0 + b * 114.0) / 1000.0;
    Some(if brightness >= 128.0 { "#000" } else { "#fff" })
}

fn claim(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Inline {
        return None;
    }
    let children = claim
        .node
        .get("children")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let mut text = String::new();
    text_of(
        children.as_array().map_or(&[][..], Vec::as_slice),
        &mut text,
    );
    let value = safe_color(&text)?;
    Some(json!({
        "type": "inline_extension",
        "name": "carve.colorSwatch",
        "version": "1",
        "children": [],
        "props": {"value": value},
        "slots": {"value": children},
        "fallback": children,
    }))
}

fn has_key(kv: &[Value], name: &str) -> bool {
    kv.iter().any(|p| {
        p.get(0)
            .and_then(Value::as_str)
            .is_some_and(|k| k.to_lowercase() == name)
    })
}

fn with_default(kv: &mut Vec<Value>, name: &str, value: &str) {
    if !has_key(kv, name) {
        kv.push(json!([name, value]));
    }
}

/// The authored pairs without `contrast`, which the renderer consumes.
fn authored_pairs(node: &Value) -> Vec<Value> {
    key_values(node.get("attrs"))
        .into_iter()
        .filter(|p| {
            p.get(0)
                .and_then(Value::as_str)
                .is_none_or(|k| k.to_lowercase() != "contrast")
        })
        .collect()
}

struct Swatch(ColorSwatchOptions);

impl Swatch {
    fn swatch(&self, node: &Value, value: &str, cx: &RenderContext<'_>) -> String {
        let ColorSwatchOptions {
            position,
            shape,
            tint,
            reveal,
        } = self.0;
        let reveal = reveal && position != SwatchPosition::None;
        let (chip_class, style) = match shape {
            SwatchShape::Square => ("swatch-chip", format!("background-color:{value}")),
            SwatchShape::Round => (
                "swatch-chip swatch-chip-round",
                format!("background-color:{value}"),
            ),
            SwatchShape::Ring => (
                "swatch-chip swatch-chip-ring",
                format!("border-color:{value}"),
            ),
        };
        let chip = format!(
            r#"<span class="{chip_class}" style="{}"></span>"#,
            cx.escape_attribute(&style)
        );
        let escaped = cx.escape(value);
        let mut cls = vec![Value::from("swatch")];
        cls.extend(classes(node.get("attrs")));
        if tint {
            cls.push("swatch-tint".into());
        }
        if position == SwatchPosition::None {
            cls.push("swatch-chip-only".into());
        }
        if reveal {
            cls.push("swatch-reveal".into());
        }
        let mut kv = authored_pairs(node);
        if tint {
            with_default(
                &mut kv,
                "style",
                &format!("background-color:color-mix(in srgb, {value} 12%, transparent)"),
            );
        }
        if position == SwatchPosition::None {
            with_default(&mut kv, "title", value);
        }
        if reveal {
            with_default(&mut kv, "tabindex", "0");
        }
        let outer = cx.attributes(&with_attrs(node, cls, kv));
        let visible = if reveal {
            format!(r#"<span class="swatch-val">{escaped}</span>"#)
        } else {
            escaped
        };
        let inner = match position {
            SwatchPosition::None => chip,
            SwatchPosition::After => format!("{visible} {chip}"),
            SwatchPosition::Before => format!("{chip} {visible}"),
        };
        format!("<span{outer}>{inner}</span>")
    }
}

impl Renderer for Swatch {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let value = node
            .pointer("/props/value")
            .and_then(Value::as_str)
            .unwrap_or("");
        let contrast = has_key(&key_values(node.get("attrs")), "contrast");
        if let Some(color) = contrast.then(|| contrast_text(value)).flatten() {
            let mut cls = vec![Value::from("swatch-label")];
            cls.extend(classes(node.get("attrs")));
            let mut kv = authored_pairs(node);
            with_default(
                &mut kv,
                "style",
                &format!("background:{value};color:{color}"),
            );
            return Ok(format!(
                "<span{}>{}</span>",
                cx.attributes(&with_attrs(node, cls, kv)),
                cx.escape(value)
            ));
        }
        Ok(self.swatch(node, value, cx))
    }
}

/// `:color[value]` with its swatch renderer, in both channels. A value that
/// is not a hex color, a color function or a named color stays core.
#[must_use]
pub fn color_swatch(options: ColorSwatchOptions) -> Bundle {
    let swatch = || Box::new(Swatch(options)) as Box<dyn Renderer>;
    Bundle {
        extensions: vec![Extension {
            declaration: declaration("carve.colorSwatch", &["value"]),
            renderers: vec![
                (Channel::Interactive, swatch()),
                (Channel::BuildTime, swatch()),
            ],
            transform: None,
        }],
        host: super::HostFields::default(),
        matchers: vec![Matcher {
            extension: "carve.colorSwatch".into(),
            form: Form::Inline,
            name: "color".into(),
            claimer: Box::new(Engine(claim)),
        }],
    }
}
