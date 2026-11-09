use super::*;

pub(in crate::parse) fn apply_abbreviations(doc: &mut Document) {
    let mut defs = BTreeMap::new();
    for child in &doc.children {
        if let BlockNode::AbbreviationDef(def) = child {
            defs.insert(def.abbr.clone(), def.expansion.clone());
        }
    }
    if defs.is_empty() {
        return;
    }
    // The definitions stay in the tree. HTML uses document children as the
    // abbreviation table, while the non-HTML renderers need the authored
    // definition line to survive as source.
    let index = abbreviation_index(&defs);
    for block in &mut doc.children {
        apply_abbreviations_block(block, &index);
    }
}

#[derive(Default)]
pub(in crate::parse) struct AbbreviationTrieNode<'a> {
    children: HashMap<u8, usize>,
    definition: Option<(&'a str, &'a str)>,
}

pub(in crate::parse) type AbbreviationIndex<'a> = Vec<AbbreviationTrieNode<'a>>;

pub(in crate::parse) fn abbreviation_index(
    defs: &BTreeMap<String, String>,
) -> AbbreviationIndex<'_> {
    let mut index = vec![AbbreviationTrieNode::default()];
    for (abbr, expansion) in defs {
        if abbr.is_empty() {
            continue;
        }
        let mut node = 0;
        for byte in abbr.bytes() {
            node = if let Some(&child) = index[node].children.get(&byte) {
                child
            } else {
                let child = index.len();
                index.push(AbbreviationTrieNode::default());
                index[node].children.insert(byte, child);
                child
            };
        }
        index[node].definition = Some((abbr.as_str(), expansion.as_str()));
    }
    index
}

