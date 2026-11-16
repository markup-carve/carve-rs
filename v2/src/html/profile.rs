//! Profiles (C2-H-129, C2-H-163 to C2-H-166) and the degraded forms.

use super::Renderer;
use super::block::visible;
use super::esc;
use crate::ast::Node;
use crate::tables::ProfileRow;

impl<'r, 'a> Renderer<'r, 'a> {
    fn profile_row(&self) -> Option<&'static ProfileRow> {
        self.profile
    }

    /// C2-H-163: the type on its axis, and the nesting bound of C2-H-166.
    pub(super) fn profile_allows(&self, n: &Node, block: bool) -> bool {
        let Some(row) = self.profile_row() else {
            return true;
        };
        if self.degrading > 0 {
            return false;
        }
        if row.max_nesting.is_some_and(|m| self.level > m) {
            return false;
        }
        allows(row, n.type_name(), block)
    }

    /// Whether a node at the current level stands past `max_nesting`.
    pub(super) fn past_bound(&self) -> bool {
        self.profile_row()
            .and_then(|r| r.max_nesting)
            .is_some_and(|m| self.level > m)
    }

    pub(super) fn profile_allows_type(&self, name: &str, block: bool) -> bool {
        self.profile_row()
            .is_none_or(|row| allows(row, name, block))
    }

    /// C2-H-165: a block outside the profile becomes paragraphs of its plain
    /// text behind the mark of its kind.
    pub(super) fn degrade_block(&mut self, n: &'r Node<'a>, depth: usize) {
        self.losses.push("profile-construct-degraded");
        for p in self.degraded(n).unwrap_or_default() {
            self.line(depth, &format!("<p>{p}</p>"));
        }
    }

    /// The children that render: visible, and not degraded to nothing.
    pub(super) fn shown(&self, children: &'r [Node<'a>]) -> Vec<&'r Node<'a>> {
        children
            .iter()
            .filter(|c| visible(c) && self.degraded(c).is_none_or(|p| !p.is_empty()))
            .collect()
    }

    /// The escaped paragraphs a block outside the profile leaves, none when
    /// it leaves no text (C2-H-165); `None` when the block is inside.
    pub(super) fn degraded(&self, n: &Node) -> Option<Vec<String>> {
        if matches!(n, Node::Paragraph(_)) || self.profile_allows(n, true) {
            return None;
        }
        let mut paras = Vec::new();
        degraded_paragraphs(n, &mut paras, 0, self.src);
        paras.retain(|p| !p.is_empty());
        Some(paras)
    }

    /// C2-H-165: an inline node outside the profile becomes its plain text.
    pub(super) fn degrade_inline(&mut self, n: &Node, out: &mut String) {
        if matches!(n, Node::Comment(_)) {
            return;
        }
        self.losses.push("profile-construct-degraded");
        let mut s = String::new();
        inline_plain(n, &mut s, 0, self.src);
        esc::text(&s, out);
    }
}

fn allows(row: &ProfileRow, ty: &str, block: bool) -> bool {
    let (deny, allow) = if block {
        (row.denied_block, row.allowed_block)
    } else {
        (row.denied_inline, row.allowed_inline)
    };
    if deny.contains(&ty) {
        return false;
    }
    allow.is_none_or(|a| a.contains(&ty))
}

/// The degraded text of an inline node (C2-H-165), unescaped.
fn inline_plain(n: &Node, out: &mut String, depth: usize, src: Option<&str>) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    if let Some(text) = src
        .and_then(|source| n.pos().and_then(|pos| source.get(pos.start()..pos.end())))
        .filter(|_| {
            matches!(
                n,
                Node::Code(_)
                    | Node::Math(_)
                    | Node::Autolink(_)
                    | Node::Xref(_)
                    | Node::Symbol(_)
                    | Node::Mention(_)
                    | Node::Tag(_)
                    | Node::CriticComment(_)
                    | Node::CitationGroup(_)
            )
        })
    {
        out.push_str(text);
        return;
    }
    if inline_plain_leaf(n, out) {
        return;
    }
    match n {
        Node::Xref(x) => {
            out.push_str("</#");
            out.push_str(x.key);
            out.push('>');
        }
        Node::Symbol(s) => {
            out.push(':');
            out.push_str(s.name);
            out.push(':');
        }
        Node::Mention(m) => {
            out.push('@');
            out.push_str(m.target);
        }
        Node::Tag(t) => {
            out.push('#');
            out.push_str(t.name);
        }
        Node::Abbreviation(a) => out.push_str(a.key),
        Node::CriticComment(c) => {
            out.push_str("{#");
            out.push_str(c.value);
            out.push_str("#}");
        }
        Node::CitationGroup(c) => out.push_str(c.raw),
        Node::CaptionNumber(c) => {
            out.push_str(&c.n.map_or_else(|| "#".to_string(), |n| n.to_string()));
        }
        other => {
            for c in other.children().into_iter().flatten() {
                inline_plain(c, out, depth + 1, src);
            }
        }
    }
}

