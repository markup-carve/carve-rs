//! PROVISIONAL: a standalone SVG sanitizer, the port of carve-php
//! `SvgSanitizer` (itself the port of carve-js `sanitizeSvg`).
//!
//! A tokenizer, not a regex scrub: an element off the allowlist is dropped
//! with its subtree, an attribute off the allowlist and every `on*` handler is
//! dropped, URL and style values are checked, and only the survivors are
//! written again. Output bytes equal carve-php on the same input, which takes
//! carve-js output except for the fixes of carve#6755, carve#6760,
//! carve#6761 and carve#6774.
// Every index sits on an ASCII byte the scan has just checked.
#![allow(clippy::indexing_slicing)]

use crate::options::UrlPolicy;
use crate::tables::{DEFAULT_DENIED_SCHEMES, PROBE_REMOVES, STYLE_PATTERNS};
use std::borrow::Cow;
use std::collections::HashSet;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// The only prefixed names written, each exactly in this spelling: prefixes
/// are case-sensitive and the root binds `xlink` when needed (carve#6782,
/// carve#6783).
const PREFIXED_NAMES: &[&str] = &["xlink:href", "xmlns:xlink", "xml:space"];

/// A decoded lone surrogate, which a `str` cannot hold, reads as this.
const SURROGATE_STANDIN: char = '\u{E000}';

const ALLOWED_TAGS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "symbol",
    "use",
    "title",
    "desc",
    "switch",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "textPath",
    "marker",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
    "mask",
    "pattern",
    "filter",
    "feGaussianBlur",
    "feOffset",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feFuncA",
    "feFuncR",
    "feFuncG",
    "feFuncB",
    "feComposite",
    "feFlood",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feTile",
    "feTurbulence",
    "feDropShadow",
    "feImage",
    "feDisplacementMap",
];

/// Lower case: `tag_allowed` looks names up lowered (carve#6765).
const ANIMATION_TAGS: &[&str] = &[
    "animate",
    "animatetransform",
    "animatemotion",
    "set",
    "mpath",
];

/// Matched in lower case.
const ALLOWED_ATTRS: &[&str] = &[
    "d",
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
    "viewbox",
    "points",
    "transform",
    "pathlength",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-opacity",
    "opacity",
    "color",
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientunits",
    "gradienttransform",
    "spreadmethod",
    "patternunits",
    "patterntransform",
    "patterncontentunits",
    "clippathunits",
    "maskunits",
    "maskcontentunits",
    "markerwidth",
    "markerheight",
    "markerunits",
    "orient",
    "refx",
    "refy",
    "preserveaspectratio",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-anchor",
    "dominant-baseline",
    "letter-spacing",
    "word-spacing",
    "clip-path",
    "clip-rule",
    "mask",
    "marker-start",
    "marker-mid",
    "marker-end",
    "stddeviation",
    "in",
    "in2",
    "result",
    "mode",
    "operator",
    "values",
    "type",
    "flood-color",
    "flood-opacity",
    "attributename",
    "begin",
    "dur",
    "from",
    "to",
    "repeatcount",
    "keytimes",
    "keysplines",
    "calcmode",
    "additive",
    "accumulate",
    "class",
    "id",
    "role",
    "xmlns",
    "xmlns:xlink",
    "xml:space",
    "version",
];

/// Values that may only hold a local `#id` reference or a literal; SMIL lists
/// are checked per `;` entry.
const REF_VALUE_ATTRS: &[&str] = &[
    "fill",
    "stroke",
    "filter",
    "clip-path",
    "mask",
    "marker-start",
    "marker-mid",
    "marker-end",
    "color",
    "stop-color",
    "flood-color",
    "values",
    "from",
    "to",
    "by",
];

/// The same list as the default deny list of C2-H-128, kept separately as in
/// carve-js.
const DANGEROUS_URL_SCHEMES: &[&str] = &[
    "javascript",
    "vbscript",
    "data",
    "file",
    "ms-msdt",
    "ms-office",
    "ms-word",
    "ms-excel",
    "ms-powerpoint",
    "ms-access",
    "ms-visio",
    "ms-project",
    "ms-publisher",
    "ms-infopath",
    "ms-spd",
    "ms-search",
    "search-ms",
    "ms-cxh",
    "ms-cxh-full",
    "shell",
    "vscode",
    "vscode-insiders",
    "jar",
];

