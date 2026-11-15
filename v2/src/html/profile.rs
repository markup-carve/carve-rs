//! Profiles (C2-H-129, C2-H-163 to C2-H-166) and the degraded forms.

use super::Renderer;
use super::esc;
use crate::ast::Node;
use crate::tables::{PROFILES, ProfileRow};

impl<'a> Renderer<'a> {
    fn profile_row(&self) -> Option<&'static ProfileRow> {
        PROFILES.iter().find(|p| p.name == self.opts.profile.name())
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

    pub(super) fn profile_allows_type(&self, name: &str, block: bool) -> bool {
        self.profile_row()
            .is_none_or(|row| allows(row, name, block))
    }

    /// C2-H-165: a block outside the profile becomes paragraphs of its plain
    /// text behind the mark of its kind.
    pub(super) fn degrade_block(&mut self, n: &'a Node, depth: usize) {
        self.losses.push("profile-construct-degraded");
        let mut paras = Vec::new();
        degraded_paragraphs(n, &mut paras, 0);
        for p in paras.into_iter().filter(|p| !p.is_empty()) {
            self.line(depth, &format!("<p>{p}</p>"));
        }
    }

    /// C2-H-165: an inline node outside the profile becomes its plain text.
    pub(super) fn degrade_inline(&mut self, n: &Node, out: &mut String) {
        if matches!(n, Node::Comment(_)) {
            return;
        }
        self.losses.push("profile-construct-degraded");
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
    match n {
        Node::Text(t) => out.push_str(&t.value),
        Node::Image(i) => {
            out.push_str("[img: ");
            out.push_str(i.alt.as_deref().unwrap_or(""));
            out.push(']');
        }
        Node::NonBreakingSpace(_) => out.push('\u{a0}'),
        Node::HardBreak(_) | Node::SoftBreak(_) => out.push(' '),
        Node::SmartPunctuation(s) => out.push_str(s.glyph.as_deref().unwrap_or(&s.source)),
        Node::FootnoteRef(r) => {
            out.push_str("[^");
            out.push_str(&r.key);
            out.push(']');
        }
        Node::Comment(_) => {}
        Node::Code(c) => {
            out.push('`');
            out.push_str(&c.value);
            out.push('`');
        }
        Node::Math(m) => {
            out.push_str(if m.display { "$$`" } else { "$`" });
            out.push_str(&m.value);
            out.push('`');
        }
        Node::LiteralInline(l) => out.push_str(&l.value),
        Node::RawInline(r) => out.push_str(&r.value),
        Node::Autolink(a) => {
            out.push('<');
            out.push_str(a.text.as_deref().unwrap_or(&a.href));
            out.push('>');
        }
        Node::Xref(x) => {
            out.push_str("</#");
            out.push_str(&x.key);
            out.push('>');
        }
        Node::Symbol(s) => {
            out.push(':');
            out.push_str(&s.name);
            out.push(':');
        }
        Node::Mention(m) => {
            out.push('@');
            out.push_str(&m.target);
        }
        Node::Tag(t) => {
            out.push('#');
            out.push_str(&t.name);
        }
        Node::Abbreviation(a) => out.push_str(&a.key),
        Node::CriticComment(c) => {
            out.push_str("{#");
            out.push_str(&c.value);
            out.push_str("#}");
        }
        Node::CitationGroup(c) => out.push_str(&c.raw),
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

pub(super) fn inlines_plain(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        inline_plain(n, &mut s, 0);
    }
    s
}

/// The escaped paragraphs a degraded block gives.
fn degraded_paragraphs(n: &Node, out: &mut Vec<String>, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    let text = match n {
        Node::Heading(h) => format!(
            "{} {}",
            "#".repeat(usize::try_from(h.level).unwrap_or(1)),
            inlines_plain(&h.children)
        ),
        Node::BlockQuote(q) => format!("> {}", block_text(&q.children)),
        Node::List(l) => {
            let items: Vec<String> = l
                .children
                .iter()
                .map(|i| format!("- {}", block_text(i.children().map_or(&[], Vec::as_slice))))
                .collect();
            items.join(" ")
        }
        Node::ThematicBreak(_) => "---".to_string(),
        // Gap HTML-016: the final line break of the value is not written.
        Node::CodeBlock(c) => c.value.strip_suffix('\n').unwrap_or(&c.value).to_string(),
        Node::RawBlock(r) => r.value.strip_suffix('\n').unwrap_or(&r.value).to_string(),
        Node::Figure(_) | Node::FigureGroup(_) => {
            n.children()
                .into_iter()
                .flatten()
                .for_each(|b| degraded_paragraphs(b, out, depth + 1));
            let caption = crate::resolve::order::caption_of(n);
            caption
                .and_then(Node::children)
                .into_iter()
                .flatten()
                .for_each(|b| degraded_paragraphs(b, out, depth + 1));
            return;
        }
        Node::Table(t) => return out.push(table_text(t)),
        Node::Div(_) | Node::Admonition(_) | Node::Directive(_) => {
            let (_, _, children, title, _) = super::block::div_parts(n);
            let mut parts: Vec<String> = Vec::new();
            if let Some(t) = title {
                parts.push(inlines_plain(t));
            }
            parts.push(block_text(children));
            parts.retain(|p| !p.is_empty());
            parts.join(" ")
        }
        Node::Include(i) => super::block::include_text(i),
        Node::Paragraph(p) => inlines_plain(&p.children),
        Node::DefinitionList(d) => {
            for c in &d.children {
                match c {
                    Node::DefinitionTerm(t) => out.push(esc::text_str(&inlines_plain(&t.children))),
                    Node::DefinitionDescription(x) => x
                        .children
                        .iter()
                        .for_each(|b| degraded_paragraphs(b, out, depth + 1)),
                    _ => {}
                }
            }
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
                    degraded_paragraphs(b, out, depth + 1);
                }
                return;
            }
            Some(c) => inlines_plain(c),
            None => String::new(),
        },
    };
    out.push(esc::text_str(&text));
}

fn is_block(n: &Node) -> bool {
    !matches!(n.type_name(), "text" | "soft_break" | "hard_break")
        && n.children().is_some()
        && !n.type_name().ends_with("_inline")
}

/// The blocks of a container joined by single spaces.
fn block_text(children: &[Node]) -> String {
    let mut parts = Vec::new();
    for c in children {
        let mut p = Vec::new();
        degraded_paragraphs(c, &mut p, 1);
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
fn table_text(t: &crate::ast::Table) -> String {
    let mut rows = Vec::new();
    for (r, _) in crate::resolve::table::rows(t) {
        let cells: Vec<String> = r
            .children()
            .into_iter()
            .flatten()
            .map(|c| esc::text_str(&block_text(c.children().map_or(&[], Vec::as_slice))))
            .collect();
        rows.push(cells.join(" | "));
    }
    rows.join("<br>\n")
}
