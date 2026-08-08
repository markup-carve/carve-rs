//! Hand-rolled SVG sanitizer (Tier-3, zero-dependency). Powers the `img` fence
//! (see [`crate::extensions::img_fence`]); usable standalone.
//!
//! A real tokenizer, NOT a regex scrub - regex "sanitizers" for SVG are
//! routinely bypassed. It walks the source tag by tag, drops any element not on
//! a presentational allowlist **together with its subtree**, drops any attribute
//! not on the allowlist (and every `on*` handler), scrubs URL/style values, and
//! re-serializes only the survivors. Text nodes pass through with `&<>`
//! re-escaped. Anything unrecognized is dropped, never echoed.
//!
//! The output is guaranteed to contain no `<script>`, no event handlers, no
//! `<foreignObject>`, no `javascript:`/external URLs, and no active CSS - so it
//! is safe to inline into the DOM or to encode into a `data:image/svg+xml` URI.
//!
//! Faithful port of carve-js `src/svg-sanitize.ts`.

use std::collections::HashSet;

use crate::escape::DANGEROUS_VALUE_SCHEMES;

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Presentational SVG element allowlist. Deliberately excludes script,
/// foreignObject, style, a, image, metadata, and SMIL - those are gated by an
/// option or dropped outright.
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

/// Elements permitted only when the matching option is set.
const LINK_TAGS: &[&str] = &["a"];
const ANIMATION_TAGS: &[&str] = &[
    "animate",
    "animateTransform",
    "animateMotion",
    "set",
    "mpath",
];
const EXTERNAL_IMAGE_TAGS: &[&str] = &["image"];

/// Attribute-name allowlist (case-insensitive). Geometry + presentation only.
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

/// Reference-carrying attrs get URL scrubbing rather than a value passthrough.
const URL_ATTRS: &[&str] = &["href", "xlink:href"];

/// Attributes whose value is a paint/filter/animation REFERENCE. These may only
/// carry local `#id` refs or literals - never a non-local `url()` or any
/// absolute URL. SMIL value lists (`values`, `from`, `to`, `by`) are validated
/// per `;`-separated segment so a later entry cannot smuggle a remote target.
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

/// Options gate the small set of constructs that are safe only in some
/// contexts. All default OFF.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SanitizeSvgOptions {
    /// Keep the `style` **attribute** (value scrubbed of `url()`/`expression()`/…).
    /// The `<style>` *element* is always dropped regardless.
    pub allow_style: bool,
    /// Keep `<a>` elements and external `href`/`xlink:href` (safe schemes only).
    pub allow_links: bool,
    /// Keep SMIL animation elements (`<animate>`, `<set>`, …).
    pub allow_animation: bool,
    /// Keep `<image>` and its external raster `href` (safe schemes only; note
    /// `data:` is still rejected as a dangerous scheme).
    pub allow_external_images: bool,
}

/// Result of [`sanitize_svg`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizeResult {
    /// The sanitized SVG. Meaningful only when [`SanitizeResult::ok`] is true.
    pub svg: String,
    /// True when the input parsed to a single well-formed `<svg>` root. When
    /// false, callers should fall back to showing the source, never the raw
    /// input.
    pub ok: bool,
}

impl SanitizeResult {
    fn rejected() -> Self {
        Self {
            svg: String::new(),
            ok: false,
        }
    }
}

// --------------------------------------------------------------------------
// Character classes (ported from the carve-js regex character sets)
// --------------------------------------------------------------------------

/// The JS `\s` character class: horizontal/vertical whitespace plus the Unicode
/// space separators and the BOM. Used wherever the carve-js regexes use `\s`.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    ) || ('\u{2000}'..='\u{200A}').contains(&c)
}

