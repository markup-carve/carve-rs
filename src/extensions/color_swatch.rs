//! Inline color swatches (Tier-3).
//!
//! Claims the reserved `color` inline role:
//!
//! - **Inline** `:color[value]` renders a swatch chip plus the value when the
//!   flattened inline content is a safe color token (hex, `rgb()`/`hsl()`, or an
//!   actual CSS named color).
//! - Invalid / unrecognized values defer to the generic inline-extension
//!   fallback, so core emits `<span class="ext-color">...</span>`.
//!
//! The render is configurable via [`ColorSwatch::position`],
//! [`ColorSwatch::shape`], [`ColorSwatch::tint`] and [`ColorSwatch::reveal`].

use crate::ast::{AttrSlot, Attrs, InlineExtension, InlineNode};
use crate::escape::{escape_attr, escape_text};
use crate::extension::{CarveExtension, RenderContext};
use crate::render::render_attrs_after_class;

/// The inline extension role this extension claims.
const ROLE: &str = "color";

/// Where the chip sits relative to the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwatchPosition {
    /// Chip before the value (default).
    #[default]
    Before,
    /// Chip after the value.
    After,
    /// Chip only; the value becomes the element `title`.
    None,
}

/// The chip shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwatchShape {
    /// A filled square (default).
    #[default]
    Square,
    /// A filled round dot.
    Round,
    /// A hollow ring (the color is the border, not the fill).
    Ring,
}

impl SwatchShape {
    fn css_name(self) -> &'static str {
        match self {
            SwatchShape::Square => "square",
            SwatchShape::Round => "round",
            SwatchShape::Ring => "ring",
        }
    }
}

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
pub struct ColorSwatch {
    position: SwatchPosition,
    shape: SwatchShape,
    tint: bool,
    reveal: bool,
}

impl ColorSwatch {
    /// Create a color swatch extension with the default render
    /// (chip before the value, square shape, no tint).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the chip position relative to the value.
    pub fn position(mut self, position: SwatchPosition) -> Self {
        self.position = position;
        self
    }

    /// Set the chip shape.
    pub fn shape(mut self, shape: SwatchShape) -> Self {
        self.shape = shape;
        self
    }

    /// Paint a faint `color-mix()` tint of the color behind the whole swatch.
    pub fn tint(mut self, tint: bool) -> Self {
        self.tint = tint;
        self
    }

    /// Collapse the value text and reveal it on hover / keyboard focus (pure-CSS,
    /// driven by the `swatch-reveal` class). The value stays in the DOM for
    /// assistive tech. Ignored when the position is `None` (already hidden).
    pub fn reveal(mut self, reveal: bool) -> Self {
        self.reveal = reveal;
        self
    }

    fn render_swatch(&self, attrs: Option<&Attrs>, color: &str) -> String {
        let mut label = escape_text(color);

        // A ring shows the color as the border; filled shapes as the background.
        let chip_class = match self.shape {
            SwatchShape::Square => "swatch-chip".to_string(),
            shape => format!("swatch-chip swatch-chip-{}", shape.css_name()),
        };
        let chip_style = match self.shape {
            SwatchShape::Ring => format!("border-color:{}", escape_attr(color)),
            _ => format!("background-color:{}", escape_attr(color)),
        };
        let chip = format!(
            "<span class=\"{}\" style=\"{}\"></span>",
            chip_class, chip_style
        );

        let mut extra_classes: Vec<&str> = Vec::new();
        let tint_style = if self.tint {
            extra_classes.push("swatch-tint");
            Some(format!(
                "background-color:color-mix(in srgb, {} 12%, transparent)",
                color,
            ))
        } else {
            None
        };

        let mut extra_kvs: Vec<(&str, &str)> = Vec::new();
        let inner = match self.position {
            SwatchPosition::None => {
                // Chip only: surface the value as the element title so it stays
                // available on hover and to assistive technology. `reveal` is
                // meaningless here (there is no inline value) and ignored.
                extra_classes.push("swatch-chip-only");
                extra_kvs.push(("title", color));
                chip.clone()
            }
            position => {
                // When revealing, wrap the value so CSS can collapse / expand it,
                // make the swatch keyboard-focusable, and keep the value in the DOM.
                if self.reveal {
                    extra_classes.push("swatch-reveal");
                    extra_kvs.push(("tabindex", "0"));
                    label = format!("<span class=\"swatch-val\">{}</span>", label);
                }
                match position {
                    SwatchPosition::After => format!("{} {}", label, chip),
                    _ => format!("{} {}", chip, label),
                }
            }
        };

        format!(
            "<span{}>{}</span>",
            open_attrs(
                attrs,
                "swatch",
                &extra_classes,
                tint_style.as_deref(),
                &extra_kvs
            ),
            inner,
        )
    }

    fn render_label(&self, attrs: Option<&Attrs>, color: &str, text_color: &str) -> String {
        format!(
            "<span{}>{}</span>",
            open_label_attrs(attrs, color, text_color),
            escape_text(color),
        )
    }
}

impl CarveExtension for ColorSwatch {
    fn name(&self) -> &'static str {
        "color"
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != ROLE {
            return None;
        }
        let value = inline_text(&node.children);
        let contrast = has_attr(node.attrs.as_ref(), "contrast");
        let attrs = if contrast {
            without_attr(node.attrs.as_ref(), "contrast")
        } else {
            None
        };
        let render_attrs = attrs.as_ref().or(node.attrs.as_ref());
        let Some(color) = safe_color(&value) else {
            return contrast
                .then(|| generic_fallback(&node.name, render_attrs, &node.children, ctx));
        };
        if contrast {
            if let Some(text_color) = auto_text_color(color) {
                return Some(self.render_label(render_attrs, color, text_color));
            }
        }
        Some(self.render_swatch(render_attrs, color))
    }
}