pub(in crate::parse) fn apply_abbreviations_block(
    block: &mut BlockNode,
    index: &AbbreviationIndex<'_>,
) {
    match block {
        BlockNode::Heading(h) => apply_abbreviations_inline(&mut h.children, index),
        BlockNode::Paragraph(p) => apply_abbreviations_inline(&mut p.children, index),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    apply_abbreviations_block(child, index);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                apply_abbreviations_inline(caption, index);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    apply_abbreviations_inline(&mut cell.children, index);
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                apply_abbreviations_inline(title, index);
            }
            for child in &mut a.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::FigureGroup(g) => {
            for child in &mut g.children {
                apply_abbreviations_block(child, index);
            }
            if let Some(caption) = &mut g.caption {
                apply_abbreviations_inline(caption, index);
            }
        }
        // A `:::` div and a block extension were missing, so an abbreviation
        // never expanded inside one -- even with the definition at the top
        // level, where collection was never in doubt. carve-js expands it.
        BlockNode::Directive(d) => {
            if let Some(title) = &mut d.title {
                apply_abbreviations_inline(title, index);
            }
            for child in &mut d.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::ExtensionCarrier(e) => {
            if let Some(summary) = &mut e.summary {
                apply_abbreviations_inline(summary, index);
            }
            for child in &mut e.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for term in &mut item.terms {
                    apply_abbreviations_inline(&mut term.children, index);
                }
                for def in &mut item.definitions {
                    for child in &mut def.children {
                        apply_abbreviations_block(child, index);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            apply_abbreviations_inline(&mut f.caption, index);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        apply_abbreviations_block(child, index);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        apply_abbreviations_inline(caption, index);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            apply_abbreviations_inline(&mut cell.children, index);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            }
        }
        _ => {}
    }
}

pub(in crate::parse) fn apply_abbreviations_inline(
    nodes: &mut Vec<InlineNode>,
    index: &AbbreviationIndex<'_>,
) {
    apply_abbreviations_inline_between(nodes, index, None, None);
}

fn abbreviation_edge(node: &InlineNode, first: bool) -> Option<char> {
    let children: &[InlineNode] = match node {
        InlineNode::Text(text) => {
            return if first {
                text.value.chars().next()
            } else {
                text.value.chars().next_back()
            }
        }
        InlineNode::EscapedText(n) => {
            return if first {
                n.value.chars().next()
            } else {
                n.value.chars().next_back()
            }
        }
        InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CellWrap(_)
        | InlineNode::NonBreakingSpace(_) => return Some(' '),
        InlineNode::Emphasis(n) => &n.children,
        InlineNode::Link(n) => &n.children,
        InlineNode::Span(n) => &n.children,
        InlineNode::Extension(n) => &n.children,
        InlineNode::CriticInsert(n) => &n.children,
        InlineNode::CriticDelete(n) => &n.children,
        _ => return None,
    };
    if first {
        children
            .iter()
            .find(|child| !matches!(child, InlineNode::Comment(_)))
            .and_then(|child| abbreviation_edge(child, true))
    } else {
        children
            .iter()
            .rev()
            .find(|child| !matches!(child, InlineNode::Comment(_)))
            .and_then(|child| abbreviation_edge(child, false))
    }
}

fn apply_abbreviations_inline_between(
    nodes: &mut Vec<InlineNode>,
    index: &AbbreviationIndex<'_>,
    before: Option<char>,
    after: Option<char>,
) {
    let mut left = Vec::with_capacity(nodes.len());
    let mut preceding = before;
    for node in nodes.iter() {
        left.push(preceding);
        if !matches!(node, InlineNode::Comment(_)) {
            preceding = abbreviation_edge(node, false);
        }
    }
    let mut right = vec![None; nodes.len()];
    let mut following = after;
    for (i, node) in nodes.iter().enumerate().rev() {
        right[i] = following;
        if !matches!(node, InlineNode::Comment(_)) {
            following = abbreviation_edge(node, true);
        }
    }
    let mut out = Vec::with_capacity(nodes.len());
    for (i, node) in std::mem::take(nodes).into_iter().enumerate() {
        match node {
            InlineNode::Text(text) => {
                append_abbreviations_in_text_between(
                    text.value, index, text.pos, &mut out, left[i], right[i],
                );
            }
            InlineNode::Emphasis(mut e) => {
                apply_abbreviations_inline_between(&mut e.children, index, left[i], right[i]);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Link(mut l) => {
                apply_abbreviations_inline_between(&mut l.children, index, left[i], right[i]);
                out.push(InlineNode::Link(l));
            }
            InlineNode::Extension(mut e) => {
                apply_abbreviations_inline_between(&mut e.children, index, left[i], right[i]);
                out.push(InlineNode::Extension(e));
            }
            // PART 9R R3 matches a term in RENDERED TEXT at word boundaries, and
            // the container the text sits in does not change that. A span fell
            // to the catch-all arm below and its children were never walked, so
            // `[HTML]{.x}` and `[HTML]{kbd}` silently dropped the expansion that
            // `*HTML*` and `[HTML](/u)` got - and PART 9 §10 made the second
            // spelling a documented feature, so the loss sits inside a construct
            // the docs teach (carve#1151).
            InlineNode::Span(mut sp) => {
                apply_abbreviations_inline_between(&mut sp.children, index, left[i], right[i]);
                out.push(InlineNode::Span(sp));
            }
            InlineNode::CitationGroup(mut g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        apply_abbreviations_inline(prefix, index);
                    }
                    if let Some(locator) = &mut item.locator {
                        apply_abbreviations_inline(locator, index);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        apply_abbreviations_inline(suffix, index);
                    }
                }
                out.push(InlineNode::CitationGroup(g));
            }
            InlineNode::Footnote(mut f) => {
                if let Some(inline) = &mut f.inline {
                    apply_abbreviations_inline(inline, index);
                }
                out.push(InlineNode::Footnote(f));
            }
            InlineNode::CriticInsert(mut c) => {
                apply_abbreviations_inline_between(&mut c.children, index, left[i], right[i]);
                out.push(InlineNode::CriticInsert(c));
            }
            InlineNode::CriticDelete(mut c) => {
                apply_abbreviations_inline_between(&mut c.children, index, left[i], right[i]);
                out.push(InlineNode::CriticDelete(c));
            }
            InlineNode::CriticSubstitute(mut c) => {
                apply_abbreviations_inline(&mut c.old, index);
                apply_abbreviations_inline(&mut c.new, index);
                out.push(InlineNode::CriticSubstitute(c));
            }
            InlineNode::Ruby(mut r) => {
                for pair in &mut r.pairs {
                    apply_abbreviations_inline(&mut pair.base, index);
                    apply_abbreviations_inline(&mut pair.annotation, index);
                }
                out.push(InlineNode::Ruby(r));
            }
            other => out.push(other),
        }
    }
    *nodes = out;
}

