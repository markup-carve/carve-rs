//! HTML escaping helpers. Behavior matches `carve-js`/`render-html.ts`:
//! text content escapes `&`, `<`, `>`; attribute values additionally
//! escape `"`.

/// Replacement entity for a text-escaped byte, or `None` if it passes through.
#[inline]
fn text_entity(byte: u8) -> Option<&'static str> {
    match byte {
        b'&' => Some("&amp;"),
        b'<' => Some("&lt;"),
        b'>' => Some("&gt;"),
        _ => None,
    }
}

/// Replacement entity for an attribute-escaped byte, or `None` if it passes
/// through. Adds `"`/`'` on top of the text set.
#[inline]
fn attr_entity(byte: u8) -> Option<&'static str> {
    match byte {
        b'&' => Some("&amp;"),
        b'<' => Some("&lt;"),
        b'>' => Some("&gt;"),
        b'"' => Some("&quot;"),
        b'\'' => Some("&apos;"),
        _ => None,
    }
}

/// Write `input` into `out`, escaping text-content characters (`&`, `<`, `>`).
/// All escaped characters are ASCII, so byte scanning is UTF-8 safe (multibyte
/// sequences only contain bytes `>= 0x80`). Copies runs between escapes in a
/// single `push_str` instead of char-by-char.
pub fn write_escaped_text(out: &mut String, input: &str) {
    let bytes = input.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if let Some(entity) = text_entity(b) {
            out.push_str(&input[start..i]);
            out.push_str(entity);
            start = i + 1;
        }
    }
    out.push_str(&input[start..]);
}

/// Write `input` into `out`, escaping attribute-value characters
/// (`&`, `<`, `>`, `"`, `'`).
pub fn write_escaped_attr(out: &mut String, input: &str) {
    let bytes = input.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if let Some(entity) = attr_entity(b) {
            out.push_str(&input[start..i]);
            out.push_str(entity);
            start = i + 1;
        }
    }
    out.push_str(&input[start..]);
}

pub fn escape_text(input: &str) -> String {
    // Fast path: nothing to escape, avoid an allocation copy where possible.
    if !input.bytes().any(|b| text_entity(b).is_some()) {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 8);
    write_escaped_text(&mut out, input);
    out
}

pub fn escape_attr(input: &str) -> String {
    if !input.bytes().any(|b| attr_entity(b).is_some()) {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 8);
    write_escaped_attr(&mut out, input);
    out
}

/// URL schemes that must never appear in an attribute value.
const DANGEROUS_VALUE_SCHEMES: [&str; 4] = ["javascript", "vbscript", "data", "file"];

/// Whether an attribute NAME is unsafe regardless of value: event handlers
/// (`on*`) and the injection sinks `srcdoc` / `formaction`. Such attributes are
/// dropped from all rendered output; there is no legitimate use in a
/// content-markup document.
pub fn is_dangerous_attr_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("on") || lower == "srcdoc" || lower == "formaction"
}

/// Blank an attribute value carrying a dangerous URL scheme or a CSS
/// `expression(...)`, so an author cannot smuggle script through an attribute
/// the name filter allows (e.g. `background`, `style`). The scheme is
/// normalized (C0 controls + spaces removed) before comparison to defeat
/// `java\tscript:` style evasion.
pub fn sanitize_attr_value<'a>(name: &str, value: &'a str) -> std::borrow::Cow<'a, str> {
    if let Some(colon) = value.find(':') {
        let scheme: String = value[..colon]
            .chars()
            .filter(|c| (*c as u32) > 0x20)
            .collect::<String>()
            .to_ascii_lowercase();
        if DANGEROUS_VALUE_SCHEMES.contains(&scheme.as_str()) {
            return std::borrow::Cow::Borrowed("");
        }
    }
    if name.eq_ignore_ascii_case("style") && has_dangerous_css(value) {
        return std::borrow::Cow::Borrowed("");
    }
    std::borrow::Cow::Borrowed(value)
}

/// Detect script-bearing / fetching constructs in a CSS `style` value:
/// `expression()` (legacy IE script), `url(...)` (can fetch or carry
/// `javascript:`), `@import`, and the legacy `behavior` / `-moz-binding`
/// script bindings. Whitespace is collapsed first so `expr ession (` cannot
/// evade. Blanks the whole value rather than attempting CSS surgery.
fn has_dangerous_css(value: &str) -> bool {
    let compact: String = value
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    compact.contains("expression(")
        || compact.contains("url(")
        || compact.contains("@import")
        || compact.contains("behavior:")
        || compact.contains("-moz-binding")
}

/// Always-on URL hardening for `href` / `src`: blank a URL whose (normalized)
/// scheme is on the dangerous denylist (`javascript`, `vbscript`, `data`,
/// `file`); every other scheme and any scheme-less URL passes. Scheme detection
/// strips C0 controls + spaces to defeat `java\tscript:` evasion. The returned
/// value is still passed through `escape_attr` by the caller.
pub fn sanitize_url(url: &str) -> std::borrow::Cow<'_, str> {
    let probe: String = url.chars().filter(|c| (*c as u32) > 0x20).collect();
    if let Some(colon) = probe.find(':') {
        // A scheme is letters/digits/+/-/. before the colon; if the prefix
        // contains anything else it is not a URL scheme (e.g. a path segment).
        let prefix = &probe[..colon];
        let is_scheme = !prefix.is_empty()
            && prefix
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
        if is_scheme && DANGEROUS_VALUE_SCHEMES.contains(&prefix.to_ascii_lowercase().as_str()) {
            return std::borrow::Cow::Borrowed("");
        }
    }
    std::borrow::Cow::Borrowed(url)
}