const ANIMATION_VALUE_ATTRS: &[&str] = &["values", "from", "to", "by"];

/// The named references that can spell a scheme; decoded for checks only.
const NAMED_REFS: &[(&str, &str)] = &[
    ("colon", ":"),
    ("semi", ";"),
    ("sol", "/"),
    ("tab", "\t"),
    ("newline", "\n"),
    ("lpar", "("),
    ("rpar", ")"),
    ("amp", "&"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", "\u{A0}"),
];

/// The options of [`sanitize_svg`]; every flag defaults to off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SanitizeSvgOptions {
    /// Keep the `style` attribute when its value cannot fetch or execute. The
    /// `<style>` element is always dropped.
    pub allow_style: bool,
    /// Keep `<a>` and its external `href` (safe schemes only).
    pub allow_links: bool,
    /// Keep the SMIL elements `animate`, `animateTransform`, `animateMotion`,
    /// `set` and `mpath`.
    pub allow_animation: bool,
    /// Keep `<image>` and its external `href` (safe schemes only; `data:`
    /// stays refused).
    pub allow_external_images: bool,
    /// The scheme policy of C2-H-128; `None` is the default deny list.
    pub url_policy: Option<UrlPolicy>,
}

/// The result of [`sanitize_svg`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizeResult {
    /// The sanitized SVG; empty when `ok` is false.
    pub svg: String,
    /// True when the input is one well-formed `<svg>` root. When false, show
    /// the source as text, never the input.
    pub ok: bool,
}

/// Sanitizes an SVG source string. The output holds no `<script>`,
/// `<foreignObject>`, `<style>`, event handler, dangerous or external URL,
/// or active CSS, and is safe to inline into the DOM.
#[must_use]
pub fn sanitize_svg(source: &str, opts: &SanitizeSvgOptions) -> SanitizeResult {
    let mut state = State {
        opts,
        out: Vec::new(),
        drop_stack: Vec::new(),
        kept: Vec::new(),
        saw_root: false,
        root_self_closed: false,
        bind_xlink: false,
    };
    state.run(source).map_or_else(
        || SanitizeResult {
            svg: String::new(),
            ok: false,
        },
        |svg| SanitizeResult { svg, ok: true },
    )
}

struct State<'o> {
    opts: &'o SanitizeSvgOptions,
    out: Vec<String>,
    drop_stack: Vec<String>,
    /// Name, the output index where its content starts, and whether a written
    /// `xmlns:xlink` is in scope.
    kept: Vec<(String, usize, bool)>,
    saw_root: bool,
    root_self_closed: bool,
    /// An `xlink:href` was written with no `xmlns:xlink` in scope (carve#6782).
    bind_xlink: bool,
}

#[derive(Default)]
struct XlinkUse {
    declared: bool,
    used: bool,
}

