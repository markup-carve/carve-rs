//! Profiles (C2-H-129, C2-H-163 to C2-H-166) and the degraded forms.

use super::Renderer;
use super::block::visible;
use super::esc;
use crate::ast::Node;
use crate::source::SourceView;
use crate::tables::ProfileRow;

impl<'r, 'a> Renderer<'r, 'a> {
    pub(super) fn profile_loss(&mut self, n: &Node) {
        self.losses.push("profile-construct-degraded");
        self.profile_losses.push(super::ProfileLoss {
            pos: n.pos().copied(),
            construct: n.type_name(),
            file: self.file_of(n.pos()),
        });
    }
    pub(super) fn only_comments(&self, nodes: &[Node]) -> bool {
        super::block::comments_only_with_critic(
            nodes,
            !self.profile_allows_type("critic_comment", false),
        )
    }

    pub(super) fn empty_after_degrade(&self, nodes: &[Node]) -> bool {
        self.profile_row().is_some()
            && !nodes.is_empty()
            && nodes.iter().all(|node| {
                if self.profile_allows(node, false) {
                    return false;
                }
                let mut text = String::new();
                inline_plain(node, &mut text, 0);
                text.is_empty()
            })
    }

    // C2-H-129, C2-H-165: a degraded node still reports when it leaves no text.
    pub(super) fn report_empty_paragraph(&mut self, n: &Node) {
        if !self.profile_allows(n, true) {
            self.profile_loss(n);
        } else if let Node::Paragraph(p) = n {
            for child in p.children.iter() {
                if matches!(child, Node::CriticComment(_)) && !self.profile_allows(child, false) {
                    self.profile_loss(child);
                }
            }
        }
    }

