//! Unit tests of the inline pass, one or more per clause. Each parses one
//! paragraph and compares a compact rendering of the tree.

use carve2::ast::Node;
use carve2::inline::{ContentKind, InlineEnv, Join, Piece, parse};
use carve2::source::Source;
use serde_json::Value;
use std::collections::HashSet;

/// Options of one parse.
#[derive(Default)]
struct Opts<'a> {
    kind: Option<ContentKind>,
    keys: &'a [&'a str],
    abbrs: &'a [(&'a str, &'a str)],
    depth: usize,
    hard: bool,
    cell_join: bool,
}

fn run(input: &str, o: &Opts<'_>) -> Vec<Node> {
    let src = Source::new(input);
    let mut pieces = Vec::new();
    let mut at = 0;
    for (n, line) in input.split('\n').enumerate() {
        let lead = if o.kind == Some(ContentKind::LineBlockLine) {
            0
        } else {
            line.len() - line.trim_start().len()
        };
        let join = if n > 0 && o.cell_join {
            Join::Cell
        } else {
            Join::Line
        };
        pieces.push(Piece {
            start: at + lead,
            end: at + line.len(),
            join,
        });
        at += line.len() + 1;
    }
    let keys: HashSet<String> = o.keys.iter().map(|k| (*k).to_string()).collect();
    let abbrs: Vec<(String, String)> = o
        .abbrs
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect();
    let env = InlineEnv {
        src: &src,
        link_keys: &keys,
        abbreviations: &abbrs,
        depth: o.depth,
        kind: o.kind.unwrap_or(ContentKind::Paragraph),
        hard_breaks: o.hard,
    };
    parse(&env, &pieces)
}

/// Compact form: `type(field=value ...)[children]`, text as its quoted value.
fn show(nodes: &[Node]) -> String {
    let v = serde_json::to_value(nodes).unwrap_or_default();
    let mut out = String::new();
    walk_list(&v, &mut out);
    out
}

fn walk_list(v: &Value, out: &mut String) {
    for (i, n) in v.as_array().into_iter().flatten().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        walk(n, out);
    }
}

fn walk(n: &Value, out: &mut String) {
    let Some(m) = n.as_object() else { return };
    let ty = m.get("type").and_then(Value::as_str).unwrap_or("?");
    if ty == "text" {
        out.push_str(&format!(
            "{:?}",
            m.get("value").and_then(Value::as_str).unwrap_or("")
        ));
        return;
    }
    out.push_str(ty);
    let mut fields: Vec<String> = Vec::new();
    for (k, v) in m {
        if matches!(k.as_str(), "type" | "children" | "pos") {
            continue;
        }
        let shown = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        fields.push(format!("{k}={shown}"));
    }
    if !fields.is_empty() {
        out.push_str(&format!("({})", fields.join(" ")));
    }
    if let Some(c) = m.get("children") {
        out.push('[');
        walk_list(c, out);
        out.push(']');
    }
}

fn p(input: &str) -> String {
    show(&run(input, &Opts::default()))
}

fn po(input: &str, o: &Opts<'_>) -> String {
    show(&run(input, o))
}

fn offsets(nodes: &[Node]) -> Vec<(String, usize, usize)> {
    nodes
        .iter()
        .map(|n| {
            let pos = n
                .pos()
                .map_or((usize::MAX, usize::MAX), |p| (p.start_offset, p.end_offset));
            (n.type_name().to_string(), pos.0, pos.1)
        })
        .collect()
}

#[test]
fn debug_abbr_in_footnote() {
    let o = Opts {
        abbrs: &[("HT", "Hyper")],
        ..Default::default()
    };
    assert_eq!(
        po("word^[HT]", &o),
        "\"word\" inline_footnote[abbreviation(expansion=Hyper key=HT namespace=abbr)]"
    );
}

#[test]
fn debug_ge() {
    assert_eq!(
        p("a {->=}~"),
        "\"a {-\" smart_punctuation(glyph=≥ kind=greater_than_or_equal source=>=) \"}~\""
    );
}