/// Build the output element's attribute string: the base class first, then any
/// extension classes, then author classes; an optional extension `style`, then
/// any extension key-values (each applied only when the author did not set its
/// own); then the author's remaining attributes. Mirrors the spoiler extension's
/// class merge.
fn open_attrs(
    attrs: Option<&Attrs>,
    base: &str,
    extra_classes: &[&str],
    extra_style: Option<&str>,
    extra_kvs: &[(&str, &str)],
) -> String {
    let mut classes: Vec<String> = base.split(' ').map(str::to_string).collect();
    for class in extra_classes {
        let class = (*class).to_string();
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    if let Some(a) = attrs {
        for class in &a.classes {
            if !classes.contains(class) {
                classes.push(class.clone());
            }
        }
    }

    let mut out = format!(" class=\"{}\"", escape_attr(&classes.join(" ")));

    let author_has = |key: &str| attrs.is_some_and(|a| a.key_values.contains_key(key));
    if let Some(style) = extra_style {
        if !author_has("style") {
            out.push_str(&format!(" style=\"{}\"", escape_attr(style)));
        }
    }
    for (key, value) in extra_kvs {
        if !author_has(key) {
            out.push_str(&format!(" {}=\"{}\"", key, escape_attr(value)));
        }
    }
    if let Some(a) = attrs {
        out.push_str(&render_attrs_after_class(a));
    }
    out
}

fn open_label_attrs(attrs: Option<&Attrs>, color: &str, text_color: &str) -> String {
    // The computed colors go last so author attributes keep their source order;
    // an explicit author `style` wins and suppresses ours (which also avoids
    // emitting a duplicate `style` attribute).
    let mut out = open_attrs(attrs, "swatch-label", &[], None, &[]);
    let author_has_style = attrs.is_some_and(|a| a.key_values.contains_key("style"));
    if !author_has_style {
        out.push_str(&format!(
            " style=\"{}\"",
            escape_attr(&format!("background:{color};color:{text_color}"))
        ));
    }
    out
}

fn generic_fallback(
    name: &str,
    attrs: Option<&Attrs>,
    children: &[InlineNode],
    ctx: &RenderContext<'_>,
) -> String {
    let base = format!("ext-{name}");
    let mut classes = vec![base];
    if let Some(a) = attrs {
        for class in &a.classes {
            if !classes.contains(class) {
                classes.push(class.clone());
            }
        }
    }
    let mut out = format!("<span class=\"{}\"", escape_attr(&classes.join(" ")));
    if let Some(a) = attrs {
        out.push_str(&render_attrs_after_class(a));
    }
    out.push('>');
    out.push_str(&ctx.render_inlines(children));
    out.push_str("</span>");
    out
}

fn has_attr(attrs: Option<&Attrs>, key: &str) -> bool {
    attrs.is_some_and(|a| a.key_values.contains_key(key))
}

fn without_attr(attrs: Option<&Attrs>, key: &str) -> Option<Attrs> {
    let mut attrs = attrs?.clone();
    attrs.key_values.remove(key);
    attrs.order.retain(|slot| match slot {
        AttrSlot::Key(k) => k != key,
        _ => true,
    });
    Some(attrs)
}

fn auto_text_color(color: &str) -> Option<&'static str> {
    let (r, g, b) = parse_rgb_bytes(color)?;
    let brightness = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
    if brightness >= 128 {
        Some("#000")
    } else {
        Some("#fff")
    }
}

fn parse_rgb_bytes(value: &str) -> Option<(u8, u8, u8)> {
    parse_hex_rgb(value).or_else(|| parse_rgb_function(value))
}

fn parse_hex_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.strip_prefix('#')?;
    match hex.len() {
        3 | 4 => {
            let mut nibbles = hex.bytes().map(hex_nibble);
            let r = nibbles.next()?? * 17;
            let g = nibbles.next()?? * 17;
            let b = nibbles.next()?? * 17;
            Some((r, g, b))
        }
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_rgb_function(value: &str) -> Option<(u8, u8, u8)> {
    let open = value.find('(')?;
    if !value.ends_with(')') {
        return None;
    }
    let name = &value[..open];
    if !matches!(name, "rgb" | "rgba") {
        return None;
    }
    let inner = &value[open + 1..value.len() - 1];
    let mut values = inner
        .split(|c: char| c == ',' || c == '/' || c.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .take(3)
        .map(parse_integer_byte);
    Some((values.next()??, values.next()??, values.next()??))
}

fn parse_integer_byte(token: &str) -> Option<u8> {
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = token.parse::<u16>().ok()?;
    Some(n.min(255) as u8)
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

/// A bare keyword is a color only when it is an actual CSS named color (or
/// `transparent` / `currentcolor`), matched case-insensitively. Arbitrary words
/// like `banana` are not (parity with carve-php / carve-js).
fn is_named_color(value: &str) -> bool {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    NAMED_COLORS.contains(&lower.as_str())
}

/// The CSS named colors plus `transparent` / `currentcolor` (lowercase).
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
    "darkviolet",
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
