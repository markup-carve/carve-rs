//! CodeCallouts (#88, Tier-2). `<n>` markers at the end of fenced-code lines
//! render as `<b class="callout">` bubbles, and an immediately following
//! paragraph of `<n> text` lines binds as `<ol class="callouts">`. carve-rs has
//! no per-node block render hook, so (like `glossary` / `list-table`) a
//! `before_render` transform rewrites a code-block-with-markers into a
//! [`BlockNode::Extension`] carrier and the bound paragraph into another, both
//! rendered by [`CarveExtension::render_block_extension`]. Off by default;
//! optional-corpus pinned when enabled. See docs/extensions.md §10.

use crate::ast::{Attrs, BlockExtension, BlockNode, CodeBlock, Document, InlineNode, Paragraph};
use crate::escape::escape_attr;
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext};
use crate::render::render_attrs;

const CODE_CARRIER: &str = "code-callouts-code";
const LIST_CARRIER: &str = "code-callouts-list";

/// The CodeCallouts Tier-2 extension. Off by default; enable per processor.
#[derive(Debug, Default, Clone)]
pub struct CodeCallouts;

impl CodeCallouts {
    pub fn new() -> Self {
        Self
    }
}

impl CarveExtension for CodeCallouts {
    fn name(&self) -> &'static str {
        "codeCallouts"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        bind_blocks(&mut doc.children);
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        match node.name.as_str() {
            CODE_CARRIER => Some(render_code(node, ctx)),
            LIST_CARRIER => Some(render_list(node, ctx)),
            _ => None,
        }
    }
}

// ----- before_render: wrap code-with-markers + the bound list ------------------

fn bind_blocks(blocks: &mut [BlockNode]) {
    for b in blocks.iter_mut() {
        descend(b);
    }
    let mut i = 0;
    while i < blocks.len() {
        let has_marker =
            matches!(&blocks[i], BlockNode::CodeBlock(c) if content_has_marker(&c.content));
        if has_marker {
            let bind_list = matches!(blocks.get(i + 1), Some(BlockNode::Paragraph(p)) if is_callout_candidate(p));
            let code = std::mem::replace(&mut blocks[i], placeholder());
            blocks[i] = carrier(CODE_CARRIER, code);
            if bind_list {
                let para = std::mem::replace(&mut blocks[i + 1], placeholder());
                blocks[i + 1] = carrier(LIST_CARRIER, para);
                i += 1;
            }
        }
        i += 1;
    }
}

fn descend(b: &mut BlockNode) {
    match b {
        BlockNode::BlockQuote(q) => bind_blocks(&mut q.children),
        BlockNode::Div(d) => bind_blocks(&mut d.children),
        BlockNode::Admonition(a) => bind_blocks(&mut a.children),
        BlockNode::Extension(e) => bind_blocks(&mut e.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                bind_blocks(&mut item.children);
            }
        }
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for def in &mut item.definitions {
                    bind_blocks(def);
                }
            }
        }
        _ => {}
    }
}

fn placeholder() -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: Vec::new(),
        ..Default::default()
    })
}

fn carrier(name: &str, inner: BlockNode) -> BlockNode {
    BlockNode::Extension(BlockExtension {
        attrs: None,
        name: name.to_string(),
        children: vec![inner],
        summary: None,
        label: None,
    })
}

// ----- marker / item recognition ----------------------------------------------

/// A `<n>` (ASCII digits) that is the last non-whitespace content on `line`.
/// Returns `(prefix, whitespace_before_marker, digits)`.
fn parse_marker(line: &str) -> Option<(&str, &str, &str)> {
    let trimmed = line.trim_end_matches([' ', '\t']);
    let inner = trimmed.strip_suffix('>')?;
    let open = inner.rfind('<')?;
    let digits = &inner[open + 1..];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let before = &inner[..open];
    let prefix = before.trim_end_matches([' ', '\t']);
    let ws = &before[prefix.len()..];
    Some((prefix, ws, digits))
}

fn content_has_marker(content: &str) -> bool {
    content.lines().any(|l| parse_marker(l).is_some())
}

