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

/// Trojan-Source bidi-override / isolate controls that must be REMOVED (not
/// entity-escaped) from rendered TEXT and CODE: U+202A..U+202E (LRE/RLE/PDF/
/// LRO/RLO) and U+2066..U+2069 (LRI/RLI/FSI/PDI). An entity reference would
/// decode back to the raw control and still reorder the live DOM, so only
/// physical removal is DOM-inert. The directional MARKS U+200E / U+200F
/// (LRM / RLM) and the zero-width characters are deliberately NOT stripped from
/// text (they are only stripped from generated ids; see `slugify_parse`).
/// Matches carve-js `stripBidiControls` / carve-php.
pub fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Whether `input` contains any bidi-override / isolate control (fast pre-check
/// so the common path allocates nothing). All such controls are 3-byte UTF-8
/// sequences led by `0xE2`, so a byte scan is enough to rule them out.
#[inline]
fn has_bidi_control(input: &str) -> bool {
    input.as_bytes().contains(&0xE2) && input.chars().any(is_bidi_control)
}

/// Write `input` into `out`, escaping text-content characters (`&`, `<`, `>`)
/// and first STRIPPING Trojan-Source bidi controls (see [`is_bidi_control`]).
/// All escaped characters are ASCII, so byte scanning is UTF-8 safe (multibyte
/// sequences only contain bytes `>= 0x80`). Copies runs between escapes in a
/// single `push_str` instead of char-by-char.
pub fn write_escaped_text(out: &mut String, input: &str) {
    if has_bidi_control(input) {
        let stripped: String = input.chars().filter(|c| !is_bidi_control(*c)).collect();
        write_escaped_text_inner(out, &stripped);
        return;
    }
    write_escaped_text_inner(out, input);
}

fn write_escaped_text_inner(out: &mut String, input: &str) {
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
    // Fast path: nothing to escape or strip, avoid an allocation copy.
    if !input.bytes().any(|b| text_entity(b).is_some()) && !has_bidi_control(input) {
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
///
/// Beyond the classic script-bearing schemes (`javascript`, `vbscript`,
/// `data`, `file`), this also denies OS protocol-handler / command-execution
/// schemes (CVE-2026-20841 class): clicking such a link can hand a payload to
/// a desktop application's URL handler. These are blocked everywhere a URL is
/// emitted (link href, image src, autolinks, and `{href=...}`/`{src=...}`
/// attribute overrides), case-insensitively and after the existing
/// obfuscation defenses.
pub(crate) const DANGEROUS_VALUE_SCHEMES: [&str; 23] = [
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

/// Whether an attribute NAME is unsafe regardless of value: event handlers
/// (`on*`) and the injection sinks `srcdoc` / `formaction`. Such attributes are
/// dropped from all rendered output; there is no legitimate use in a
/// content-markup document.
pub fn is_dangerous_attr_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("on") || lower == "srcdoc" || lower == "formaction"
}

/// Whether an attribute NAME has valid HTML/XML-ish syntax. Invalid names are
/// dropped before interpolation so they cannot break out of the attribute slot.
pub fn is_valid_attr_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == ':') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':' || c == '-')
}

/// The four attributes whose value is a LIST of URLs a consumer resolves or
/// fetches, with the separators that attribute's own grammar uses. PART 9 §25
/// probes these token-wise instead of at the value's head, so a dangerous
/// scheme cannot hide behind a safe one (markup-carve/carve#1320).
const URL_LIST_ATTRS: [(&str, bool); 4] = [
    // (name, comma is also a separator)
    ("srcset", true),
    ("imagesrcset", true),
    ("ping", false),
    ("attributionsrc", false),
];

/// The separators for a URL-list attribute name, or `None` when the name is
/// not one. THE NAME MATCH IS CASE-INSENSITIVE, like the `on` prefix in the
/// same clause: an attribute block may spell it in any case and the element
/// still carries the author's spelling, so matching the exact bytes would
/// leave `SRCSET` unprobed.
fn url_list_separators(name: &str) -> Option<fn(char) -> bool> {
    fn ascii_ws(c: char) -> bool {
        matches!(c, '\t' | '\n' | '\u{0C}' | '\r' | ' ')
    }
    fn ascii_ws_or_comma(c: char) -> bool {
        c == ',' || ascii_ws(c)
    }
    URL_LIST_ATTRS
        .iter()
        .find(|(candidate, _)| name.eq_ignore_ascii_case(candidate))
        .map(|(_, commas)| {
            if *commas {
                ascii_ws_or_comma as fn(char) -> bool
            } else {
                ascii_ws as fn(char) -> bool
            }
        })
}

