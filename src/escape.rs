//! HTML escaping helpers. Behavior matches `carve-js`/`render-html.ts`:
//! text content escapes `&`, `<`, `>`; attribute values additionally
//! escape `"`.

pub fn escape_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

pub fn escape_attr(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
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
pub fn sanitize_attr_value(name: &str, value: &str) -> String {
    if let Some(colon) = value.find(':') {
        let scheme: String = value[..colon]
            .chars()
            .filter(|c| (*c as u32) > 0x20)
            .collect::<String>()
            .to_ascii_lowercase();
        if DANGEROUS_VALUE_SCHEMES.contains(&scheme.as_str()) {
            return String::new();
        }
    }
    if name.eq_ignore_ascii_case("style") {
        let compact: String = value
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if compact.contains("expression(") {
            return String::new();
        }
    }
    value.to_string()
}