impl State<'_> {
    fn run(&mut self, source: &str) -> Option<String> {
        let cleaned = xml_chars_only(source);
        let src = trim(&cleaned);
        let mut cursor = 0;
        while let Some(found) = src[cursor..].find('<') {
            let start = cursor + found;
            self.consume_text(&src[cursor..start]).then_some(())?;
            let end = token_end(src, start)?;
            self.consume_token(&src[start..end]).then_some(())?;
            cursor = end;
        }
        if !self.consume_text(&src[cursor..])
            || !self.saw_root
            || !self.kept.is_empty()
            || !self.drop_stack.is_empty()
        {
            return None;
        }
        if self.bind_xlink {
            let at = "<svg xmlns=\"".len() + SVG_NS.len() + 1;
            self.out[0].insert_str(at, &format!(" xmlns:xlink=\"{XLINK_NS}\""));
        }
        let svg = self.out.concat();
        let head = svg.as_bytes();
        if head.len() < 5
            || !head[..4].eq_ignore_ascii_case(b"<svg")
            || !matches!(
                head[4],
                b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r' | b'/' | b'>'
            )
        {
            return None;
        }
        let body = svg.trim_end_matches(is_ws);
        let tail_ok = if self.root_self_closed {
            body.ends_with("/>")
        } else {
            body.len() >= 6 && body.as_bytes()[body.len() - 6..].eq_ignore_ascii_case(b"</svg>")
        };
        tail_ok.then_some(svg)
    }

    fn consume_text(&mut self, between: &str) -> bool {
        if between.is_empty() {
            return true;
        }
        if !self.saw_root {
            return trim(between).is_empty();
        }
        if self.drop_stack.is_empty() {
            self.out.push(escape_text(between));
        }
        true
    }

    fn consume_token(&mut self, token: &str) -> bool {
        if let Some(name) = closing_name(token) {
            return self.close_tag(name);
        }
        let Some(rest) = token.strip_prefix('<') else {
            return true;
        };
        let name_len = name_length(rest);
        if name_len == 0 {
            return true;
        }
        let name = &rest[..name_len];
        let self_close = token.ends_with("/>");
        let raw_end = token.len() - if self_close { 2 } else { 1 };
        let raw = token.get(name_len + 1..raw_end).unwrap_or("");
        self.open_tag(name, raw, self_close)
    }

    fn close_tag(&mut self, name: &str) -> bool {
        if !self.drop_stack.is_empty() {
            return self.drop_stack.pop().as_deref() == Some(name);
        }
        let Some((open, at, _)) = self.kept.pop() else {
            return false;
        };
        if open != name {
            return false;
        }
        if self.out.len() == at {
            if let Some(opener) = self.out.last_mut() {
                opener.pop();
                opener.push_str("/>");
            }
            if name == "svg" {
                self.root_self_closed = true;
            }
        } else {
            self.out.push(format!("</{name}>"));
        }
        true
    }

    fn open_tag(&mut self, name: &str, raw: &str, self_close: bool) -> bool {
        let outside = self.kept.is_empty() && self.drop_stack.is_empty();
        let is_root = outside && !self.saw_root;
        if (is_root && name != "svg") || (outside && self.saw_root) {
            return false;
        }
        if !self.drop_stack.is_empty() || !self.tag_allowed(name) {
            if !self_close {
                self.drop_stack.push(name.to_string());
            }
            return true;
        }
        if is_root {
            self.saw_root = true;
            self.root_self_closed = self_close;
        }
        self.write_open(name, raw, is_root, self_close);
        true
    }

    fn write_open(&mut self, name: &str, raw: &str, is_root: bool, self_close: bool) {
        let mut tag = String::with_capacity(name.len() + raw.len() + 48);
        tag.push('<');
        tag.push_str(name);
        if is_root {
            tag.push_str(" xmlns=\"");
            tag.push_str(SVG_NS);
            tag.push('"');
        }
        let mut xlink = XlinkUse::default();
        self.sanitize_attrs(raw, name, is_root, &mut tag, &mut xlink);
        let xlink_bound = xlink.declared || self.kept.last().is_some_and(|k| k.2);
        if xlink.used && !xlink_bound {
            self.bind_xlink = true;
        }
        tag.push_str(if self_close { "/>" } else { ">" });
        self.out.push(tag);
        if !self_close {
            self.kept
                .push((name.to_string(), self.out.len(), xlink_bound));
        }
    }

    fn tag_allowed(&self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        if ALLOWED_TAGS.contains(&name) || ALLOWED_TAGS.contains(&lower.as_str()) {
            return true;
        }
        (self.opts.allow_links && lower == "a")
            || (self.opts.allow_animation && ANIMATION_TAGS.contains(&lower.as_str()))
            || (self.opts.allow_external_images && lower == "image")
    }

    fn sanitize_attrs(
        &self,
        raw: &str,
        tag: &str,
        is_root: bool,
        out: &mut String,
        xlink: &mut XlinkUse,
    ) {
        let lower_tag = tag.to_ascii_lowercase();
        let external_href = (self.opts.allow_links && lower_tag == "a")
            || (self.opts.allow_external_images && lower_tag == "image");
        let attrs = parse_attrs(raw);
        let animates_href = animates_href(&attrs);
        let mut seen: HashSet<&str> = HashSet::with_capacity(attrs.len());
        for &(name, value) in &attrs {
            if !seen.insert(name) {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            // carve#6755: the root's own xmlns goes by name; a regex over the
            // written attributes also matched values.
            if is_root && lower == "xmlns" {
                continue;
            }
            if animates_href
                && ANIMATION_VALUE_ATTRS.contains(&lower.as_str())
                && value.is_some_and(|value| !local_refs(value))
            {
                continue;
            }
            let before = out.len();
            self.sanitize_attribute(name, &lower, value, external_href, out);
            if out.len() > before {
                xlink.used |= name == "xlink:href";
                xlink.declared |= name == "xmlns:xlink";
            }
        }
    }

    fn sanitize_attribute(
        &self,
        name: &str,
        lower: &str,
        value: Option<&str>,
        external_href: bool,
        out: &mut String,
    ) {
        // carve#6774: XML has no valueless attribute and none allowed here is boolean.
        let Some(value) = value else {
            return;
        };
        if lower.starts_with("on") || namespace_unwritable(name, value) {
            return;
        }
        if lower == "href" || lower == "xlink:href" {
            if self.url_value_allowed(value, external_href) {
                write_attr(out, name, value);
            }
            return;
        }
        if lower == "style" {
            if self.opts.allow_style && !style_is_dangerous(value) {
                write_attr(out, name, value);
            }
            return;
        }
        if !lower.starts_with("aria-")
            && !lower.starts_with("data-")
            && !ALLOWED_ATTRS.contains(&lower)
        {
            return;
        }
        let unsafe_value = if REF_VALUE_ATTRS.contains(&lower) {
            ref_attr_unsafe(value)
        } else {
            value_has_external_ref(value)
        };
        if !unsafe_value {
            write_attr(out, name, value);
        }
    }

    fn url_value_allowed(&self, value: &str, external_href: bool) -> bool {
        let decoded = normalize_for_check(value);
        (decoded.starts_with('#') || external_href)
            && scheme_is_safe(&decoded)
            && self.url_allowed(&decoded)
    }

    /// The scheme of C2-H-159 against the `urlPolicy` lists.
    fn url_allowed(&self, url: &str) -> bool {
        let scheme = url_scheme(url);
        if scheme.is_empty() {
            return true;
        }
        match &self.opts.url_policy {
            Some(UrlPolicy::Allow(allowed)) => {
                allowed.iter().any(|s| s.to_ascii_lowercase() == scheme)
            }
            Some(UrlPolicy::Deny(denied)) => {
                !denied.iter().any(|s| s.to_ascii_lowercase() == scheme)
            }
            None => !DEFAULT_DENIED_SCHEMES.contains(&scheme.as_str()),
        }
    }
}