/// The carve-js `SCHEME_PROBE_STRIP_RE` character class: every control
/// character - `char::is_control`, which is Cc exactly, so U+0000..U+001F plus
/// DEL and the C1 block - and every ASCII space, plus the Unicode separators
/// and the BOM. Stripped before a URL scheme probe so `java\tscript:`,
/// `java<DEL>script:` and NBSP-obfuscated schemes are all still detected.
///
/// THIS IS THE SECOND SPELLING of `escape::is_url_probe_skippable` and it has
/// to stay as wide. It is a PROBE class, not an emit class: PART 9 §29 T5 puts
/// DEL and C1 outside what a target may emit, and reading this class off that
/// one is what let a split scheme through `href` / `xlink:href` and defeated
/// `has_absolute_scheme`'s reject-every-absolute-scheme rule outright
/// (markup-carve/carve-rs#833). Filtering only removes characters, so a wider
/// class refuses more and can never permit more.
fn is_scheme_strip(c: char) -> bool {
    c.is_control()
        || (c as u32) <= 0x20
        || matches!(
            c,
            '\u{00A0}'
                | '\u{1680}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
        )
        || ('\u{2000}'..='\u{200A}').contains(&c)
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':' || c == '-'
}

// --------------------------------------------------------------------------
// URL / scheme / reference checks
// --------------------------------------------------------------------------

fn strip_scheme_ws(value: &str) -> String {
    value.chars().filter(|c| !is_scheme_strip(*c)).collect()
}

/// The leading `[a-zA-Z][a-zA-Z0-9+.-]*` scheme of `probe`, if it is immediately
/// followed by a `:` (mirrors `^([a-zA-Z][a-zA-Z0-9+.-]*):`).
fn leading_scheme(probe: &str) -> Option<&str> {
    let b = probe.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphabetic() {
        return None;
    }
    let mut i = 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'+' | b'.' | b'-')) {
        i += 1;
    }
    if i < b.len() && b[i] == b':' {
        Some(&probe[..i])
    } else {
        None
    }
}

/// Any absolute-URL scheme (`https:`, `ms-msdt:`, …) - mirrors `ABSOLUTE_SCHEME_RE`.
fn has_absolute_scheme(probe: &str) -> bool {
    leading_scheme(probe).is_some()
}

fn scheme_is_safe(url: &str) -> bool {
    let probe = strip_scheme_ws(url);
    match leading_scheme(&probe) {
        None => true, // relative / fragment - safe
        Some(scheme) => !DANGEROUS_VALUE_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()),
    }
}