fn inline_plain_leaf(n: &Node, out: &mut String) -> bool {
    match n {
        Node::Text(t) => out.push_str(t.value),
        Node::Image(i) => {
            out.push_str("[img: ");
            out.push_str(i.alt.unwrap_or(""));
            out.push(']');
        }
        Node::NonBreakingSpace(_) => out.push('\u{a0}'),
        Node::HardBreak(_) | Node::SoftBreak(_) => out.push(' '),
        Node::SmartPunctuation(s) => out.push_str(s.glyph.unwrap_or(s.source)),
        Node::FootnoteRef(r) => {
            out.push_str("[^");
            out.push_str(r.key);
            out.push(']');
        }
        Node::Comment(_) => {}
        Node::Code(c) => {
            out.push('`');
            out.push_str(c.value);
            out.push('`');
        }
        Node::Math(m) => {
            out.push_str(if m.display { "$$`" } else { "$`" });
            out.push_str(m.value);
            out.push('`');
        }
        Node::LiteralInline(l) => out.push_str(l.value),
        Node::RawInline(r) => out.push_str(r.value),
        Node::Autolink(a) => {
            out.push('<');
            out.push_str(a.text.unwrap_or(a.href));
            out.push('>');
        }
        _ => return false,
    }
    true
}

pub(super) fn inlines_plain(nodes: &[Node], src: Option<&str>) -> String {
    let mut s = String::new();
    for n in nodes {
        inline_plain(n, &mut s, 0, src);
    }
    s
}

/// C2-H-165: inside a degraded block, childless code and math give their values.
fn inlines_block_plain(nodes: &[Node]) -> String {
    fn append(n: &Node, out: &mut String, depth: usize) {
        if depth > crate::resolve::WALK_CAP {
            return;
        }
        match n {
            Node::Code(c) => out.push_str(c.value),
            Node::Math(m) => out.push_str(m.value),
            Node::LiteralInline(l) => out.push_str(l.value),
            other => {
                if let Some(children) = other.children().filter(|c| !c.is_empty()) {
                    for child in children {
                        append(child, out, depth + 1);
                    }
                } else {
                    inline_plain(other, out, depth, None);
                }
            }
        }
    }
    let mut out = String::new();
    for n in nodes {
        append(n, &mut out, 0);
    }
    out
}

/// The escaped paragraphs a degraded block gives.
fn degraded_paragraphs(n: &Node, out: &mut Vec<String>, depth: usize, src: Option<&str>) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    if let Some(text) = degraded_simple_text(n, src) {
        out.push(esc::text_str(&text));
        return;
    }
    let text = match n {
        Node::BlockQuote(q) => {
            let inner = block_text(&*q.children, src);
            if inner.is_empty() {
                return;
            }
            format!("> {inner}")
        }
        Node::Figure(_) | Node::FigureGroup(_) => {
            degraded_figure(n, out, depth, src);
            return;
        }
        Node::Table(t) => {
            out.push(table_text(t, src));
            if let Some(caption) = &t.caption {
                for block in caption.children().into_iter().flatten() {
                    degraded_paragraphs(block, out, depth + 1, src);
                }
            }
            return;
        }
        Node::DefinitionList(d) => {
            degraded_definitions(d, out, depth, src);
            return;
        }
        Node::Comment(_)
        | Node::Frontmatter(_)
        | Node::LinkReferenceDefinition(_)
        | Node::AbbreviationDef(_)
        | Node::Footnote(_) => return,
        other => match other.children() {
            Some(c) if c.iter().any(is_block) => {
                for b in c {
                    degraded_paragraphs(b, out, depth + 1, src);
                }
                return;
            }
            Some(c) => inlines_plain(c, src),
            None => String::new(),
        },
    };
    out.push(esc::text_str(&text));
}

