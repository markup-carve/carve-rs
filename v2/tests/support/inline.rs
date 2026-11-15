//! Shared by the inline tests: building the pieces of a top-level paragraph
//! from an oracle tree and comparing the inline pass with it.
#![allow(dead_code)]

use carve2::inline::{ContentKind, InlineEnv, Join, Piece, parse};
use carve2::source::Source;
use serde_json::Value;
use std::collections::HashSet;

/// Result fields the resolve stage adds (ast-results.json); the inline pass
/// leaves them out.
pub fn strip_results(v: &mut Value) {
    match v {
        Value::Object(m) => {
            let ty = m
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let is_ref = m.contains_key("key");
            match ty.as_str() {
                "footnote_ref" | "inline_footnote" | "math" => {
                    m.remove("number");
                    m.remove("numberPrefix");
                }
                "link" | "image" if is_ref => {
                    m.remove("href");
                    m.remove("src");
                    m.remove("tooltip");
                }
                "xref" => {
                    m.remove("href");
                }
                "caption_number" => {
                    m.remove("n");
                }
                _ => {}
            }
            for (_, c) in m.iter_mut() {
                strip_results(c);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(strip_results),
        _ => {}
    }
}

pub fn pieces_for(src: &Source, node: &Value, heading: bool) -> Option<Vec<Piece>> {
    let pos = node.get("pos")?;
    let first = usize::try_from(pos.get("startLine")?.as_u64()?).ok()? - 1;
    let last = usize::try_from(pos.get("endLine")?.as_u64()?).ok()? - 1;
    let start_off = usize::try_from(pos.get("startOffset")?.as_u64()?).ok()?;
    let text = src.text();
    let mut out = Vec::new();
    for line in first..=last {
        let (mut s, e) = src.line_range(line);
        if line == first {
            s = start_off;
            if heading {
                let b = text.as_bytes();
                while s < e && matches!(b.get(s), Some(b'#' | b'=')) {
                    s += 1;
                }
            }
        }
        while s < e && matches!(text.as_bytes().get(s), Some(b' ' | b'\t')) {
            s += 1;
        }
        out.push(Piece {
            start: s,
            end: e,
            join: Join::Line,
        });
    }
    Some(out)
}

/// One top-level paragraph or heading compared with the oracle.
pub struct Compared {
    pub expected: Value,
    pub actual: Value,
}

/// Parses every top-level paragraph and heading of `input` with the
/// definitions the oracle tree holds, beside the oracle's children.
pub fn compare(input: &str, tree: &Value, sorted: fn(&Value) -> String) -> Vec<(String, String)> {
    let children = tree
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut keys = HashSet::new();
    let mut abbrs: Vec<(String, String)> = Vec::new();
    for c in &children {
        let key = c
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match c.get("type").and_then(Value::as_str) {
            Some("link_reference_definition") => {
                keys.insert(key);
            }
            Some("abbreviation_def") if !abbrs.iter().any(|(k, _)| *k == key) => {
                let exp = c
                    .get("expansion")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                abbrs.push((key, exp));
            }
            _ => {}
        }
    }
    let src = Source::new(input);
    let mut out = Vec::new();
    for c in &children {
        let ty = c.get("type").and_then(Value::as_str).unwrap_or("");
        if !matches!(ty, "paragraph" | "heading")
            || c.get("attrs").is_some()
            || c.get("role").is_some()
        {
            continue;
        }
        let heading = ty == "heading";
        let Some(pieces) = pieces_for(&src, c, heading) else {
            continue;
        };
        let env = InlineEnv {
            src: &src,
            link_keys: &keys,
            abbreviations: &abbrs,
            depth: 0,
            kind: if heading {
                ContentKind::Heading
            } else {
                ContentKind::Paragraph
            },
            hard_breaks: false,
        };
        let nodes = parse(&env, &pieces);
        let mut expected = c
            .get("children")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        strip_results(&mut expected);
        let actual = serde_json::to_value(&nodes).unwrap_or_default();
        out.push((sorted(&expected), sorted(&actual)));
    }
    out
}