/// Decode CSS escapes (`\72` -> `r`, `\/` -> `/`) so an escaped `url(` /
/// `expression(` cannot slip past the needle checks. Mirrors `decodeCssEscapes`.
fn decode_css_escapes(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        if chars[i] == '\\' && i + 1 < n {
            if chars[i + 1].is_ascii_hexdigit() {
                // [0-9a-f]{1,6}\s?
                let mut hex = String::new();
                let mut k = i + 1;
                while k < n && hex.len() < 6 && chars[k].is_ascii_hexdigit() {
                    hex.push(chars[k]);
                    k += 1;
                }
                if k < n && is_js_space(chars[k]) {
                    k += 1;
                }
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if cp <= 0x10FFFF {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                        // surrogate -> dropped (JS emits a lone surrogate; not
                        // representable in Rust and validation-only anyway)
                    }
                    // cp > 0x10FFFF -> '' (matches the JS guard)
                }
                i = k;
                continue;
            }
            // [\s\S] single char: emit it literally (`\/` -> `/`, `\\` -> `\`)
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Named character references that can obfuscate a URL scheme.
fn named_ref(name: &str) -> Option<char> {
    match name {
        "colon" => Some(':'),
        "semi" => Some(';'),
        "sol" => Some('/'),
        "tab" => Some('\t'),
        "newline" => Some('\n'),
        "lpar" => Some('('),
        "rpar" => Some(')'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => None,
    }
}

/// Decode XML/HTML character references (numeric `&#x61;`/`&#97;` + the named
/// set) so an entity-encoded scheme is normalized before a URL/scheme check.
/// Used ONLY for validation, never for output. Mirrors `decodeEntities`.
fn decode_entities(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        if chars[i] == '&' {
            if let Some((decoded, consumed)) = try_entity(&chars, i) {
                out.push(decoded);
                i += consumed;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Try to decode a `&…;` reference starting at `chars[i] == '&'`. Returns the
/// decoded char and the number of source chars consumed, or `None` (the `&`
/// stays literal - matching the regex leaving an unrecognized reference alone).
fn try_entity(chars: &[char], i: usize) -> Option<(char, usize)> {
    let n = chars.len();
    let j = i + 1;
    if j >= n {
        return None;
    }
    if chars[j] == '#' {
        // Numeric: #\d+; (decimal) or #x[0-9a-f]+; (hex, case-insensitive).
        let (radix, mut k) = if j + 1 < n && (chars[j + 1] == 'x' || chars[j + 1] == 'X') {
            (16u32, j + 2)
        } else {
            (10u32, j + 1)
        };
        let start = k;
        while k < n
            && (if radix == 16 {
                chars[k].is_ascii_hexdigit()
            } else {
                chars[k].is_ascii_digit()
            })
        {
            k += 1;
        }
        if k == start || k >= n || chars[k] != ';' {
            return None;
        }
        let digits: String = chars[start..k].iter().collect();
        let cp = u32::from_str_radix(&digits, radix).ok()?;
        if cp <= 0x10FFFF {
            if let Some(ch) = char::from_u32(cp) {
                return Some((ch, k + 1 - i));
            }
        }
        // out of range or surrogate -> leave the reference literal
        return None;
    }
    // Named: [a-z][a-z0-9]* (case-insensitive).
    if !chars[j].is_ascii_alphabetic() {
        return None;
    }
    let mut k = j + 1;
    while k < n && chars[k].is_ascii_alphanumeric() {
        k += 1;
    }
    if k >= n || chars[k] != ';' {
        return None;
    }
    let name: String = chars[j..k].iter().map(|c| c.to_ascii_lowercase()).collect();
    named_ref(&name).map(|c| (c, k + 1 - i))
}

/// Full normalization for any URL/reference/style check: undo both entity and
/// CSS-escape obfuscation.
fn normalize_for_check(value: &str) -> String {
    decode_css_escapes(&decode_entities(value))
}

/// A `url(...)` whose content does not begin with `#` is a NON-LOCAL reference.
/// Ports `NONLOCAL_URL_RE` = `url\(\s*['"]?\s*(?!#)` (case-insensitive, `.test`).
fn has_nonlocal_url(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i + 4 <= n {
        if chars[i].eq_ignore_ascii_case(&'u')
            && chars[i + 1].eq_ignore_ascii_case(&'r')
            && chars[i + 2].eq_ignore_ascii_case(&'l')
            && chars[i + 3] == '('
        {
            let mut j = i + 4;
            while j < n && is_js_space(chars[j]) {
                j += 1;
            }
            if j < n && (chars[j] == '\'' || chars[j] == '"') {
                j += 1;
            }
            while j < n && is_js_space(chars[j]) {
                j += 1;
            }
            // negative lookahead (?!#): satisfied at end-of-string or a non-`#`
            if j >= n || chars[j] != '#' {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn ref_attr_unsafe(value: &str) -> bool {
    let decoded = normalize_for_check(value);
    for seg in decoded.split(';') {
        let s = seg.trim();
        if has_nonlocal_url(s) {
            return true;
        }
        let probe = strip_scheme_ws(s);
        if has_absolute_scheme(&probe) {
            return true;
        }
        // A leading `/` is a path reference (`//host/x` protocol-relative or
        // `/abs/path`) - both fetch remotely.
        if probe.starts_with('/') {
            return true;
        }
    }
    false
}

fn value_has_external_ref(value: &str) -> bool {
    let decoded = normalize_for_check(value);
    if has_nonlocal_url(&decoded) {
        return true;
    }
    !scheme_is_safe(&decoded)
}

/// Blank a style value that can fetch or execute. Whole-value rejection, not CSS
/// surgery. CSS escapes are decoded first so `u\72l(` folds to `url(`.
fn style_is_dangerous(value: &str) -> bool {
    let without_comments = strip_css_comments(value);
    let compact: String = normalize_for_check(&without_comments)
        .to_lowercase()
        .chars()
        .filter(|c| !is_js_space(*c))
        .collect();
    compact.contains("expression(")
        || compact.contains("url(")
        || compact.contains("@import")
        || compact.contains("behavior:")
        || compact.contains("-moz-binding")
        || compact.contains("javascript:")
}

/// Remove `/* … */` CSS comments (lazy, like `/\/\*[\s\S]*?\*\//g`).
fn strip_css_comments(value: &str) -> String {
    let bytes = value.as_bytes();
    let n = bytes.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        if bytes[i] == b'/' && i + 1 < n && bytes[i + 1] == b'*' {
            match find_bytes(bytes, b"*/", i + 2) {
                Some(end) => {
                    i = end + 2;
                    continue;
                }
                None => break, // unterminated comment: drop the rest
            }
        }
        // copy the char at i
        let c = value[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

// --------------------------------------------------------------------------
// Output escaping
// --------------------------------------------------------------------------

/// Escape a bare `&` but leave intact the entities valid in an XML document: the
/// five predefined names and numeric refs (`&#38;`, `&#x26;`). Mirrors
/// `escapeAmp` = `&(?!#\d+;|#x[0-9a-fA-F]+;|(?:amp|lt|gt|quot|apos);)`.
fn escape_amp(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' {
            out.push_str(&s[start..i]);
            if amp_is_entity(b, i + 1) {
                out.push('&');
            } else {
                out.push_str("&amp;");
            }
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&s[start..]);
    out
}

/// Whether the text at `b[j..]` is one of the entity forms the `escapeAmp`
/// lookahead preserves. Note: the hex form requires a lowercase `x` (XML rule),
/// unlike `decode_entities`, whose regex carries the `i` flag.
fn amp_is_entity(b: &[u8], j: usize) -> bool {
    let n = b.len();
    if j >= n {
        return false;
    }
    if b[j] == b'#' {
        if j + 1 < n && b[j + 1] == b'x' {
            let mut k = j + 2;
            let start = k;
            while k < n && b[k].is_ascii_hexdigit() {
                k += 1;
            }
            return k > start && k < n && b[k] == b';';
        }
        let mut k = j + 1;
        let start = k;
        while k < n && b[k].is_ascii_digit() {
            k += 1;
        }
        return k > start && k < n && b[k] == b';';
    }
    for name in ["amp", "lt", "gt", "quot", "apos"] {
        let nb = name.as_bytes();
        if j + nb.len() < n && &b[j..j + nb.len()] == nb && b[j + nb.len()] == b';' {
            return true;
        }
    }
    false
}

fn escape_text(s: &str) -> String {
    escape_amp(s).replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    escape_amp(s)
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// --------------------------------------------------------------------------
// Allowlist checks
// --------------------------------------------------------------------------

fn tag_allowed(name: &str, opts: &SanitizeSvgOptions) -> bool {
    let n = name.to_ascii_lowercase();
    if ALLOWED_TAGS.contains(&name) || ALLOWED_TAGS.contains(&n.as_str()) {
        return true;
    }
    if opts.allow_links && LINK_TAGS.contains(&n.as_str()) {
        return true;
    }
    // The `<style>` *element* is never allowed - its text can carry
    // `@import`/`url()` that no attribute scrub would catch.
    if opts.allow_animation && ANIMATION_TAGS.contains(&n.as_str()) {
        return true;
    }
    if opts.allow_external_images && EXTERNAL_IMAGE_TAGS.contains(&n.as_str()) {
        return true;
    }
    false
}

// --------------------------------------------------------------------------
// Attribute parsing + sanitizing
// --------------------------------------------------------------------------

/// Parse an attribute list. Mirrors `ATTR_RE`:
/// `([A-Za-z_:][\w:.-]*)(?:\s*=\s*("([^"]*)"|'([^']*)'|[^\s"'>]+))?`, scanned
/// globally (unrecognized text between names is skipped).
fn parse_attrs(raw: &str) -> Vec<(String, Option<String>)> {
    let chars: Vec<char> = raw.chars().collect();
    let n = chars.len();
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut i = 0;
    while i < n {
        if !is_name_start(chars[i]) {
            i += 1;
            continue;
        }
        let name_start = i;
        i += 1;
        while i < n && is_name_char(chars[i]) {
            i += 1;
        }
        let name: String = chars[name_start..i].iter().collect();
        // Optional (?: \s*=\s* value ). On any failure the position stays at the
        // end of the name (the `=`/whitespace was only tentatively consumed).
        let mut j = i;
        while j < n && is_js_space(chars[j]) {
            j += 1;
        }
        if j >= n || chars[j] != '=' {
            out.push((name, None));
            continue;
        }
        j += 1;
        while j < n && is_js_space(chars[j]) {
            j += 1;
        }
        if j < n && chars[j] == '"' {
            let vs = j + 1;
            let mut k = vs;
            while k < n && chars[k] != '"' {
                k += 1;
            }
            if k < n {
                out.push((name, Some(chars[vs..k].iter().collect())));
                i = k + 1;
            } else {
                out.push((name, None)); // unterminated quote: value group fails
            }
        } else if j < n && chars[j] == '\'' {
            let vs = j + 1;
            let mut k = vs;
            while k < n && chars[k] != '\'' {
                k += 1;
            }
            if k < n {
                out.push((name, Some(chars[vs..k].iter().collect())));
                i = k + 1;
            } else {
                out.push((name, None));
            }
        } else {
            // Unquoted: [^\s"'>]+
            let vs = j;
            let mut k = vs;
            while k < n
                && !is_js_space(chars[k])
                && chars[k] != '"'
                && chars[k] != '\''
                && chars[k] != '>'
            {
                k += 1;
            }
            if k > vs {
                out.push((name, Some(chars[vs..k].iter().collect())));
                i = k;
            } else {
                out.push((name, None));
            }
        }
    }
    out
}

fn sanitize_attrs(raw: &str, opts: &SanitizeSvgOptions, tag: &str) -> String {
    let t = tag.to_ascii_lowercase();
    let allow_external_href =
        (opts.allow_links && t == "a") || (opts.allow_external_images && t == "image");
    let mut out = String::new();
    // Duplicate attributes are not well-formed XML, so keep only the first
    // occurrence of each name (exact-case, mirroring the JS `seen` Set).
    let mut seen: HashSet<String> = HashSet::new();
    for (name, value) in parse_attrs(raw) {
        if seen.contains(&name) {
            continue;
        }
        seen.insert(name.clone());
        let n = name.to_ascii_lowercase();
        if n.starts_with("on") {
            continue; // every event handler, always
        }
        if URL_ATTRS.contains(&n.as_str()) {
            let Some(value) = value else { continue };
            let decoded = normalize_for_check(&value);
            let local = decoded.starts_with('#');
            if !local && !allow_external_href {
                continue;
            }
            if !scheme_is_safe(&decoded) {
                continue;
            }
            out.push(' ');
            out.push_str(&name);
            out.push_str("=\"");
            out.push_str(&escape_attr(&value));
            out.push('"');
            continue;
        }
        if n == "style" {
            if !opts.allow_style {
                continue;
            }
            let Some(value) = value else { continue };
            if style_is_dangerous(&value) {
                continue;
            }
            out.push(' ');
            out.push_str(&name);
            out.push_str("=\"");
            out.push_str(&escape_attr(&value));
            out.push('"');
            continue;
        }
        if n.starts_with("aria-") || n.starts_with("data-") || ALLOWED_ATTRS.contains(&n.as_str()) {
            if let Some(ref v) = value {
                let unsafe_ref = if REF_VALUE_ATTRS.contains(&n.as_str()) {
                    ref_attr_unsafe(v)
                } else {
                    value_has_external_ref(v)
                };
                if unsafe_ref {
                    continue;
                }
            }
            match value {
                None => {
                    out.push(' ');
                    out.push_str(&name);
                }
                Some(v) => {
                    out.push(' ');
                    out.push_str(&name);
                    out.push_str("=\"");
                    out.push_str(&escape_attr(&v));
                    out.push('"');
                }
            }
        }
    }
    out
}

/// Remove the bare `xmlns=…` declarations from a built attr string (mirrors
/// `\s+xmlns\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)` replaced globally). `xmlns:xlink`
/// is untouched (the char after `xmlns` is `:`, not `\s*=`).
fn remove_bare_xmlns(attrs: &str) -> String {
    let chars: Vec<char> = attrs.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        if let Some(end) = xmlns_match(&chars, i) {
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn xmlns_match(chars: &[char], start: usize) -> Option<usize> {
    let n = chars.len();
    let mut i = start;
    if i >= n || !is_js_space(chars[i]) {
        return None; // \s+ needs at least one whitespace
    }
    while i < n && is_js_space(chars[i]) {
        i += 1;
    }
    for w in ['x', 'm', 'l', 'n', 's'] {
        if i >= n || chars[i].to_ascii_lowercase() != w {
            return None;
        }
        i += 1;
    }
    while i < n && is_js_space(chars[i]) {
        i += 1;
    }
    if i >= n || chars[i] != '=' {
        return None;
    }
    i += 1;
    while i < n && is_js_space(chars[i]) {
        i += 1;
    }
    if i >= n {
        return None;
    }
    if chars[i] == '"' || chars[i] == '\'' {
        let quote = chars[i];
        i += 1;
        while i < n && chars[i] != quote {
            i += 1;
        }
        if i >= n {
            return None; // unterminated quoted value
        }
        return Some(i + 1);
    }
    // [^\s>]+
    let vs = i;
    while i < n && !is_js_space(chars[i]) && chars[i] != '>' {
        i += 1;
    }
    if i > vs {
        Some(i)
    } else {
        None
    }
}

// --------------------------------------------------------------------------
// Tokenizer
// --------------------------------------------------------------------------

enum TokKind {
    Close(String),
    Ignorable,
    Open {
        name: String,
        attrs: String,
        self_close: bool,
    },
}

struct Tok {
    start: usize,
    end: usize,
    kind: TokKind,
}

fn find_bytes(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

/// Find the next tokenizer match at or after `from`, mirroring the ordered
/// alternation of carve-js `TOKEN_RE` scanned with `exec`.
fn next_token(src: &str, from: usize) -> Option<Tok> {
    let b = src.as_bytes();
    let len = b.len();
    let mut i = from;
    while i < len {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &src[i..];
        // 1. comment
        if rest.starts_with("<!--") {
            if let Some(p) = find_bytes(b, b"-->", i + 4) {
                return Some(Tok {
                    start: i,
                    end: p + 3,
                    kind: TokKind::Ignorable,
                });
            }
            i += 1;
            continue;
        }
        // 2. CDATA
        if rest.starts_with("<![CDATA[") {
            if let Some(p) = find_bytes(b, b"]]>", i + 9) {
                return Some(Tok {
                    start: i,
                    end: p + 3,
                    kind: TokKind::Ignorable,
                });
            }
            i += 1;
            continue;
        }
        // 3. DOCTYPE (case-sensitive DOCTYPE|doctype)
        if rest.starts_with("<!DOCTYPE") || rest.starts_with("<!doctype") {
            if let Some(p) = find_bytes(b, b">", i + 9) {
                return Some(Tok {
                    start: i,
                    end: p + 1,
                    kind: TokKind::Ignorable,
                });
            }
            i += 1;
            continue;
        }
        // 4. PI
        if rest.starts_with("<?") {
            if let Some(p) = find_bytes(b, b"?>", i + 2) {
                return Some(Tok {
                    start: i,
                    end: p + 2,
                    kind: TokKind::Ignorable,
                });
            }
            i += 1;
            continue;
        }
        // 5. close tag </name\s*>
        if rest.starts_with("</") {
            if let Some((name, after)) = parse_tag_name(b, i + 2) {
                let j = skip_js_space(src, after);
                if j < len && b[j] == b'>' {
                    return Some(Tok {
                        start: i,
                        end: j + 1,
                        kind: TokKind::Close(name),
                    });
                }
            }
            i += 1;
            continue;
        }
        // 6. open tag <name attrs (/?)>
        if let Some((name, name_end)) = parse_tag_name(b, i + 1) {
            if let Some((attrs, self_close, end)) = scan_open_tag(src, name_end) {
                return Some(Tok {
                    start: i,
                    end,
                    kind: TokKind::Open {
                        name,
                        attrs,
                        self_close,
                    },
                });
            }
        }
        i += 1;
    }
    None
}

/// Parse `[A-Za-z][\w:.-]*` at `start`, returning `(name, end_index)`. Names are
/// ASCII, so byte scanning is safe.
fn parse_tag_name(b: &[u8], start: usize) -> Option<(String, usize)> {
    if start >= b.len() || !b[start].is_ascii_alphabetic() {
        return None;
    }
    let mut i = start + 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b':' | b'.' | b'-'))
    {
        i += 1;
    }
    // SAFETY: all bytes consumed are ASCII, so this is a valid str slice.
    Some((String::from_utf8_lossy(&b[start..i]).into_owned(), i))
}

/// Scan the attribute region of an open tag from `name_end` (mirrors the lazy
/// `((?:"[^"]*"|'[^']*'|[^>"'])*?)(\/?)>`). Returns `(attrs, self_close, end)` or
/// `None` if no tag end is found (e.g. an unterminated quote).
fn scan_open_tag(src: &str, name_end: usize) -> Option<(String, bool, usize)> {
    let b = src.as_bytes();
    let len = b.len();
    let mut j = name_end;
    loop {
        if j >= len {
            return None;
        }
        match b[j] {
            b'/' if j + 1 < len && b[j + 1] == b'>' => {
                return Some((src[name_end..j].to_string(), true, j + 2));
            }
            b'>' => {
                return Some((src[name_end..j].to_string(), false, j + 1));
            }
            b'"' => j = find_bytes(b, b"\"", j + 1)? + 1,
            b'\'' => j = find_bytes(b, b"'", j + 1)? + 1,
            _ => {
                let c = src[j..].chars().next().unwrap();
                j += c.len_utf8();
            }
        }
    }
}

fn skip_js_space(src: &str, mut i: usize) -> usize {
    while i < src.len() {
        let c = src[i..].chars().next().unwrap();
        if is_js_space(c) {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    i
}

// --------------------------------------------------------------------------
// Well-formedness tail checks
// --------------------------------------------------------------------------

fn trim_end_js_space(s: &str) -> &str {
    let mut end = s.len();
    while end > 0 {
        let c = s[..end].chars().next_back().unwrap();
        if is_js_space(c) {
            end -= c.len_utf8();
        } else {
            break;
        }
    }
    &s[..end]
}

fn head_ok(out: &str) -> bool {
    // ^<svg[\s/>] (case-insensitive)
    if out.len() < 4 || !out[..4].eq_ignore_ascii_case("<svg") {
        return false;
    }
    match out[4..].chars().next() {
        Some(c) => is_js_space(c) || c == '/' || c == '>',
        None => false,
    }
}

fn tail_ok(out: &str, self_closed: bool) -> bool {
    let trimmed = trim_end_js_space(out);
    if self_closed {
        trimmed.ends_with("/>")
    } else {
        // </svg>\s*$ (case-insensitive)
        let n = trimmed.len();
        n >= 6 && trimmed[n - 6..].eq_ignore_ascii_case("</svg>")
    }
}

// --------------------------------------------------------------------------
// Public entry point
// --------------------------------------------------------------------------

/// Sanitize an SVG source string. See the module docs for the guarantees.
pub fn sanitize_svg(source: &str, opts: &SanitizeSvgOptions) -> SanitizeResult {
    let src = source.trim();
    let mut out = String::new();
    let mut last_index = 0usize;
    let mut drop_stack: Vec<String> = Vec::new(); // dropped open elements (subtree discarded)
    let mut kept: Vec<String> = Vec::new(); // kept open elements (matched on close)
    let mut saw_svg_root = false;
    let mut root_self_closed = false;
    let mut search = 0usize;

    while let Some(tok) = next_token(src, search) {
        // Text between the previous token and this one.
        if tok.start > last_index {
            let between = &src[last_index..tok.start];
            if !saw_svg_root {
                if !between.trim().is_empty() {
                    return SanitizeResult::rejected();
                }
            } else if drop_stack.is_empty() {
                out.push_str(&escape_text(between));
            }
        }
        last_index = tok.end;
        search = tok.end;

        match tok.kind {
            TokKind::Close(end_name) => {
                if !drop_stack.is_empty() {
                    let d = drop_stack.pop().unwrap();
                    if d != end_name {
                        return SanitizeResult::rejected();
                    }
                } else {
                    match kept.pop() {
                        Some(open) if open == end_name => {
                            out.push_str("</");
                            out.push_str(&end_name);
                            out.push('>');
                        }
                        _ => return SanitizeResult::rejected(),
                    }
                }
            }
            TokKind::Ignorable => {}
            TokKind::Open {
                name,
                attrs,
                self_close,
            } => {
                let allowed = tag_allowed(&name, opts);
                let is_root = kept.is_empty() && drop_stack.is_empty() && !saw_svg_root;

                if is_root && name != "svg" {
                    return SanitizeResult::rejected();
                }
                if !is_root && kept.is_empty() && drop_stack.is_empty() && saw_svg_root {
                    return SanitizeResult::rejected();
                }

                if !drop_stack.is_empty() || !allowed {
                    // Either already discarding a subtree, or this element is not
                    // allowed: track nesting by name so only the matching close
                    // exits it (self-closing tags open nothing).
                    if !self_close {
                        drop_stack.push(name);
                    }
                } else {
                    if is_root {
                        saw_svg_root = true;
                        root_self_closed = self_close;
                    }
                    let mut a = sanitize_attrs(&attrs, opts, &name);
                    if is_root {
                        // Force the canonical SVG namespace on the root: drop any
                        // author `xmlns` and inject ours. `xmlns:xlink` is kept.
                        let stripped = remove_bare_xmlns(&a);
                        a = format!(" xmlns=\"{SVG_NS}\"{stripped}");
                    }
                    out.push('<');
                    out.push_str(&name);
                    out.push_str(&a);
                    out.push_str(if self_close { "/>" } else { ">" });
                    if !self_close {
                        kept.push(name);
                    }
                }
            }
        }
    }

    // Trailing text.
    if last_index < src.len() && saw_svg_root && drop_stack.is_empty() {
        out.push_str(&escape_text(&src[last_index..]));
    }

    // Well-formedness: a single closed <svg> root, balanced, nothing left open.
    if !saw_svg_root || !kept.is_empty() || !drop_stack.is_empty() {
        return SanitizeResult::rejected();
    }
    if !head_ok(&out) || !tail_ok(&out, root_self_closed) {
        return SanitizeResult::rejected();
    }
    SanitizeResult { svg: out, ok: true }
}
