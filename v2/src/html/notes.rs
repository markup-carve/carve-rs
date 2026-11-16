//! The footnote section (C2-H-089 to C2-H-094, C2-H-158, C2-H-169).

use super::Renderer;
use super::attrs::El;
use super::block::{attr_str, visible};
use crate::ast::{Node, ParagraphRole};
use crate::resolve::order::addr;

impl<'r, 'a> Renderer<'r, 'a> {
    /// C2-H-158, C2-H-169: the directive's blocks, then the section with its
    /// title and label.
    pub(super) fn placing_directive(&mut self, n: &'r Node<'a>, depth: usize) {
        let Node::Directive(d) = n else { return };
        for c in d.children.iter() {
            self.block(c, depth);
        }
        let id = self.adm_ids.get(&addr(n)).cloned();
        let first = self.title_lines(d.title.as_deref(), d.group_label, id.clone());
        self.footnote_section(depth, Some((first, id)));
    }

    /// The section at `depth`, with the title lines and id of a placing
    /// directive when one places it.
    pub(super) fn footnote_section(
        &mut self,
        depth: usize,
        head: Option<(Vec<String>, Option<String>)>,
    ) {
        let (first, id) = head.unwrap_or_default();
        let mut el = El::default();
        el.structural.push(("role", Some("doc-endnotes".into())));
        match id {
            Some(id) => el.generated.push(("aria-labelledby", Some(id.into()))),
            None => el
                .generated
                .push(("aria-label", Some(self.opts.label("endnotes").into()))),
        }
        let open = format!("<section{}>", el.write(self.filter()));
        self.line(depth, &open);
        for f in first {
            self.line(depth + 1, &f);
        }
        self.line(depth + 1, "<hr>");
        self.line(depth + 1, "<ol>");
        let saved = self.level;
        self.level = 1;
        for i in 0..self.order.notes.len() {
            if self.shown.get(i) == Some(&true) {
                self.note(i, depth + 2);
            }
        }
        self.level = saved;
        self.line(depth + 1, "</ol>");
        self.line(depth, "</section>");
    }

    fn note(&mut self, i: usize, depth: usize) {
        let Some(note) = self.order.notes.get(i) else {
            return;
        };
        let def = note.node;
        let back = self.backlinks(i);
        let mut el = El {
            id: Some(format!("fn{}", i + 1).into()),
            id_first: true,
            ..El::default()
        };
        if self.opts.roundtrip {
            match def {
                Node::Footnote(f) => el
                    .carriers
                    .push(("data-carve-footnote-label", Some(f.key.into()))),
                _ => el.carriers.push(("data-carve-inline-footnote", None)),
            }
        }
        self.line(depth, &format!("<li{}>", el.write(self.filter())));
        match def {
            Node::Footnote(f) => self.note_body(&*f.children, &back, depth + 1),
            Node::InlineFootnote(f) => {
                let inner = self.leaf(f.children);
                self.line(depth + 1, &format!("<p>{inner}{back}</p>"));
            }
            _ => {}
        }
        self.line(depth, "</li>");
    }

    /// C2-H-090, C2-H-091: every paragraph gets `<p>`; the backlinks join the
    /// last paragraph or stand in one of their own.
    fn note_body(&mut self, children: &'r [Node<'a>], back: &str, depth: usize) {
        let last = children.iter().rposition(visible);
        let ends_in_paragraph = last.and_then(|i| children.get(i)).is_some_and(
            |c| matches!(c, Node::Paragraph(p) if p.role != Some(ParagraphRole::Image)),
        );
        for (i, c) in children.iter().enumerate() {
            if ends_in_paragraph && Some(i) == last {
                if let Some(s) = self.paragraph(c, false) {
                    let body = s.strip_suffix("</p>").unwrap_or(&s);
                    self.line(depth, &format!("{body}{back}</p>"));
                }
            } else {
                self.block(c, depth);
            }
        }
        if !ends_in_paragraph {
            self.line(depth, &format!("<p>{back}</p>"));
        }
    }

    fn backlinks(&self, i: usize) -> String {
        let Some(note) = self.order.notes.get(i) else {
            return String::new();
        };
        let label = self.opts.label("footnoteBacklink");
        let many = note.refs.len() > 1;
        let mut parts = Vec::new();
        for (k, r) in note.refs.iter().enumerate() {
            let id = self.mark_ids.get(&addr(r)).cloned().unwrap_or_default();
            let (aria, sup) = if many {
                (
                    format!("{label} {}", k + 1),
                    format!("<sup>{}</sup>", k + 1),
                )
            } else {
                (label.to_string(), String::new())
            };
            parts.push(format!(
                "<a href=\"#{}\" role=\"doc-backlink\" aria-label=\"{}\">\u{21a9}{sup}</a>",
                attr_str(&id),
                attr_str(&aria)
            ));
        }
        parts.join(" ")
    }
}