/// THE probe PART 9 §25 applies to an attribute value: read the scheme that
/// leads it and answer whether that scheme is denylisted. The scheme is
/// normalized - every control and whitespace character removed, see
/// `is_url_probe_skippable` - before comparison, to defeat `java\tscript:` and
/// `java<DEL>script:` style evasion alike.
///
/// The token-wise rule below runs THIS SAME function per token rather than a
/// parallel path, so the two cannot drift: the rule changes WHERE the probe
/// runs, not WHAT it denies.
fn leads_with_dangerous_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    let scheme: String = value[..colon]
        .chars()
        .filter(|c| !is_url_probe_skippable(*c))
        .collect::<String>()
        .to_ascii_lowercase();
    DANGEROUS_VALUE_SCHEMES.contains(&scheme.as_str())
}

/// Blank an attribute value carrying a dangerous URL scheme or a CSS
/// `expression(...)`, so an author cannot smuggle script through an attribute
/// the name filter allows (e.g. `background`, `style`).
///
/// For the four URL-list attributes the probe runs on EVERY non-empty token
/// rather than on the value's head, and any hit blanks the WHOLE value. What
/// is blanked is the whole value and not the offending candidate: excising one
/// would make the rendered attribute differ from the author's bytes, which
/// this clause already declined to do, and it would give one value a third
/// outcome when the defect being fixed is that one value already had two.
///
/// The comma split OVER-BLANKS one shape and that is the chosen side:
/// `srcset="https://example.com/a,data:x 1x"` is ONE candidate to a consumer
/// and is blanked here anyway. Reading it exactly would mean requiring the
/// HTML candidate-list algorithm from three engines that have to agree byte
/// for byte. Do not "fix" it - the corpus pins it, along with the `ping`
/// counterpart that must NOT blank.
pub fn sanitize_attr_value<'a>(name: &str, value: &'a str) -> std::borrow::Cow<'a, str> {
    // THE TOKEN PASS IS ADDED TO THE VALUE-WIDE PROBE, NOT SWAPPED FOR IT.
    // The clause changes WHERE the probe runs, not WHAT it denies, so the head
    // probe stays and the tokens are probed as well. Swapping would DENY LESS
    // than this engine denied before the ruling, shipping a regression as a
    // security fix: `ping="java script:alert(1)"` splits into `java` and
    // `script:alert(1)`, neither of them a denied scheme, while the value-wide
    // probe's strip closes exactly the gap the whitespace split opens.
    //
    // THE CORPUS CANNOT SEE THIS. A token-only engine passes all 1141
    // documents, the ten that pin the ruling included, so the three engines
    // could diverge on a security boundary with the conformance suite green.
    // It is pinned by a test row here, matching markup-carve/carve-js#1164.
    if leads_with_dangerous_scheme(value) {
        return std::borrow::Cow::Borrowed("");
    }
    if let Some(is_separator) = url_list_separators(name) {
        // The clause says "every NON-EMPTY token", and that qualifier costs
        // nothing to honour here: an empty token holds no colon, so the probe
        // answers no for it already. Guarding on it explicitly would add a
        // condition that cannot fail, which reads like a defense and is not.
        if value.split(is_separator).any(leads_with_dangerous_scheme) {
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
    let compact = normalize_css_for_dangerous_check(value);
    compact.contains("expression(")
        || compact.contains("url(")
        || compact.contains("@import")
        || compact.contains("behavior:")
        || compact.contains("-moz-binding")
}

fn normalize_css_for_dangerous_check(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = '\0';
            for c in chars.by_ref() {
                if prev == '*' && c == '/' {
                    break;
                }
                prev = c;
            }
            continue;
        }
        if ch == '\\' {
            let mut hex = String::new();
            while hex.len() < 6 {
                let Some(next) = chars.peek().copied() else {
                    break;
                };
                if next.is_ascii_hexdigit() {
                    hex.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if !hex.is_empty() {
                if matches!(chars.peek(), Some(c) if c.is_whitespace()) {
                    chars.next();
                }
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if let Some(decoded) = char::from_u32(cp) {
                        if !decoded.is_whitespace() {
                            out.extend(decoded.to_lowercase());
                        }
                    }
                }
                continue;
            }
            if let Some(escaped) = chars.next() {
                if !escaped.is_whitespace() {
                    out.extend(escaped.to_lowercase());
                }
            }
            continue;
        }
        if !ch.is_whitespace() {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// Always-on URL hardening for `href` / `src`: blank a URL whose (normalized)
/// scheme is on the dangerous denylist (`javascript`, `vbscript`, `data`,
/// `file`); every other scheme and any scheme-less URL passes. Scheme detection
/// drops every control and whitespace character - see
/// `is_url_probe_skippable` - to defeat `java\tscript:` and `java<DEL>script:`
/// evasion alike. STRIP-THEN-PROBE: the stripped form is only a judgement aid,
/// and a URL that passes is returned with its original bytes. The returned
/// value is still passed through `escape_attr` by the caller.
pub fn sanitize_url(url: &str) -> std::borrow::Cow<'_, str> {
    let probe: String = url
        .chars()
        .filter(|c| !is_url_probe_skippable(*c))
        .collect();
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

/// Characters dropped before probing a URL's scheme: every control character
/// and every whitespace character, plus the zero-width no-break space / BOM.
pub(crate) fn is_url_probe_skippable(c: char) -> bool {
    c.is_control() || c.is_whitespace() || c == '\u{FEFF}'
}

#[cfg(test)]
mod tests {
    use super::{escape_text, is_bidi_control, sanitize_attr_value, sanitize_url};

    #[test]
    fn bidi_overrides_and_isolates_are_controls() {
        for c in ['\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}'] {
            assert!(is_bidi_control(c), "U+{:04X} should be a control", c as u32);
        }
        for c in ['\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}'] {
            assert!(is_bidi_control(c), "U+{:04X} should be a control", c as u32);
        }
    }

    #[test]
    fn marks_and_zero_width_are_not_bidi_controls() {
        for c in [
            '\u{200E}', '\u{200F}', '\u{200B}', '\u{FEFF}', 'a', '\u{2065}', '\u{206A}',
        ] {
            assert!(
                !is_bidi_control(c),
                "U+{:04X} must not be a control",
                c as u32
            );
        }
    }

    #[test]
    fn escape_text_strips_bidi_controls() {
        assert_eq!(escape_text("a\u{202e}b"), "ab");
        // Strip AND escape in one pass.
        assert_eq!(escape_text("a\u{202e}<b>"), "a&lt;b&gt;");
        // Nothing to strip: fast path returns the input unchanged.
        assert_eq!(escape_text("plain"), "plain");
    }

    #[test]
    fn sanitize_url_blocks_os_handler_schemes() {
        // OS protocol-handler / command-execution schemes (CVE-2026-20841 class)
        // are blanked everywhere a URL is emitted.
        for url in [
            "ms-msdt:/id",
            "ms-office:ofe|u|http://evil/x.docm",
            "ms-word:ofe|u|x",
            "ms-excel:ofe|u|x",
            "ms-powerpoint:ofe|u|x",
            "ms-access:x",
            "ms-visio:x",
            "ms-project:x",
            "ms-publisher:x",
            "ms-infopath:x",
            "ms-spd:x",
            "ms-search:x",
            "search-ms:x",
            "ms-cxh:x",
            "ms-cxh-full:x",
            "shell:Startup",
            "vscode:x",
            "vscode-insiders:x",
            "jar:http://evil/x.jar!/",
        ] {
            assert_eq!(sanitize_url(url), "", "{url} must be blanked");
        }
    }

    #[test]
    fn sanitize_url_os_handler_block_is_case_insensitive() {
        assert_eq!(sanitize_url("MS-OFFICE:ofe|u|x"), "");
        assert_eq!(sanitize_url("Ms-Msdt:/id"), "");
        assert_eq!(sanitize_url("SHELL:Startup"), "");
        assert_eq!(sanitize_url("VSCode:x"), "");
    }

    #[test]
    fn sanitize_url_allows_safe_schemes() {
        for url in [
            "https://ok.com",
            "http://ok.com",
            "mailto:a@b.com",
            "tel:+15551234",
            "ftp://ok.com/x",
            "sms:+15551234",
            "/relative/path",
            "#anchor",
        ] {
            assert_eq!(sanitize_url(url), url, "{url} must pass unchanged");
        }
    }

    #[test]
    fn sanitize_attr_value_blocks_os_handler_overrides() {
        // `{href=...}` / `{src=...}` attribute overrides route through the same set.
        assert_eq!(sanitize_attr_value("href", "ms-office:ofe|u|x"), "");
        assert_eq!(sanitize_attr_value("src", "ms-msdt:/id"), "");
        assert_eq!(sanitize_attr_value("href", "SHELL:Startup"), "");
        // Safe schemes survive.
        assert_eq!(
            sanitize_attr_value("href", "https://ok.com"),
            "https://ok.com"
        );
        assert_eq!(
            sanitize_attr_value("href", "tel:+15551234"),
            "tel:+15551234"
        );
    }
}
