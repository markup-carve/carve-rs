//! Roundtrip abbreviation templates (C2-H-178 to C2-H-181).

use super::Renderer;
use crate::ast::{AbbreviationDef, Attrs, Node};

impl Renderer<'_, '_> {
    /// Writes a maximal run and returns its length.
    pub(super) fn abbreviation_run(&mut self, nodes: &[Node], depth: usize) -> usize {
        let mut source = String::new();
        let mut n = 0;
        for node in nodes {
            let Node::AbbreviationDef(def) = node else {
                break;
            };
            if n > 0 {
                source.push_str("\n\n");
            }
            if !def.key.bytes().all(|b| b.is_ascii_alphanumeric())
                || def.key.is_empty()
                || def.expansion.is_empty()
                || def.expansion.contains(['\n', '\r'])
            {
                self.losses.push("field-unspellable");
            }
            source.push_str(&definition(def));
            n += 1;
        }
        if n > 0 {
            let text = escape_template(&source);
            self.line(
                depth,
                &format!("<template data-carve-abbreviations>{text}</template>"),
            );
        }
        n
    }
}

fn definition(def: &AbbreviationDef<'_>) -> String {
    let mut out = String::new();
    if let Some(attrs) = attrs_source(def.attrs, def.lang) {
        out.push_str(&attrs);
        out.push('\n');
    }
    out.push_str("*[");
    out.push_str(def.key);
    out.push_str("]: ");
    out.push_str(def.expansion);
    out
}

fn attrs_source(attrs: Option<&Attrs<'_>>, lang: Option<&str>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(attrs) = attrs {
        if let Some(id) = attrs.id {
            parts.push(format!("#{id}"));
        }
        for class in attrs.classes {
            if class
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                parts.push(format!(".{class}"));
            } else {
                parts.push(format!("class={}", value_source(class)));
            }
        }
    }
    if let Some(lang) = lang {
        parts.push(format!(":{lang}"));
    }
    if let Some(attrs) = attrs {
        for &(key, value) in attrs.key_values {
            if value.is_empty() && !key.starts_with('_') {
                parts.push(key.to_string());
            } else {
                parts.push(format!("{key}={}", value_source(value)));
            }
        }
    }
    (!parts.is_empty()).then(|| format!("{{{}}}", parts.join(" ")))
}

fn value_source(value: &str) -> String {
    if !value.is_empty()
        && !value
            .chars()
            .any(|c| matches!(c, ' ' | '\t' | '"' | '\\' | '|' | '}'))
    {
        return value.to_string();
    }
    let quoted = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{quoted}\"")
}

fn escape_template(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for c in source.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}