fn write_attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape_attr(value));
    out.push('"');
}

/// carve#6764: raw characters outside XML `Char` go before any check, so a
/// check never sees a value the output does not hold. The byte scan finds C0
/// controls and the 0xEF lead byte of U+FFFE / U+FFFF.
fn xml_chars_only(source: &str) -> Cow<'_, str> {
    let suspect = source
        .bytes()
        .any(|b| (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r')) || b == 0xEF);
    if suspect && source.contains(non_xml_char) {
        Cow::Owned(source.chars().filter(|&c| !non_xml_char(c)).collect())
    } else {
        Cow::Borrowed(source)
    }
}

/// JavaScript's `\s` and `String.prototype.trim()` set.
fn is_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn trim(text: &str) -> &str {
    text.trim_matches(is_ws)
}

/// Removed before a scheme is read; wider than what a target may emit.
fn scheme_strip(c: char) -> bool {
    matches!(c, '\0'..=' ' | '\u{7F}'..='\u{9F}') || is_ws(c)
}

fn strip_scheme_chars(text: &str) -> String {
    text.chars().filter(|&c| !scheme_strip(c)).collect()
}

fn name_start(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-')
}

/// The length of `[A-Za-z][A-Za-z0-9_:.-]*` at the start, or 0.
fn name_length(text: &str) -> usize {
    let b = text.as_bytes();
    if b.first().is_none_or(|&c| !name_start(c)) {
        return 0;
    }
    1 + b[1..].iter().take_while(|&&c| name_char(c)).count()
}

/// The name of `</name WS*>`, the whole token.
fn closing_name(token: &str) -> Option<&str> {
    let rest = token.strip_prefix("</")?;
    let len = name_length(rest);
    if len == 0 {
        return None;
    }
    let tail = rest[len..].strip_suffix('>')?;
    tail.chars().all(is_ws).then(|| &rest[..len])
}

/// The offset after the token, or `None` when it does not close.
fn token_end(src: &str, start: usize) -> Option<usize> {
    for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")] {
        if src[start..].starts_with(open) {
            let from = start + open.len();
            return src[from..].find(close).map(|at| from + at + close.len());
        }
    }
    let b = src.as_bytes();
    let mut at = start + 1;
    while at < b.len() {
        match b[at] {
            b'>' => return Some(at + 1),
            b'<' => return None,
            q @ (b'"' | b'\'') => {
                at += 1 + b[at + 1..].iter().position(|&c| c == q)?;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn skip_space(raw: &str, at: usize) -> usize {
    at + raw[at..]
        .char_indices()
        .find(|&(_, c)| !is_ws(c))
        .map_or(raw.len() - at, |(i, _)| i)
}

fn parse_attrs(raw: &str) -> Vec<(&str, Option<&str>)> {
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut at = 0;
    while at < b.len() {
        at = skip_space(raw, at);
        if at >= b.len() {
            break;
        }
        let c = b[at];
        if !(c.is_ascii_alphabetic() || c == b'_' || c == b':') {
            at += raw[at..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let len = 1 + b[at + 1..].iter().take_while(|&&c| name_char(c)).count();
        let name = &raw[at..at + len];
        at = skip_space(raw, at + len);
        if b.get(at) != Some(&b'=') {
            out.push((name, None));
            continue;
        }
        at = skip_space(raw, at + 1);
        if let Some(&q @ (b'"' | b'\'')) = b.get(at) {
            let end = b[at + 1..]
                .iter()
                .position(|&c| c == q)
                .map_or(b.len(), |i| at + 1 + i);
            out.push((name, Some(&raw[at + 1..end])));
            at = end + 1;
        } else {
            let len = raw[at..]
                .char_indices()
                .find(|&(_, c)| c == '>' || is_ws(c))
                .map_or(raw.len() - at, |(i, _)| i);
            out.push((name, Some(&raw[at..at + len])));
            at += len;
        }
    }
    out
}

/// carve#6761: an animation of `href` may only hold `#id` entries, like the
/// `href` it changes.
fn animates_href(attrs: &[(&str, Option<&str>)]) -> bool {
    attrs.iter().any(|&(name, value)| {
        name.eq_ignore_ascii_case("attributename")
            && value.is_some_and(|value| {
                let target = trim(&normalize_for_check(value)).to_ascii_lowercase();
                target == "href" || target == "xlink:href"
            })
    })
}

fn local_refs(value: &str) -> bool {
    normalize_for_check(value).split(';').all(|segment| {
        let segment = trim(segment);
        segment.is_empty() || segment.starts_with('#')
    })
}

fn probe_removed(c: char) -> bool {
    if ('!'..='~').contains(&c) {
        return false;
    }
    PROBE_REMOVES
        .iter()
        .any(|&(from, to)| (from..=to).contains(&c))
}

/// The scheme of C2-H-159: probe-removed characters are skipped, the first
/// kept one must be a letter.
fn url_scheme(url: &str) -> String {
    let mut scheme = String::new();
    for c in url.chars() {
        if probe_removed(c) {
            continue;
        }
        if c == ':' {
            return scheme.to_ascii_lowercase();
        }
        let ok = if scheme.is_empty() {
            c.is_ascii_alphabetic()
        } else {
            c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
        };
        if !ok {
            return String::new();
        }
        scheme.push(c);
    }
    String::new()
}

/// The scheme of `^[a-zA-Z][a-zA-Z0-9+.-]*:` after the strip, if any.
fn leading_scheme(probe: &str) -> Option<&str> {
    let b = probe.as_bytes();
    if b.first().is_none_or(|c| !c.is_ascii_alphabetic()) {
        return None;
    }
    let len = 1 + b[1..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-'))
        .count();
    (b.get(len) == Some(&b':')).then(|| &probe[..len])
}

fn scheme_is_safe(url: &str) -> bool {
    let probe = strip_scheme_chars(url);
    leading_scheme(&probe)
        .is_none_or(|scheme| !DANGEROUS_URL_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()))
}

fn code_point(cp: u32) -> Option<char> {
    if (0xD800..=0xDFFF).contains(&cp) {
        return Some(SURROGATE_STANDIN);
    }
    char::from_u32(cp)
}

/// The reference at `&` that `&(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z][a-zA-Z0-9]*);`
/// reads: its length and its replacement.
fn entity_at(text: &str) -> Option<(usize, Option<char>, Option<&'static str>)> {
    let b = text.as_bytes();
    if b.get(1) == Some(&b'#') {
        let (hex, digits_at) = match b.get(2) {
            Some(b'x' | b'X') if b.get(3).is_some_and(u8::is_ascii_hexdigit) => (true, 3),
            _ => (false, 2),
        };
        let digits = b[digits_at..]
            .iter()
            .take_while(|c| {
                if hex {
                    c.is_ascii_hexdigit()
                } else {
                    c.is_ascii_digit()
                }
            })
            .count();
        if digits == 0 || b.get(digits_at + digits) != Some(&b';') {
            return None;
        }
        let len = digits_at + digits + 1;
        let body = text[digits_at..digits_at + digits].trim_start_matches('0');
        if body.len() > if hex { 6 } else { 7 } {
            return Some((len, None, None));
        }
        let cp = if body.is_empty() {
            0
        } else {
            u32::from_str_radix(body, if hex { 16 } else { 10 }).ok()?
        };
        return Some((len, code_point(cp), None));
    }
    if !b.get(1).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let name_len = 1 + b[2..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric())
        .count();
    if b.get(1 + name_len) != Some(&b';') {
        return None;
    }
    let name = text[1..=name_len].to_ascii_lowercase();
    let named = NAMED_REFS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, v)| *v);
    Some((name_len + 2, None, named))
}

fn decode_entities(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some((len, char_value, named)) = entity_at(rest) {
            if let Some(c) = char_value {
                out.push(c);
            } else if let Some(text) = named {
                out.push_str(text);
            } else {
                out.push_str(&rest[..len]);
            }
            rest = &rest[len..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn decode_css_escapes(value: &str) -> String {
    if !value.contains('\\') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut chars = value.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(&(at, first)) = chars.peek() else {
            out.push('\\');
            break;
        };
        if !first.is_ascii_hexdigit() {
            chars.next();
            out.push(first);
            continue;
        }
        let digits = value[at..]
            .bytes()
            .take(6)
            .take_while(u8::is_ascii_hexdigit)
            .count();
        for _ in 0..digits {
            chars.next();
        }
        if chars.peek().is_some_and(|&(_, c)| is_ws(c)) {
            chars.next();
        }
        if let Some(c) = u32::from_str_radix(&value[at..at + digits], 16)
            .ok()
            .and_then(code_point)
        {
            out.push(c);
        }
    }
    out
}

fn normalize_for_check(value: &str) -> String {
    if !value.contains('&') {
        return decode_css_escapes(value);
    }
    decode_css_escapes(&decode_entities(value))
}

/// Drops `/* ... */` comments, the first `*/` closing each.
fn strip_css_comments(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find("/*") {
        let Some(close) = rest[open + 2..].find("*/") else {
            break;
        };
        out.push_str(&rest[..open]);
        rest = &rest[open + 2 + close + 2..];
    }
    out.push_str(rest);
    out
}

fn style_is_dangerous(value: &str) -> bool {
    let compact: String = normalize_for_check(&strip_css_comments(value))
        .to_lowercase()
        .chars()
        .filter(|&c| !is_ws(c))
        .collect();
    // carve#6760: the core list of C2-H-162 (CSS image functions fetch from a
    // bare string) plus `javascript:`.
    STYLE_PATTERNS.iter().any(|p| compact.contains(p)) || compact.contains("javascript:")
}

/// Any `url(` whose content does not start with `#` after CSS whitespace and
/// one optional quote (carve#6763). Only CSS whitespace is skipped: a URL keeps
/// a leading no-break space, so `url(\u{A0}#g)` names another resource
/// (carve#6773).
fn non_local_url(value: &str) -> bool {
    let b = value.as_bytes();
    let css_ws = |c: &&u8| matches!(c, b'\t' | b'\n' | 0x0C | b'\r' | b' ');
    (0..b.len().saturating_sub(3)).any(|at| {
        if !b[at..at + 4].eq_ignore_ascii_case(b"url(") {
            return false;
        }
        let mut next = at + 4;
        next += b[next..].iter().take_while(css_ws).count();
        if matches!(b.get(next), Some(b'"' | b'\'')) {
            next += 1;
        }
        next += b[next..].iter().take_while(css_ws).count();
        b.get(next) != Some(&b'#')
    })
}

fn ref_attr_unsafe(value: &str) -> bool {
    normalize_for_check(value).split(';').any(|segment| {
        let segment = trim(segment);
        if non_local_url(segment) {
            return true;
        }
        // A slash-led path (`//host/x`, `/abs`) fetches remotely like an
        // absolute URL.
        let probe = strip_scheme_chars(segment);
        leading_scheme(&probe).is_some() || probe.starts_with('/')
    })
}

fn value_has_external_ref(value: &str) -> bool {
    let decoded = normalize_for_check(value);
    non_local_url(&decoded) || !scheme_is_safe(&decoded)
}

/// Keeps the entities XML defines (the five names and numeric references to
/// an XML `Char`, carve#6764) and escapes every other `&`.
fn escape_amp(text: &str, out: &mut String) {
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        out.push('&');
        rest = &rest[at + 1..];
        if !xml_entity_follows(rest) {
            out.push_str("amp;");
        }
    }
    out.push_str(rest);
}

fn xml_entity_follows(rest: &str) -> bool {
    let b = rest.as_bytes();
    let numeric = |from: usize, hex: bool| {
        let n = b[from..]
            .iter()
            .take_while(|c| {
                if hex {
                    c.is_ascii_hexdigit()
                } else {
                    c.is_ascii_digit()
                }
            })
            .count();
        if n == 0 || b.get(from + n) != Some(&b';') {
            return false;
        }
        let digits = rest[from..from + n].trim_start_matches('0');
        if digits.len() > if hex { 6 } else { 7 } {
            return false;
        }
        let cp = u32::from_str_radix(digits, if hex { 16 } else { 10 }).unwrap_or(0);
        xml_char(cp)
    };
    match b.first() {
        Some(b'#') if b.get(1) == Some(&b'x') => numeric(2, true),
        Some(b'#') => numeric(1, false),
        _ => ["amp;", "lt;", "gt;", "quot;", "apos;"]
            .iter()
            .any(|name| rest.starts_with(name)),
    }
}

fn xml_char(cp: u32) -> bool {
    matches!(cp, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x1_0000..=0x10_FFFF)
}

/// A raw character outside the XML 1.0 `Char` production; a `str` holds no
/// surrogates.
fn non_xml_char(c: char) -> bool {
    !xml_char(u32::from(c))
}

fn escape_text(text: &str) -> String {
    let mut amp = String::with_capacity(text.len());
    escape_amp(text, &mut amp);
    amp.replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(text: &str) -> String {
    let mut amp = String::with_capacity(text.len());
    escape_amp(text, &mut amp);
    amp.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const NAMESPACE_WS: [char; 5] = [' ', '\t', '\n', '\r', '\u{A0}'];

/// A prefixed name off [`PREFIXED_NAMES`], or an `xmlns:xlink` whose value,
/// references read, is empty (Namespaces in XML forbids undeclaring a prefix,
/// carve#6779) or holds XML whitespace or U+00A0, which is no URI reference
/// (carve#6783, carve#6811). The same holds for a child's default `xmlns`,
/// except that an empty one is a legal undeclaration (carve#6843).
fn namespace_unwritable(name: &str, value: &str) -> bool {
    if name == "xmlns" {
        return !value.is_empty() && decode_entities(value).contains(NAMESPACE_WS);
    }
    if !name.contains(':') {
        return false;
    }
    if !PREFIXED_NAMES.contains(&name) {
        return true;
    }
    name == "xmlns:xlink" && {
        let decoded = decode_entities(value);
        decoded.is_empty() || decoded.contains(NAMESPACE_WS)
    }
}
