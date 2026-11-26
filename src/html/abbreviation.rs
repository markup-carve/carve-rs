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
                || def.expansion.contains('\n')
                || def.expansion.ends_with([' ', '\t'])
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
    out.push_str(&def.expansion.replace("\r\n", "\n").replace('\n', " "));
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

impl Renderer<'_, '_> {
    /// C2-H-216: under `roundtrip` a run of snippet definitions is one template
    /// that holds the run as the canonical writer spells it. A run with a
    /// definition of an included file carries the scope. Returns its length.
    pub(super) fn snippet_run(&mut self, nodes: &[Node], depth: usize) -> usize {
        let run: Vec<&Node> = nodes
            .iter()
            .take_while(|n| matches!(n, Node::SnippetDefinition(_)))
            .collect();
        let included = crate::snippet::expand::with_side(|side| {
            side.is_some_and(|s| {
                run.iter()
                    .any(|n| s.included_defs.contains(&n.payload_addr()))
            })
        });
        let mut children: Vec<serde_json::Value> = run
            .iter()
            .filter_map(|n| serde_json::to_value(n).ok())
            .collect();
        plain_abbreviations(&mut children);
        let doc =
            serde_json::json!({"type": "document", "astVersion": "2.0", "children": children});
        let (source, entries) = crate::write::write_value(&doc, &crate::write::Settings::default());
        for entry in &entries {
            if let Some(&(code, ..)) = crate::tables::MESSAGES
                .iter()
                .find(|(c, ..)| entry.code == *c)
            {
                self.losses.push(code);
            }
        }
        let text = escape_template(source.strip_suffix('\n').unwrap_or(&source));
        let scope = if included {
            " data-carve-snippet-scope=\"include\""
        } else {
            ""
        };
        self.line(
            depth,
            &format!("<template data-carve-snippets{scope}>{text}</template>"),
        );
        run.len()
    }
}

/// C2-I-098: an abbreviation in a definition is its term as text in the template.
fn plain_abbreviations(nodes: &mut Vec<serde_json::Value>) {
    use serde_json::Value;
    for node in nodes.iter_mut() {
        if node["type"] == "abbreviation" {
            let key = node["key"].clone();
            *node = serde_json::json!({"type": "text", "value": key});
            continue;
        }
        if let Some(map) = node.as_object_mut() {
            for value in map.values_mut() {
                if let Value::Array(list) = value
                    && list.iter().all(|v| v.get("type").is_some())
                {
                    plain_abbreviations(list);
                }
            }
        }
    }
    let plain = |v: &Value| v["type"] == "text" && v.as_object().is_some_and(|m| m.len() == 2);
    let mut joined: Vec<Value> = Vec::with_capacity(nodes.len());
    for node in nodes.drain(..) {
        if let Some(last) = joined.last_mut()
            && plain(last)
            && plain(&node)
        {
            let text = format!(
                "{}{}",
                last["value"].as_str().unwrap_or(""),
                node.get("value").and_then(Value::as_str).unwrap_or("")
            );
            last["value"] = Value::String(text);
            continue;
        }
        joined.push(node);
    }
    *nodes = joined;
}