/// Split a text node around the abbreviations in it.
///
/// The pieces are CONTIGUOUS SLICES of the node being split, so when that node
/// carried a span every piece's span follows from its offset within it - no
/// re-scanning of the document, and no invention.
///
/// The two guards are what keep that true. A text node whose span is a
/// different length than its value is not a verbatim slice of the source (the
/// no-break-space sentinel is one character standing in for two), and a node
/// spanning more than one line has no single column to count from. Either way
/// the pieces get no position rather than a derived-from-wrong one.
fn append_abbreviations_in_text_between(
    text: String,
    index: &AbbreviationIndex<'_>,
    pos: Option<Pos>,
    out: &mut Vec<InlineNode>,
    before: Option<char>,
    after: Option<char>,
) {
    let anchor = pos.filter(|p| {
        p.start_line == p.end_line
            && p.end_offset.saturating_sub(p.start_offset) == text.chars().count()
    });
    // Chars consumed so far, which is the offset of the NEXT piece.
    let mut chars_done = 0usize;
    let span_from = |start: usize, len: usize| -> Option<Pos> {
        // By reference: `Pos` is no longer `Copy`, and this closure runs
        // once per piece.
        let p = anchor.as_ref()?;
        Some(Pos {
            start_line: p.start_line,
            end_line: p.start_line,
            start_column: p.start_column + start,
            end_column: p.start_column + start + len,
            start_offset: p.start_offset + start,
            end_offset: p.start_offset + start + len,
            file: None,
        })
    };
    let mut plain_start = 0;
    let mut plain_chars = 0;
    let mut i = 0;
    while i < text.len() {
        let mut matched: Option<(&str, &str)> = None;
        let ch = text[i..].chars().next().unwrap();
        if abbreviation_boundary(text[..i].chars().next_back().or(before)) {
            let mut node = 0;
            for (offset, byte) in text.as_bytes()[i..].iter().enumerate() {
                let Some(&child) = index[node].children.get(byte) else {
                    break;
                };
                node = child;
                if let Some(definition) = index[node].definition {
                    if abbreviation_boundary(text[i + offset + 1..].chars().next().or(after)) {
                        matched = Some(definition);
                        break;
                    }
                }
            }
        }
        if let Some((abbr, expansion)) = matched {
            if plain_start < i {
                out.push(InlineNode::Text(Text {
                    value: text[plain_start..i].to_owned(),
                    pos: span_from(plain_chars, chars_done - plain_chars),
                }));
            }
            let len = abbr.chars().count();
            out.push(InlineNode::Abbreviation(Abbreviation {
                abbr: abbr.to_string(),
                expansion: expansion.to_string(),
                pos: span_from(chars_done, len),
            }));
            chars_done += len;
            i += abbr.len();
            plain_start = i;
            plain_chars = chars_done;
            continue;
        }
        chars_done += 1;
        i += ch.len_utf8();
    }
    if plain_start < text.len() {
        let value = if plain_start == 0 {
            text
        } else {
            text[plain_start..].to_owned()
        };
        out.push(InlineNode::Text(Text {
            value,
            pos: span_from(plain_chars, chars_done - plain_chars),
        }));
    }
}

fn abbreviation_boundary(adjacent: Option<char>) -> bool {
    !adjacent.is_some_and(quote_state::is_alnum)
}
