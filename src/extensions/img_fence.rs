//! SVG `img` fence (Tier-3, ships off). Faithful port of carve-js
//! `src/svg-fence.ts`.
//!
//! Claims fenced blocks whose info word is `img` (alias `image`) and renders the
//! SVG **body** - sanitized - rather than showing it as verbatim source. `svg` /
//! `xml` are deliberately NOT claimed, so an author can still syntax-highlight
//! SVG source with those words.
//!
//! Two emit modes, **sandbox by default**:
//!
//! - **sandbox (default):** the sanitized SVG is encoded into a
//!   `data:image/svg+xml` URI on an `<img>`, which the browser sandboxes - no
//!   script, no fetch, no DOM leakage - regardless of the sanitizer. `{alt=…}`
//!   sets the alt text.
//! - **inline (opt-in):** with [`ImgFence::allow_inline`], a fence marked
//!   `{inline}` renders a live `<svg>` in the DOM. Without it, `{inline}` is
//!   ignored and the fence stays sandboxed - an author cannot self-elevate out
//!   of the sandbox. Inline SVG is guarded only by this hand-rolled sanitizer
//!   (not a browser-grade parser); it is for TRUSTED author content, not a
//!   hardened XSS boundary for attacker-controlled input.
//!
//! A body that is not a single `<svg>` root degrades to an escaped code block.
//! Author `{#id .class}` on the fence merge onto the `<img>` (sandbox) or the
//! root `<svg>` (inline), hardened by the core attribute sanitizer and - for
//! inline - re-run through the SVG sanitizer.
//!
//! Like [`crate::extensions::fenced_render`], the transform runs in
//! `before_render` and only on the HTML target: it claims each matching
//! `CodeBlock` and replaces it with a `RawBlock`. For the Markdown / plain /
//! ANSI targets the `CodeBlock` is left untouched so those renderers emit it as
//! its source fence.

use crate::ast::{BlockNode, CodeBlock, Document, RawBlock};
use crate::escape::{escape_attr, escape_text};
use crate::extension::{BeforeRenderContext, CarveExtension};
use crate::render::render_attrs_without_keys;

use super::svg_sanitize::{sanitize_svg, SanitizeSvgOptions};

/// Fence attributes the extension consumes rather than emitting: the inline
/// mode flag, the `alt` text, and the now-redundant `sandbox` marker (sandbox is
/// the default; kept consumed so an explicit `{sandbox}` doesn't leak).
const CONSUMED_KEYS: &[&str] = &["inline", "alt", "sandbox"];

/// SVG `img` fence extension. See the module docs.
pub struct ImgFence {
    languages: Vec<String>,
    options: SanitizeSvgOptions,
    allow_inline: bool,
}

impl Default for ImgFence {
    fn default() -> Self {
        Self::new()
    }
}

impl ImgFence {
    /// A sandbox-only `img` fence claiming `img` / `image`, all sanitizer opts
    /// off.
    pub fn new() -> Self {
        Self {
            languages: vec!["img".into(), "image".into()],
            options: SanitizeSvgOptions::default(),
            allow_inline: false,
        }
    }

    /// Claim a single fence info word (replaces the default `img` / `image`).
    pub fn with_language(mut self, word: impl Into<String>) -> Self {
        let word = word.into();
        if !word.is_empty() {
            self.languages = vec![word];
        }
        self
    }

    /// Claim an explicit set of fence info words (empties are filtered; an
    /// all-empty list leaves the current words unchanged).
    pub fn with_languages<I, S>(mut self, words: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let filtered: Vec<String> = words
            .into_iter()
            .map(Into::into)
            .filter(|w| !w.is_empty())
            .collect();
        if !filtered.is_empty() {
            self.languages = filtered;
        }
        self
    }

    /// Keep the `style` attribute (value scrubbed of `url()`/`expression()`/…).
    pub fn allow_style(mut self, allow: bool) -> Self {
        self.options.allow_style = allow;
        self
    }

    /// Keep `<a>` elements and external `href`/`xlink:href` (safe schemes only).
    pub fn allow_links(mut self, allow: bool) -> Self {
        self.options.allow_links = allow;
        self
    }

    /// Keep SMIL animation elements (`<animate>`, `<set>`, …).
    pub fn allow_animation(mut self, allow: bool) -> Self {
        self.options.allow_animation = allow;
        self
    }

    /// Keep `<image>` and its external raster `href` (safe schemes only).
    pub fn allow_external_images(mut self, allow: bool) -> Self {
        self.options.allow_external_images = allow;
        self
    }

