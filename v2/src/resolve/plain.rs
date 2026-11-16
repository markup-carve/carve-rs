//! The plain text of inline content (C2-H-143), shared by heading slugs,
//! caption keys, cross-reference text and task labels.

use crate::ast::Node;

use std::collections::BTreeMap;

/// How a symbol contributes: nothing (C2-H-143) or the text the caption
/// writes for it (C2-B-208).
#[derive(Clone, Copy)]
pub enum Symbols<'a> {
    Nothing,
    Written(&'a BTreeMap<String, String>),
}

/// Appends the plain text of `nodes` to `out`.
pub fn inline_text(nodes: &[Node], symbols: Symbols<'_>, out: &mut String) {
    for n in nodes {
        node_text(n, symbols, out, 0, false);
    }
}

/// The plain text of `nodes` as a new string.
#[must_use]
pub fn text_of(nodes: &[Node]) -> String {
    let mut s = String::new();
    inline_text(nodes, Symbols::Nothing, &mut s);
    s
}

/// C2-H-165: block degradation keeps an image's fallback spelling.
#[must_use]
pub fn text_of_degraded_block(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        node_text(n, Symbols::Nothing, &mut s, 0, true);
    }
    s
}

fn node_text(n: &Node, symbols: Symbols<'_>, out: &mut String, depth: usize, image_spelling: bool) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    if leaf_text(n, symbols, out, image_spelling) {
        return;
    }
    match n {
        Node::Ruby(r) => {
            for p in r.pairs.iter() {
                if let Node::RubyPair(p) = p {
                    children_text(p.base, symbols, out, depth, image_spelling);
                    out.push('(');
                    children_text(p.annotation, symbols, out, depth, image_spelling);
                    out.push(')');
                }
            }
        }
        Node::InlineExtension(e) if e.name == "index" => {}
        Node::FootnoteRef(_)
        | Node::InlineFootnote(_)
        | Node::Xref(_)
        | Node::RawInline(_)
        | Node::CriticComment(_)
        | Node::Comment(_)
        | Node::CitationGroup(_) => {}
        other => {
            if let Some(c) = other.children() {
                children_text(c, symbols, out, depth, image_spelling);
            }
        }
    }
}

fn children_text(
    nodes: &[Node],
    symbols: Symbols<'_>,
    out: &mut String,
    depth: usize,
    image_spelling: bool,
) {
    for c in nodes {
        node_text(c, symbols, out, depth + 1, image_spelling);
    }
}

/// The nodes that give a fixed text; true when `n` was one of them.
fn leaf_text(n: &Node, symbols: Symbols<'_>, out: &mut String, image_spelling: bool) -> bool {
    match n {
        _ if leaf_text_simple(n, out, image_spelling) => return true,
        Node::Mention(m) => {
            out.push('@');
            out.push_str(m.target);
        }
        Node::Tag(t) => {
            out.push('#');
            out.push_str(t.name);
        }
        Node::CaptionNumber(c) => {
            if let Some(n) = c.n {
                out.push_str(&n.to_string());
            }
        }
        Node::Symbol(s) => {
            if let Symbols::Written(map) = symbols {
                symbol_text(map, s.name, out);
            }
        }
        _ => return false,
    }
    true
}

fn leaf_text_simple(n: &Node, out: &mut String, image_spelling: bool) -> bool {
    match n {
        Node::Text(t) => out.push_str(t.value),
        Node::Code(t) => out.push_str(t.value),
        Node::LiteralInline(t) => out.push_str(t.value),
        Node::Math(t) => out.push_str(t.value),
        Node::NonBreakingSpace(_) => out.push('\u{a0}'),
        Node::SoftBreak(_) | Node::HardBreak(_) => out.push(' '),
        Node::SmartPunctuation(s) => out.push_str(s.glyph.unwrap_or(s.source)),
        Node::Image(i) => {
            if image_spelling {
                out.push_str("[img: ");
                out.push_str(i.alt.unwrap_or(""));
                out.push(']');
            } else {
                out.push_str(i.alt.unwrap_or(""));
            }
        }
        Node::Autolink(a) => out.push_str(a.text.unwrap_or(a.href)),
        Node::Abbreviation(a) => out.push_str(a.key),
        _ => return false,
    }
    true
}

/// The text a symbol is written as (C2-H-101).
pub fn symbol_text(map: &BTreeMap<String, String>, name: &str, out: &mut String) {
    if let Some(v) = map.get(name) {
        out.push_str(v);
    } else {
        out.push(':');
        out.push_str(name);
        out.push(':');
    }
}

/// A canonical source spelling of inline content, for the heading match of
/// C2-I-084 on a tree without its source (gap HTML-002): delimiter pairs in
/// their bare form, code spans in one backtick, every other node as its plain
/// text.
pub fn source_text(nodes: &[Node], out: &mut String, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    for n in nodes {
        let mark = match n {
            Node::Strong(_) => "*",
            Node::Emphasis(_) => "/",
            Node::SmartPunctuation(s) => {
                out.push_str(s.source);
                continue;
            }
            Node::Code(c) => {
                out.push('`');
                out.push_str(c.value);
                out.push('`');
                continue;
            }
            _ => {
                node_text(n, Symbols::Nothing, out, depth, false);
                continue;
            }
        };
        out.push_str(mark);
        source_text(n.children().unwrap_or_default(), out, depth + 1);
        out.push_str(mark);
    }
}
