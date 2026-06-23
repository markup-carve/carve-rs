use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::escape::escape_attr;
use crate::extension::{
    BeforeRenderContext, CarveExtension, InlineMatch, MatcherContext, RenderContext,
};

const REFS_BLOCK: &str = "citations-references";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationMode {
    Numbered,
    AuthorDate,
}

impl From<CitationMode> for CitationRenderMode {
    fn from(mode: CitationMode) -> Self {
        match mode {
            CitationMode::Numbered => CitationRenderMode::Numbered,
            CitationMode::AuthorDate => CitationRenderMode::AuthorDate,
        }
    }
}

#[derive(Debug, Clone)]
struct Def {
    entry: Vec<InlineNode>,
    author: Option<String>,
    year: Option<String>,
}

pub struct Citations {
    mode: CitationMode,
    defs: RefCell<BTreeMap<String, Def>>,
    order: RefCell<Vec<String>>,
}

impl Citations {
    pub fn new() -> Self {
        Self::with_mode(CitationMode::Numbered)
    }

    pub fn author_date() -> Self {
        Self::with_mode(CitationMode::AuthorDate)
    }

    pub fn with_mode(mode: CitationMode) -> Self {
        Self {
            mode,
            defs: RefCell::new(BTreeMap::new()),
            order: RefCell::new(Vec::new()),
        }
    }
}

impl Default for Citations {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for Citations {
    fn name(&self) -> &'static str {
        "citations"
    }

    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        match_citation(text, pos, ctx)
    }

    fn after_parse(&self, mut doc: Document) -> Document {
        self.defs.borrow_mut().clear();
        self.order.borrow_mut().clear();
        doc.children = collect_defs(doc.children, &mut self.defs.borrow_mut());
        doc
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        let defs = self.defs.borrow();
        let mut seen = BTreeSet::new();
        let mut order = Vec::new();
        for block in &mut doc.children {
            annotate_citations_block(block, &defs, self.mode, &mut seen, &mut order);
        }
        drop(defs);
        *self.order.borrow_mut() = order;
        if !self.order.borrow().is_empty() {
            inject_references_block(&mut doc.children);
        }
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name == REFS_BLOCK {
            Some(render_refs_list(
                ctx,
                self.mode,
                &self.order.borrow(),
                &self.defs.borrow(),
            ))
        } else {
            None
        }
    }
}

fn match_citation(text: &str, pos: usize, ctx: &MatcherContext<'_>) -> Option<InlineMatch> {
    if !text.get(pos..)?.starts_with('[') {
        return None;
    }
    let close = close_bracket(text, pos)?;
    if matches!(text.as_bytes().get(close + 1), Some(b'(' | b'[' | b'{')) {
        return None;
    }
    let inner = &text[pos + 1..close];
    if !inner.contains('@') {
        return None;
    }
    let mut items = Vec::new();
    for part in inner.split(';') {
        items.push(parse_item(part, ctx)?);
    }
    if items.is_empty() {
        return None;
    }
    Some(InlineMatch {
        node: InlineNode::CitationGroup(CitationGroup {
            items,
            raw: text[pos..close + 1].to_string(),
            mode: None,
        }),
        end: close + 1,
    })
}

fn close_bracket(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn parse_item(raw: &str, ctx: &MatcherContext<'_>) -> Option<Citation> {
    let trimmed = raw.trim();
    for (at, _) in trimmed.match_indices('@') {
        if at > 0 && trimmed.as_bytes().get(at - 1) == Some(&b'\\') {
            continue;
        }
        let (key, key_end) = parse_key(trimmed, at + 1)?;
        let rest = trimmed[key_end..].trim_start();
        let locator = if rest.is_empty() {
            None
        } else if let Some(loc) = rest.strip_prefix(',') {
            let loc = loc.trim();
            if loc.is_empty() {
                return None;
            }
            Some(ctx.parse_inlines(loc))
        } else {
            continue;
        };
        let suppress_author = at > 0 && trimmed.as_bytes()[at - 1] == b'-';
        let prefix_end = if suppress_author { at - 1 } else { at };
        let prefix_text = trimmed[..prefix_end].trim_end();
        let prefix = (!prefix_text.is_empty()).then(|| ctx.parse_inlines(prefix_text));
        return Some(Citation {
            key: key.to_string(),
            prefix,
            locator,
            suppress_author,
            number: None,
            label: None,
        });
    }
    None
}

fn parse_key(text: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let first = *bytes.get(start)?;
    if !(first.is_ascii_alphanumeric() || first == b'_') {
        return None;
    }
    let mut end = start + 1;
    while let Some(&b) = bytes.get(end) {
        if b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'_' | b':'
                    | b'.'
                    | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'+'
                    | b'?'
                    | b'<'
                    | b'>'
                    | b'~'
                    | b'/'
                    | b'-'
            )
        {
            end += 1;
        } else {
            break;
        }
    }
    Some((&text[start..end], end))
}