    /// Permit **inline** rendering (a live `<svg>` in the page DOM) for fences
    /// carrying an `{inline}` attribute. Default `false`: every fence is
    /// sandboxed and `{inline}` is ignored. A HOST decision on purpose - a
    /// per-fence `{inline}` alone must never self-elevate out of the sandbox.
    pub fn allow_inline(mut self, allow: bool) -> Self {
        self.allow_inline = allow;
        self
    }

    fn claims(&self, lang: Option<&str>) -> bool {
        lang.map(|l| self.languages.iter().any(|w| w == l))
            .unwrap_or(false)
    }
}

impl CarveExtension for ImgFence {
    fn name(&self) -> &'static str {
        "img-fence"
    }

    fn before_render(&self, mut doc: Document, ctx: &BeforeRenderContext<'_>) -> Document {
        // Only the HTML renderer emits the sanitized element. For Markdown /
        // plain / ANSI, leave the CodeBlock so those renderers emit its source
        // fence (matching fenced_render / carve-js). Inline SVG needs no client
        // script, so the static render is byte-identical to the interactive one.
        if !ctx.target_is_html() {
            return doc;
        }
        transform_blocks(&mut doc.children, self);
        // Footnote bodies render from footnote_defs (outside the tree), so a
        // claimed block inside a footnote must be transformed too.
        for blocks in doc.footnote_defs.values_mut() {
            transform_blocks(blocks, self);
        }
        doc
    }
}

/// Rewrite every claimed code block into its sanitized `RawBlock`, recursing
/// into containers exactly like `fenced_render::transform_blocks`.
fn transform_blocks(blocks: &mut [BlockNode], ext: &ImgFence) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::CodeBlock(code) if ext.claims(code.lang.as_deref()) => {
                let html = render_code_block(code, ext);
                *block = BlockNode::RawBlock(RawBlock {
                    format: "html".into(),
                    content: html,
                    // Synthesized by an extension: no source span to report (PART 12 §4).
                    pos: None,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    transform_blocks(&mut item.children, ext);
                }
            }
            BlockNode::BlockQuote(b) => transform_blocks(&mut b.children, ext),
            BlockNode::Admonition(a) => transform_blocks(&mut a.children, ext),
            BlockNode::Div(d) => transform_blocks(&mut d.children, ext),
            BlockNode::Extension(e) => transform_blocks(&mut e.children, ext),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        transform_blocks(def, ext);
                    }
                }
            }
            // KNOWN LIMITATION (parity gap with carve-js): a captioned fence is
            // parsed as `Figure { target: FigureTarget::CodeBlock, .. }`. carve-js
            // renders it as `<figure><img>…<figcaption>`, but carve-rs cannot: a
            // `before_render` transform can only swap a `CodeBlock` for a
            // `RawBlock`, and `FigureTarget` has no raw-HTML variant to hold the
            // sanitized output. So a captioned `img` fence degrades to its escaped
            // source (safe, never raw) - identical to how `fenced_render` (mermaid,
            // chart, …) already behaves for captioned fences. Fixing it belongs at
            // the extension-model level (a `FigureTarget::RawBlock`, or a
            // figure-renderer extension hook) so every block-transforming extension
            // benefits, not just this one. Tracked by the ignored parity test in
            // tests/img_fence.rs.
            _ => {}
        }
    }
}