/// `<n> ` at the very start of a line's first text node → the marker number.
fn item_number(line: &[InlineNode]) -> Option<String> {
    let InlineNode::Text(t) = line.first()? else {
        return None;
    };
    let rest = t.strip_prefix('<')?;
    let gt = rest.find('>')?;
    let digits = &rest[..gt];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if rest.as_bytes().get(gt + 1) != Some(&b' ') {
        return None;
    }
    Some(digits.to_string())
}

fn is_callout_candidate(p: &Paragraph) -> bool {
    let lines = split_lines(&p.children);
    !lines.is_empty() && lines.iter().all(|line| item_number(line).is_some())
}

// ----- render -----------------------------------------------------------------

fn render_code(node: &BlockExtension, ctx: &RenderContext<'_>) -> String {
    let Some(BlockNode::CodeBlock(c)) = node.children.first() else {
        // Another code-block transformer (MathBlock / FencedRender) registered
        // before us may have replaced the carrier's inner CodeBlock; render
        // whatever it became so the content is never dropped.
        return ctx.render_blocks_at(&node.children, ctx.level());
    };
    let pad = ctx.indent(ctx.level());
    let body = c
        .content
        .split('\n')
        .map(|line| match parse_marker(line) {
            Some((prefix, ws, n)) => format!(
                "{}{}<b class=\"callout\" data-callout=\"{}\">{}</b>",
                ctx.escape_html(prefix),
                ws,
                n,
                n
            ),
            None => ctx.escape_html(line),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let title = code_title_attr(c);
    let lang = c
        .lang
        .as_ref()
        .map(|l| format!(" class=\"language-{l}\""))
        .unwrap_or_default();
    format!(
        "{pad}<pre{title}{attrs}><code{lang}>{body}\n</code></pre>",
        attrs = render_attrs(&c.attrs)
    )
}

/// Mirror the core code renderer: an opener `header` is carried on `title` and
/// emitted as a `title="..."` attribute unless the author already set one.
fn code_title_attr(c: &CodeBlock) -> String {
    match &c.title {
        Some(title)
            if !c
                .attrs
                .as_ref()
                .is_some_and(|a| a.key_values.keys().any(|k| k.eq_ignore_ascii_case("title"))) =>
        {
            format!(" title=\"{}\"", escape_attr(title))
        }
        _ => String::new(),
    }
}

fn render_list(node: &BlockExtension, ctx: &RenderContext<'_>) -> String {
    let Some(BlockNode::Paragraph(p)) = node.children.first() else {
        return String::new();
    };
    let pad = ctx.indent(ctx.level());
    let inner = ctx.indent(ctx.level() + 1);
    let items = split_lines(&p.children)
        .into_iter()
        .map(|line| {
            let n = item_number(&line).unwrap_or_default();
            let mut rest: Vec<InlineNode> = Vec::with_capacity(line.len());
            if let Some(InlineNode::Text(t)) = line.first() {
                rest.push(InlineNode::Text(strip_item_prefix(t)));
            }
            rest.extend(line.iter().skip(1).cloned());
            format!(
                "{}<li value=\"{}\">{}</li>",
                inner,
                n,
                ctx.render_inlines(&rest)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{pad}<ol{attrs}>\n{items}\n{pad}</ol>",
        attrs = render_attrs(&Some(with_base_class(&p.attrs, "callouts")))
    )
}

/// Strip a leading `<n> ` from the first text node.
fn strip_item_prefix(t: &str) -> String {
    if let Some(rest) = t.strip_prefix('<') {
        if let Some(gt) = rest.find('>') {
            return rest[gt + 2..].to_string();
        }
    }
    t.to_string()
}

fn with_base_class(attrs: &Option<Attrs>, base: &str) -> Attrs {
    let mut a = attrs.clone().unwrap_or_default();
    a.classes.insert(0, base.to_string());
    a
}

// ----- helpers ----------------------------------------------------------------

/// Split an inline run into per-line segments at each soft-break (dropping
/// empties).
fn split_lines(nodes: &[InlineNode]) -> Vec<Vec<InlineNode>> {
    let mut lines: Vec<Vec<InlineNode>> = vec![Vec::new()];
    for n in nodes {
        if matches!(n, InlineNode::SoftBreak) {
            lines.push(Vec::new());
        } else {
            lines.last_mut().unwrap().push(n.clone());
        }
    }
    lines.retain(|l| !l.is_empty());
    lines
}
