//! GitHub alert blockquotes import as admonitions (CARVE-P2-033).

use crate::ast::{Admonition, BlockNode, BlockQuote, InlineNode, Paragraph};

/// GitHub alert label -> the Carve admonition kind it imports as.
const KINDS: [(&str, &str); 5] = [
    ("[!NOTE]", "note"),
    ("[!TIP]", "tip"),
    ("[!IMPORTANT]", "info"),
    ("[!WARNING]", "warning"),
    ("[!CAUTION]", "danger"),
];

/// The admonition kind for a label spelled exactly as GitHub documents it.
pub(crate) fn kind(label: &str) -> Option<&'static str> {
    KINDS
        .iter()
        .find(|(spelling, _)| *spelling == label)
        .map(|(_, kind)| *kind)
}

/// The kind named by a quote's first source line, read from the raw source so
/// an escaped `\[!NOTE]` stays literal as it does on GitHub.
pub(crate) fn first_line_kind(quote: &str) -> Option<&'static str> {
    let line = quote.split(['\n', '\r']).next().unwrap_or_default();
    let body = line
        .trim_start_matches([' ', '\t'])
        .strip_prefix('>')?
        .trim_matches([' ', '\t']);
    kind(body)
}

/// The `aria-label` the HTML renderer writes for an admonition kind.
pub(crate) fn aria_label(kind: &str) -> String {
    let mut chars = kind.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + chars.as_str()
    })
}

/// Rewrites a top-level quote whose first line named `kind` as its admonition.
///
/// The label must be the first paragraph's whole first line, ended by a soft
/// break or the paragraph; a hard break keeps the quote. A quote with nothing
/// after the label stays a quote.
pub(crate) fn admonition(kind: &'static str, quote: BlockQuote) -> BlockNode {
    let mut children = quote.children;
    let Some(BlockNode::Paragraph(head)) = children.first() else {
        return BlockNode::BlockQuote(BlockQuote { children, ..quote });
    };
    let leading = head
        .children
        .iter()
        .take_while(|node| matches!(node, InlineNode::Text(_)))
        .count();
    let label: String = head.children[..leading]
        .iter()
        .filter_map(|node| match node {
            InlineNode::Text(text) => Some(text.value.as_str()),
            _ => None,
        })
        .collect();
    let ends_line = match head.children.get(leading) {
        None => true,
        Some(node) => matches!(node, InlineNode::SoftBreak(_)),
    };
    if quote.attrs.is_some()
        || head.attrs.is_some()
        || self::kind(&label) != Some(kind)
        || !ends_line
    {
        return BlockNode::BlockQuote(BlockQuote { children, ..quote });
    }
    let tail: Vec<InlineNode> = head.children.iter().skip(leading + 1).cloned().collect();
    let has_body = !tail.is_empty() || children.len() > 1;
    if !has_body {
        return BlockNode::BlockQuote(BlockQuote { children, ..quote });
    }
    if tail.is_empty() {
        children.remove(0);
    } else if let Some(BlockNode::Paragraph(head)) = children.first_mut() {
        *head = Paragraph {
            children: tail,
            ..head.clone()
        };
    }
    BlockNode::Admonition(Admonition {
        attrs: quote.attrs,
        kind: kind.to_owned(),
        title: None,
        label: None,
        children,
        pos: quote.pos,
    })
}