fn degraded_figure(n: &Node, out: &mut Vec<String>, depth: usize, src: Option<&str>) {
    n.children()
        .into_iter()
        .flatten()
        .for_each(|b| degraded_paragraphs(b, out, depth + 1, src));
    let caption = crate::resolve::order::caption_of(n);
    caption
        .and_then(Node::children)
        .into_iter()
        .flatten()
        .for_each(|b| degraded_paragraphs(b, out, depth + 1, src));
}

fn degraded_definitions(
    d: &crate::ast::DefinitionList,
    out: &mut Vec<String>,
    depth: usize,
    src: Option<&str>,
) {
    for c in d.children.iter() {
        match c {
            Node::DefinitionTerm(t) => out.push(esc::text_str(&inlines_plain(&*t.children, src))),
            Node::DefinitionDescription(x) => x
                .children
                .iter()
                .for_each(|b| degraded_paragraphs(b, out, depth + 1, src)),
            _ => {}
        }
    }
}

fn degraded_simple_text(n: &Node, src: Option<&str>) -> Option<String> {
    Some(match n {
        Node::Heading(h) => format!(
            "{} {}",
            "#".repeat(usize::try_from(h.level).unwrap_or(1)),
            inlines_block_plain(&*h.children)
        ),
        Node::List(l) => {
            let items: Vec<String> = l
                .children
                .iter()
                .map(|i| format!("- {}", block_text(i.children().unwrap_or_default(), src)))
                .collect();
            items.join(" ")
        }
        Node::ThematicBreak(_) => "---".to_string(),
        // Gap HTML-016: the final line break of the value is not written.
        Node::CodeBlock(c) => c.value.strip_suffix('\n').unwrap_or(c.value).to_string(),
        Node::RawBlock(r) => r.value.strip_suffix('\n').unwrap_or(r.value).to_string(),
        Node::Div(_) | Node::Admonition(_) | Node::Directive(_) => {
            let (_, _, children, title, _) = super::block::div_parts(n);
            let mut parts: Vec<String> = Vec::new();
            if let Some(t) = title {
                parts.push(inlines_plain(t, src));
            }
            parts.push(block_text(children, src));
            parts.retain(|p| !p.is_empty());
            parts.join(" ")
        }
        Node::Include(i) => super::block::include_text(i),
        Node::Paragraph(p) => inlines_block_plain(&*p.children),
        // Leaves of inline content: an inline node with children is no block here.
        Node::CellParagraph(p) => inlines_block_plain(&*p.children),
        Node::Line(l) => inlines_block_plain(&*l.children),
        _ => return None,
    })
}

fn is_block(n: &Node) -> bool {
    !matches!(n.type_name(), "text" | "soft_break" | "hard_break")
        && n.children().is_some()
        && !n.type_name().ends_with("_inline")
}

/// The blocks of a container joined by single spaces.
fn block_text(children: &[Node], src: Option<&str>) -> String {
    let mut parts = Vec::new();
    for c in children {
        let mut p = Vec::new();
        degraded_paragraphs(c, &mut p, 1, src);
        parts.extend(p);
    }
    parts.retain(|p| !p.is_empty());
    let joined = parts.join(" ");
    // The parts are escaped already; the caller escapes again.
    joined
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{a0}")
}

/// One line per row, cells joined by ` | `, a hard break between the rows.
fn table_text(t: &crate::ast::Table, src: Option<&str>) -> String {
    let mut rows = Vec::new();
    for (r, _) in crate::resolve::table::rows(t) {
        let cells: Vec<String> = r
            .children()
            .into_iter()
            .flatten()
            .map(|c| esc::text_str(&block_text(c.children().unwrap_or_default(), src)))
            .collect();
        rows.push(cells.join(" | "));
    }
    rows.join("<br>\n")
}