fn collect_defs(blocks: Vec<BlockNode>, defs: &mut BTreeMap<String, Def>) -> Vec<BlockNode> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            BlockNode::Paragraph(mut p) => {
                let original_children = p.children;
                let lines = split_on_soft_breaks(original_children);
                let mut kept = Vec::new();
                for line in lines {
                    if let Some((key, def)) = as_definition(&line) {
                        defs.insert(key, def);
                    } else {
                        kept.push(line);
                    }
                }
                if kept.is_empty() {
                    continue;
                }
                p.children = join_with_soft_breaks(kept);
                out.push(BlockNode::Paragraph(p));
            }
            BlockNode::List(mut l) => {
                for item in &mut l.items {
                    item.children = collect_defs(std::mem::take(&mut item.children), defs);
                }
                out.push(BlockNode::List(l));
            }
            BlockNode::BlockQuote(mut b) => {
                b.children = collect_defs(b.children, defs);
                out.push(BlockNode::BlockQuote(b));
            }
            BlockNode::Admonition(mut a) => {
                a.children = collect_defs(a.children, defs);
                out.push(BlockNode::Admonition(a));
            }
            BlockNode::Div(mut d) => {
                d.children = collect_defs(d.children, defs);
                out.push(BlockNode::Div(d));
            }
            other => out.push(other),
        }
    }
    out
}

fn split_on_soft_breaks(nodes: Vec<InlineNode>) -> Vec<Vec<InlineNode>> {
    let mut lines = vec![Vec::new()];
    for node in nodes {
        if matches!(node, InlineNode::SoftBreak) {
            lines.push(Vec::new());
        } else {
            lines.last_mut().unwrap().push(node);
        }
    }
    lines
}

fn join_with_soft_breaks(lines: Vec<Vec<InlineNode>>) -> Vec<InlineNode> {
    let mut out = Vec::new();
    for (idx, line) in lines.into_iter().enumerate() {
        if idx > 0 {
            out.push(InlineNode::SoftBreak);
        }
        out.extend(line);
    }
    out
}

fn as_definition(line: &[InlineNode]) -> Option<(String, Def)> {
    let InlineNode::CitationGroup(group) = line.first()? else {
        return None;
    };
    if group.items.len() != 1 {
        return None;
    }
    let item = &group.items[0];
    if item.prefix.is_some() || item.locator.is_some() || item.suppress_author {
        return None;
    }
    let InlineNode::Text(second) = line.get(1)? else {
        return None;
    };
    if !second.starts_with(':') {
        return None;
    }

    let mut entry = line[1..].to_vec();
    if let InlineNode::Text(head) = &mut entry[0] {
        *head = head.trim_start_matches(':').trim_start().to_string();
    }
    let mut def = Def {
        entry,
        author: None,
        year: None,
    };
    consume_leading_attrs(&mut def);
    Some((item.key.clone(), def))
}

fn consume_leading_attrs(def: &mut Def) {
    let Some(InlineNode::Text(head)) = def.entry.first_mut() else {
        return;
    };
    let Some(rest) = head.strip_prefix('{') else {
        return;
    };
    let Some(close) = rest.find('}') else {
        return;
    };
    let attrs = &rest[..close];
    def.author = attr_value(attrs, "author");
    def.year = attr_value(attrs, "year");
    *head = rest[close + 1..].trim_start().to_string();
    if head.is_empty() {
        def.entry.remove(0);
    }
}