/// The HTML for a claimed `img` fence: sandboxed `<img>` or (opt-in) inline SVG.
/// No leading indent / trailing newline - the `RawBlock` renderer adds the
/// indentation for the node's nesting level, matching carve-js `${pad}…`.
/// Fall back to the SVG's own `<title>` for the `<img>` alt text when the author
/// gave no `{alt=…}`, so a sandboxed image is described to assistive tech instead
/// of being silently decorative (empty alt). The svg is already sanitized, so
/// this is a plain extraction; the result is escaped again on output. Returns
/// `None` when there is no non-empty title.
fn svg_title(svg: &str) -> Option<String> {
    let lower = svg.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let inner_start = lower[open..].find('>')? + open + 1;
    let inner_end = lower[inner_start..].find("</title>")? + inner_start;
    // Strip any nested tags, then undo the entity escaping the sanitizer applied.
    let mut text = String::new();
    let mut in_tag = false;
    for c in svg[inner_start..inner_end].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let decoded = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let trimmed = decoded.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn render_code_block(code: &CodeBlock, ext: &ImgFence) -> String {
    let result = sanitize_svg(&code.content, &ext.options);
    if !result.ok {
        return source_fallback(code);
    }

    // Inline is a HOST capability: the `{inline}` fence flag only takes effect
    // when the host opted in with `allow_inline`. Otherwise the fence is
    // sandboxed and `{inline}` is ignored.
    let inline = ext.allow_inline && consumed_value(code, "inline").is_some();

    if !inline {
        let alt = consumed_value(code, "alt")
            .or_else(|| svg_title(&result.svg))
            .unwrap_or_default();
        let src = format!("data:image/svg+xml,{}", encode_uri_component(&result.svg));
        // Sandbox mode promises no fetches: drop any author `src` / `srcset` so
        // it cannot override the sanitized data URI with an external resource.
        let img_attrs = render_author_attrs(code, &["src", "srcset"]);
        return format!(
            "<img src=\"{}\" alt=\"{}\"{}>",
            escape_attr(&src),
            escape_attr(&alt),
            img_attrs
        );
    }

    let fence_attrs = render_author_attrs(code, &[]);
    if fence_attrs.is_empty() {
        return result.svg;
    }
    // Fence attributes land on the root <svg>, so they must clear the SAME
    // SVG-specific scrub as the body. Splice them onto the root, then
    // re-sanitize (idempotent for the already-clean body).
    let merged = sanitize_svg(&merge_into_root(&result.svg, &fence_attrs), &ext.options);
    if merged.ok {
        merged.svg
    } else {
        source_fallback(code)
    }
}

/// Case-insensitive lookup of a consumed key in the block's attributes (matching
/// how `CONSUMED_KEYS` are stripped), so `{Sandbox}` / `{ALT=…}` are honored.
fn consumed_value(code: &CodeBlock, key: &str) -> Option<String> {
    let attrs = code.attrs.as_ref()?;
    for (k, v) in &attrs.key_values {
        if k.eq_ignore_ascii_case(key) {
            return Some(v.clone());
        }
    }
    None
}

/// Render the author fence attributes (minus the consumed keys and any extra
/// stripped names) through the SAME core hardening every other element gets, so
/// a `{onclick=…}` on the fence cannot smuggle a handler.
fn render_author_attrs(code: &CodeBlock, extra_strip: &[&str]) -> String {
    let mut blocked: Vec<String> = CONSUMED_KEYS.iter().map(|s| s.to_string()).collect();
    blocked.extend(extra_strip.iter().map(|s| s.to_ascii_lowercase()));
    let refs: Vec<&str> = blocked.iter().map(String::as_str).collect();
    render_attrs_without_keys(&code.attrs, &refs)
}

/// A self-contained escaped code-block fallback: never blank, never raw.
fn source_fallback(code: &CodeBlock) -> String {
    let lang_attr = match code.lang.as_deref() {
        Some(l) if !l.is_empty() => format!(" class=\"language-{l}\""),
        _ => String::new(),
    };
    format!(
        "<pre><code{}>{}\n</code></pre>",
        lang_attr,
        escape_text(&code.content)
    )
}

/// Splice a rendered attr string (` id="…" class="…"`) into the root `<svg>`
/// tag. The fence attributes win: any attribute the fence sets is first removed
/// from the sanitized root so the merge never emits a duplicate. Attributes only
/// the root has are preserved. (The result is re-sanitized, so spacing is
/// normalized on the next pass.)
fn merge_into_root(svg: &str, attr_str: &str) -> String {
    if attr_str.is_empty() {
        return svg.to_string();
    }
    let fence_names = attr_names(attr_str);
    // Match the root tag quote-aware so a `>` inside a quoted attribute value is
    // not mistaken for the tag's end.
    let Some((root_attrs, slash, end)) = split_root_svg(svg) else {
        return svg.to_string();
    };
    let mut cleaned = root_attrs.to_string();
    for name in &fence_names {
        cleaned = remove_first_named_attr(&cleaned, name);
    }
    format!("<svg{attr_str}{cleaned}{slash}>{}", &svg[end..])
}

/// Collect the lowercased attribute names from a rendered attr string, mirroring
/// `\s([A-Za-z_:][\w:.-]*)\s*=`.
fn attr_names(attr_str: &str) -> Vec<String> {
    let chars: Vec<char> = attr_str.chars().collect();
    let n = chars.len();
    let mut names = Vec::new();
    let mut i = 0;
    while i < n {
        // \s
        if !is_ascii_ws(chars[i]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        if j >= n || !(chars[j].is_ascii_alphabetic() || chars[j] == '_' || chars[j] == ':') {
            i += 1;
            continue;
        }
        let name_start = j;
        j += 1;
        while j < n
            && (chars[j].is_ascii_alphanumeric()
                || chars[j] == '_'
                || chars[j] == ':'
                || chars[j] == '.'
                || chars[j] == '-')
        {
            j += 1;
        }
        let mut k = j;
        while k < n && is_ascii_ws(chars[k]) {
            k += 1;
        }
        if k < n && chars[k] == '=' {
            names.push(
                chars[name_start..j]
                    .iter()
                    .map(|c| c.to_ascii_lowercase())
                    .collect(),
            );
        }
        i += 1;
    }
    names
}

/// Split the leading `<svg…>` tag: `(root_attrs, slash, byte_index_after_tag)`.
/// Mirrors `^<svg((?:"[^"]*"|'[^']*'|[^>])*?)(\/?)>` on the sanitized root.
fn split_root_svg(svg: &str) -> Option<(&str, &str, usize)> {
    if svg.len() < 4 || !svg[..4].eq_ignore_ascii_case("<svg") {
        return None;
    }
    let b = svg.as_bytes();
    let len = b.len();
    let mut j = 4;
    loop {
        if j >= len {
            return None;
        }
        match b[j] {
            b'/' if j + 1 < len && b[j + 1] == b'>' => {
                return Some((&svg[4..j], "/", j + 2));
            }
            b'>' => return Some((&svg[4..j], "", j + 1)),
            b'"' => match memchr(b, b'"', j + 1) {
                Some(q) => j = q + 1,
                None => j += 1, // unbalanced: [^>] consumes it (root is well-formed anyway)
            },
            b'\'' => match memchr(b, b'\'', j + 1) {
                Some(q) => j = q + 1,
                None => j += 1,
            },
            _ => {
                let c = svg[j..].chars().next().unwrap();
                j += c.len_utf8();
            }
        }
    }
}

/// Remove the first ` name\s*=\s*(value)` occurrence (case-insensitive) from a
/// root-attr string, mirroring the per-name `.replace(…)` in carve-js.
fn remove_first_named_attr(root_attrs: &str, name: &str) -> String {
    let chars: Vec<char> = root_attrs.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if let Some(end) = named_attr_match(&chars, i, name) {
            let mut out: String = chars[..i].iter().collect();
            out.extend(chars[end..].iter());
            return out;
        }
        i += 1;
    }
    root_attrs.to_string()
}

/// Match ` name\s*=\s*(value)` (a single leading `\s`, then `name` ci) at
/// `start`. Returns the end index of the match, or `None`.
fn named_attr_match(chars: &[char], start: usize, name: &str) -> Option<usize> {
    let n = chars.len();
    let mut i = start;
    // single \s
    if i >= n || !is_ascii_ws(chars[i]) {
        return None;
    }
    i += 1;
    // name (ci)
    for nc in name.chars() {
        if i >= n || !chars[i].eq_ignore_ascii_case(&nc) {
            return None;
        }
        i += 1;
    }
    // \s*
    while i < n && is_ascii_ws(chars[i]) {
        i += 1;
    }
    if i >= n || chars[i] != '=' {
        return None;
    }
    i += 1;
    // \s*
    while i < n && is_ascii_ws(chars[i]) {
        i += 1;
    }
    if i >= n {
        return None;
    }
    // value: "[^"]*" | '[^']*' | [^\s>]+
    if chars[i] == '"' || chars[i] == '\'' {
        let quote = chars[i];
        i += 1;
        while i < n && chars[i] != quote {
            i += 1;
        }
        if i >= n {
            return None;
        }
        return Some(i + 1);
    }
    let vs = i;
    while i < n && !is_ascii_ws(chars[i]) && chars[i] != '>' {
        i += 1;
    }
    if i > vs {
        Some(i)
    } else {
        None
    }
}

/// Percent-encode like JavaScript `encodeURIComponent`: it leaves
/// `A-Za-z0-9-_.!~*'()` unencoded and encodes a space as `%20`. Encodes UTF-8
/// bytes with uppercase hex.
fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            );
        if keep {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(hex_upper(byte >> 4));
            out.push(hex_upper(byte & 0xF));
        }
    }
    out
}

fn hex_upper(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + (nibble - 10)) as char,
    }
}

/// ASCII whitespace, matching how the carve-js rendered attr strings are shaped
/// (single ASCII spaces). Mirrors the `\s` in the merge regexes for that domain.
fn is_ascii_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C' | '\x0B')
}

fn memchr(b: &[u8], needle: u8, from: usize) -> Option<usize> {
    b[from..]
        .iter()
        .position(|&x| x == needle)
        .map(|p| from + p)
}