    // C2-H-165: containers omit empty degraded children, but still report them.
    pub(super) fn report_suppressed(&mut self, children: &[Node]) {
        for child in children {
            if !visible(child) {
                continue;
            }
            if let Node::Paragraph(p) = child {
                if self.only_comments(p.children) {
                    self.report_empty_paragraph(child);
                }
            } else if self.degraded(child).is_some_and(|paras| paras.is_empty()) {
                self.profile_loss(child);
            }
        }
    }

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
        self.profile_loss(n);
        for p in self.degraded(n).unwrap_or_default() {
            self.line(depth, &format!("<p>{p}</p>"));
        }
    }

    pub(super) fn degrade_captioned(&mut self, n: &'r Node<'a>, depth: usize) {
        self.profile_loss(n);
        let mut paras = Vec::new();
        degraded_paragraphs(n, &mut paras, 0, self.src_of(n));
        for p in paras.into_iter().filter(|p| !p.is_empty()) {
            self.line(depth, &format!("<p>{p}</p>"));
        }
    }

    /// C2-N-074: a caption written as the paragraphs of a degraded block (C2-H-165).
    pub(super) fn degraded_caption(&mut self, caption: &Node<'_>, depth: usize) {
        let mut paras = Vec::new();
        for block in caption.children().into_iter().flatten() {
            degraded_paragraphs(block, &mut paras, 1, self.src_of(block));
        }
        for p in paras.into_iter().filter(|p| !p.is_empty()) {
            self.line(depth, &format!("<p>{p}</p>"));
        }
    }

    /// The children that render: visible, and not degraded to nothing.
    pub(super) fn shown(&self, children: &'r [Node<'a>]) -> Vec<&'r Node<'a>> {
        children
            .iter()
            .filter(|c| {
                visible(c)
                    && !matches!(c, Node::Paragraph(p) if self.only_comments(p.children))
                    && self.degraded(c).is_none_or(|p| !p.is_empty())
            })
            .collect()
    }

    /// The escaped paragraphs a block outside the profile leaves, none when
    /// it leaves no text (C2-H-165); `None` when the block is inside.
    pub(super) fn degraded(&self, n: &Node) -> Option<Vec<String>> {
        let figure = matches!(
            n,
            Node::Paragraph(_) | Node::CodeBlock(_) | Node::BlockQuote(_)
        ) && crate::resolve::order::caption_of(n).is_some();
        if figure {
            if self.profile_allows_type("figure", true) && !self.past_bound() {
                return None;
            }
        } else if matches!(n, Node::Paragraph(_)) || self.profile_allows(n, true) {
            return None;
        }
        let mut paras = Vec::new();
        degraded_paragraphs(n, &mut paras, 0, self.src_of(n));
        paras.retain(|p| !p.is_empty());
        Some(paras)
    }

    /// C2-H-165: an inline node outside the profile becomes its plain text.
    pub(super) fn degrade_inline(&mut self, n: &Node, out: &mut String) {
        if matches!(n, Node::Comment(_)) {
            return;
        }
        self.profile_loss(n);
        if let Node::Xref(x) = n {
            let target = x.href.and_then(|href| href.strip_prefix('#'));
            if let Some(text) = target.and_then(|id| self.xref_text.get(id)) {
                out.push_str(text);
                return;
            }
        }
        let mut s = String::new();
        inline_plain(n, &mut s, 0);
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
fn inline_plain(n: &Node, out: &mut String, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
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
        Node::CriticComment(_) => {}
        Node::CitationGroup(c) => out.push_str(c.raw),
        Node::CaptionNumber(c) => {
            out.push_str(&c.n.map_or_else(|| "#".to_string(), |n| n.to_string()));
        }
        other => {
            for c in other.children().into_iter().flatten() {
                inline_plain(c, out, depth + 1);
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
        Node::SmartPunctuation(s) => out.push_str(s.glyph),
        Node::FootnoteRef(r) => {
            out.push_str("[^");
            out.push_str(r.key);
            out.push(']');
        }
        Node::Comment(_) => {}
        Node::Code(c) => out.push_str(c.value),
        Node::Math(m) => out.push_str(m.value),
        Node::LiteralInline(l) => out.push_str(l.value),
        Node::RawInline(r) => out.push_str(r.value),
        Node::Autolink(a) => out.push_str(a.text.unwrap_or(a.href)),
        _ => return false,
    }
    true
}

#[cfg(test)]
fn code_spelling(value: &str, out: &mut String) {
    let runs: std::collections::HashSet<usize> = value
        .split(|c| c != '`')
        .map(str::len)
        .filter(|len| *len > 0)
        .collect();
    let mut tick_count = 1;
    while runs.contains(&tick_count) {
        tick_count += 1;
    }
    let ticks = "`".repeat(tick_count);
    let padded = value.starts_with('`')
        || value.ends_with('`')
        || (value.starts_with(' ') && value.ends_with(' ') && value.bytes().any(|b| b != b' '));
    out.push_str(&ticks);
    if padded {
        out.push(' ');
    }
    out.push_str(value);
    if padded {
        out.push(' ');
    }
    out.push_str(&ticks);
}

/// C2-H-165: inside a degraded block, childless code and math give their values.
fn inlines_block_plain(nodes: &[Node]) -> String {
    crate::resolve::plain::text_of_degraded_block(nodes)
}

/// The escaped paragraphs a degraded block gives.
fn degraded_paragraphs(n: &Node, out: &mut Vec<String>, depth: usize, src: Option<SourceView<'_>>) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    degraded_body(n, out, depth, src);
    if let Some(caption) = crate::resolve::order::caption_of(n) {
        for block in caption.children().into_iter().flatten() {
            degraded_paragraphs(block, out, depth + 1, src);
        }
    }
}

fn degraded_body(n: &Node, out: &mut Vec<String>, depth: usize, src: Option<SourceView<'_>>) {
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
        Node::FigureGroup(_) => {
            degraded_figure(n, out, depth, src);
            return;
        }
        Node::Table(t) => {
            out.push(table_text(t, src));
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
            Some(c) => inlines_block_plain(c),
            None => String::new(),
        },
    };
    out.push(esc::text_str(&text));
}

fn degraded_figure(n: &Node, out: &mut Vec<String>, depth: usize, src: Option<SourceView<'_>>) {
    n.children()
        .into_iter()
        .flatten()
        .for_each(|b| degraded_paragraphs(b, out, depth + 1, src));
}

fn degraded_definitions(
    d: &crate::ast::DefinitionList,
    out: &mut Vec<String>,
    depth: usize,
    src: Option<SourceView<'_>>,
) {
    for c in d.children.iter() {
        match c {
            Node::DefinitionTerm(t) => out.push(esc::text_str(&inlines_block_plain(&*t.children))),
            Node::DefinitionDescription(x) => x
                .children
                .iter()
                .for_each(|b| degraded_paragraphs(b, out, depth + 1, src)),
            _ => {}
        }
    }
}

fn degraded_simple_text(n: &Node, src: Option<SourceView<'_>>) -> Option<String> {
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
                .filter_map(|i| {
                    let text = block_text(i.children().unwrap_or_default(), src);
                    (!text.is_empty()).then(|| format!("- {text}"))
                })
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
                parts.push(inlines_block_plain(t));
            }
            parts.push(block_text(children, src));
            parts.retain(|p| !p.is_empty());
            parts.join(" ")
        }
        Node::Include(i) => super::block::include_text(i),
        Node::Paragraph(p) => inlines_block_plain(&*p.children),
        // A line block is one leaf block in a degraded profile. Empty lines
        // contribute nothing; each remaining line keeps its leading spaces.
        Node::LineBlock(lb) => block_text(lb.children, src),
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
fn block_text(children: &[Node], src: Option<SourceView<'_>>) -> String {
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
fn table_text(t: &crate::ast::Table, src: Option<SourceView<'_>>) -> String {
    let mut rows = Vec::new();
    for (r, _) in crate::resolve::table::rows(t) {
        let cells: Vec<String> = r
            .children()
            .into_iter()
            .flatten()
            .filter(|c| !matches!(c, Node::TableCell(cell) if cell.span.is_some()))
            .map(|c| esc::text_str(&block_text(c.children().unwrap_or_default(), src)))
            .collect();
        rows.push(cells.join(" | "));
    }
    rows.join("<br>\n")
}

#[cfg(test)]
mod code_spelling_tests {
    use super::code_spelling;

    #[test]
    fn c2_w_077_one_sided_space_needs_no_padding() {
        let mut out = String::new();
        code_spelling(" a", &mut out);
        assert_eq!(out, "` a`");
    }

    #[test]
    fn c2_w_077_uses_shortest_absent_backtick_run() {
        let mut out = String::new();
        code_spelling("a`b```c", &mut out);
        assert_eq!(out, "``a`b```c``");
    }
}