fn attr_value(attrs: &str, key: &str) -> Option<String> {
    for token in attrs.split_whitespace() {
        let (k, v) = token.split_once('=')?;
        if k == key {
            return Some(v.trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

fn annotate_citations_block(
    block: &mut BlockNode,
    defs: &BTreeMap<String, Def>,
    mode: CitationMode,
    seen: &mut BTreeSet<String>,
    order: &mut Vec<String>,
) {
    match block {
        BlockNode::Heading(h) => {
            annotate_citations_inline(&mut h.children, defs, mode, seen, order)
        }
        BlockNode::Paragraph(p) => {
            annotate_citations_inline(&mut p.children, defs, mode, seen, order);
        }
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    annotate_citations_block(child, defs, mode, seen, order);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                annotate_citations_block(child, defs, mode, seen, order);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    annotate_citations_inline(&mut cell.children, defs, mode, seen, order);
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                annotate_citations_inline(title, defs, mode, seen, order);
            }
            for child in &mut a.children {
                annotate_citations_block(child, defs, mode, seen, order);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                annotate_citations_block(child, defs, mode, seen, order);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    annotate_citations_inline(term, defs, mode, seen, order);
                }
                for def_blocks in &mut item.definitions {
                    for child in def_blocks {
                        annotate_citations_block(child, defs, mode, seen, order);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            annotate_citations_inline(&mut f.caption, defs, mode, seen, order);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        annotate_citations_block(child, defs, mode, seen, order);
                    }
                }
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            annotate_citations_inline(&mut cell.children, defs, mode, seen, order);
                        }
                    }
                }
                FigureTarget::Paragraph(p) => {
                    annotate_citations_inline(&mut p.children, defs, mode, seen, order);
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        _ => {}
    }
}

fn annotate_citations_inline(
    nodes: &mut [InlineNode],
    defs: &BTreeMap<String, Def>,
    mode: CitationMode,
    seen: &mut BTreeSet<String>,
    order: &mut Vec<String>,
) {
    for node in nodes {
        match node {
            InlineNode::CitationGroup(g) => {
                g.mode = Some(mode.into());
                for item in &mut g.items {
                    let Some(def) = defs.get(&item.key) else {
                        continue;
                    };
                    if seen.insert(item.key.clone()) {
                        order.push(item.key.clone());
                    }
                    let number = order.iter().position(|key| key == &item.key).unwrap() + 1;
                    item.number = Some(number);
                    item.label = Some(match mode {
                        CitationMode::Numbered => number.to_string(),
                        CitationMode::AuthorDate => {
                            if item.suppress_author {
                                def.year.clone().unwrap_or_else(|| number.to_string())
                            } else {
                                let label = format!(
                                    "{} {}",
                                    def.author.as_deref().unwrap_or_default(),
                                    def.year.as_deref().unwrap_or_default()
                                )
                                .trim()
                                .to_string();
                                if label.is_empty() {
                                    number.to_string()
                                } else {
                                    label
                                }
                            }
                        }
                    });
                }
            }
            InlineNode::Emphasis(e) => {
                annotate_citations_inline(&mut e.children, defs, mode, seen, order)
            }
            InlineNode::Link(l) => {
                annotate_citations_inline(&mut l.children, defs, mode, seen, order)
            }
            InlineNode::Span(s) => {
                annotate_citations_inline(&mut s.children, defs, mode, seen, order)
            }
            InlineNode::Extension(e) => {
                annotate_citations_inline(&mut e.children, defs, mode, seen, order);
            }
            _ => {}
        }
    }
}

fn inject_references_block(blocks: &mut Vec<BlockNode>) {
    let carrier = BlockNode::Extension(BlockExtension {
        attrs: None,
        name: REFS_BLOCK.to_string(),
        children: Vec::new(),
        summary: None,
        label: None,
    });
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Div(d) if has_class(&d.attrs, "references") => {
                d.children.push(carrier);
                return;
            }
            BlockNode::Admonition(a) if a.kind == "references" => {
                a.children.push(carrier);
                return;
            }
            _ => {}
        }
    }
    blocks.push(carrier);
}

fn has_class(attrs: &Option<Attrs>, class: &str) -> bool {
    attrs
        .as_ref()
        .is_some_and(|attrs| attrs.classes.iter().any(|c| c == class))
}

fn render_refs_list(
    ctx: &RenderContext<'_>,
    mode: CitationMode,
    order: &[String],
    defs: &BTreeMap<String, Def>,
) -> String {
    let mut keys = order.to_vec();
    if mode == CitationMode::AuthorDate {
        keys.sort_by(|a, b| {
            let left = defs.get(a).and_then(|d| d.author.as_deref()).unwrap_or(a);
            let right = defs.get(b).and_then(|d| d.author.as_deref()).unwrap_or(b);
            left.cmp(right)
        });
    }
    let tag = if mode == CitationMode::AuthorDate {
        "ul"
    } else {
        "ol"
    };
    let mut out = format!("<{tag} class=\"references\">");
    for key in keys {
        if let Some(def) = defs.get(&key) {
            out.push('\n');
            out.push_str(&format!(
                "  <li id=\"ref-{}\">{}</li>",
                escape_attr(&key),
                ctx.render_inlines(&def.entry)
            ));
        }
    }
    out.push('\n');
    out.push_str(&format!("</{tag}>"));
    out
}
