//! Block elements (C2-H-005 to C2-H-071, C2-H-133, C2-H-142, C2-H-154 to
//! C2-H-158).

use super::Renderer;
use super::attrs::El;
use super::esc;
use crate::ast::{self, Attrs, ListSpacing, Node, ParagraphRole};
use crate::resolve::order::addr;
use crate::tables::TASK_STATES;
use std::borrow::Cow;

/// True when a block writes something (C2-H-008, C2-H-019, C2-H-070, C2-H-071).
pub(super) fn visible(n: &Node) -> bool {
    match n {
        Node::Comment(_)
        | Node::Frontmatter(_)
        | Node::LinkReferenceDefinition(_)
        | Node::AbbreviationDef(_)
        | Node::Footnote(_)
        | Node::CitationDefinition(_) => false,
        Node::Paragraph(p) => !comments_only(p.children),
        _ => true,
    }
}

/// A node list that holds only comments, breaks and blank text (C2-H-019).
pub(super) fn comments_only(nodes: &[Node]) -> bool {
    comments_only_with_critic(nodes, false)
}

pub(super) fn comments_only_with_critic(nodes: &[Node], critic: bool) -> bool {
    let comment =
        |n: &Node| matches!(n, Node::Comment(_)) || (critic && matches!(n, Node::CriticComment(_)));
    // C2-H-019: a paragraph with no comment, as one hard break, is written.
    nodes.iter().any(comment)
        && nodes.iter().all(|n| match n {
            Node::Comment(_) | Node::SoftBreak(_) | Node::HardBreak(_) => true,
            Node::CriticComment(_) => critic,
            Node::Text(t) => t.value.trim_matches([' ', '\t', '\n']).is_empty(),
            _ => false,
        })
        && hard_breaks_dropped(nodes, &comment)
}

/// C2-H-069, C2-H-019: every hard break is one a comment-only line takes
/// with it: the break behind it, or the break before it at the end.
fn hard_breaks_dropped(nodes: &[Node], comment: &dyn Fn(&Node) -> bool) -> bool {
    let mut lines: Vec<(bool, Option<bool>)> = Vec::new();
    let mut has_comment = false;
    for n in nodes {
        match n {
            Node::SoftBreak(_) | Node::HardBreak(_) => {
                lines.push((has_comment, Some(matches!(n, Node::HardBreak(_)))));
                has_comment = false;
            }
            _ => has_comment |= comment(n),
        }
    }
    lines.push((has_comment, None));
    let last = lines.len() - 1;
    (0..last).all(|i| {
        let hard = lines.get(i).and_then(|l| l.1) == Some(true);
        let behind = lines.get(i).is_some_and(|l| l.0);
        let before_last = i + 1 == last && lines.get(last).is_some_and(|l| l.0);
        !hard || behind || before_last
    })
}

fn has_attrs(attrs: Option<&Attrs<'_>>, lang: Option<&str>) -> bool {
    attrs.is_some_and(|a| !a.is_empty()) || lang.is_some()
}

impl<'r, 'a> Renderer<'r, 'a> {
    /// Renders one block at `depth`.
    pub(super) fn block(&mut self, n: &'r Node<'a>, depth: usize) {
        if matches!(
            n,
            Node::Paragraph(_) | Node::CodeBlock(_) | Node::BlockQuote(_)
        ) && crate::resolve::order::caption_of(n).is_some()
        {
            if !self.profile_allows_type("figure", true) || self.past_bound() {
                self.degrade_captioned(n, depth);
            } else {
                self.figure(n, depth, false);
            }
            self.long_descriptions(depth);
            return;
        }
        if matches!(n, Node::Paragraph(_)) {
            self.line_with(depth, |r, out| r.paragraph_into(n, false, out));
            self.long_descriptions(depth);
            return;
        }
        if !self.profile_allows(n, true) {
            self.degrade_block(n, depth);
            return;
        }
        if self.level > crate::resolve::WALK_CAP {
            return;
        }
        self.level += 1;
        self.block_inner(n, depth);
        self.level -= 1;
        self.long_descriptions(depth);
    }

    /// C2-H-080: the long descriptions of the images of a block, after it.
    fn long_descriptions(&mut self, depth: usize) {
        for d in std::mem::take(&mut self.long_desc) {
            for b in d {
                self.block(b, depth);
            }
        }
    }

    fn block_inner(&mut self, n: &'r Node<'a>, depth: usize) {
        match n {
            Node::Paragraph(_) => {
                self.line_with(depth, |r, out| r.paragraph_into(n, false, out));
            }
            Node::Heading(_) => self.heading(n, depth, true),
            Node::List(_) => self.list(n, depth),
            Node::DefinitionList(_) => self.definition_list(n, depth),
            Node::BlockQuote(q) => self.quote(n, &*q.children, depth),
            Node::Div(_) | Node::Admonition(_) | Node::Directive(_) => self.div_like(n, depth),
            Node::LineBlock(_) => self.line_block(n, depth),
            Node::CodeBlock(_) => self.code_block(n, depth),
            Node::RawBlock(_) => self.raw_block(n, depth),
            _ => self.block_inner_other(n, depth),
        }
    }

    fn block_inner_other(&mut self, n: &'r Node<'a>, depth: usize) {
        match n {
            Node::ThematicBreak(t) => {
                let el = self.el(t.attrs, t.lang, n);
                let s = format!("<hr{}>", el.write(self.filter()));
                self.line(depth, &s);
            }
            Node::Table(_) => self.table(n, depth, false),
            Node::FigureGroup(_) => self.figure_group(n, depth),
            // C2-N-072: an include's own attributes are not written.
            Node::Include(i) => {
                let s = format!(
                    "<p{}>{}</p>",
                    self.el(None, None, n).write(self.filter()),
                    esc::text_str(&include_text(i))
                );
                self.losses.push("include-unresolved");
                self.line(depth, &s);
                if let Some(caption) = i.caption.as_deref().filter(|_| i.children.is_none()) {
                    self.degraded_caption(caption, depth);
                }
            }
            Node::Section(s) => {
                let el = self.el(s.attrs, s.lang, n);
                let open = format!("<section{}>", el.write(self.filter()));
                self.container(&open, "</section>", Vec::new(), &*s.children, depth);
            }
            Node::BlockExtension(e) => {
                match self.hooked(n, true, None, depth) {
                    Some(true) => return,
                    Some(false) => {}
                    None => self.losses.push("extension-fallback-rendered"),
                }
                let wrapped = e.attrs.is_some_and(|attrs| !attrs.is_empty());
                if wrapped {
                    let el = self.el(e.attrs, e.lang, n);
                    self.line(depth, &format!("<div{}>", el.write(self.filter())));
                }
                for c in e.fallback.iter() {
                    self.block(c, depth + usize::from(wrapped));
                }
                if wrapped {
                    self.line(depth, "</div>");
                }
            }
            _ => {}
        }
    }

    /// The element of a block: its attributes, language and source line.
    pub(super) fn el<'b>(
        &self,
        attrs: Option<&'b Attrs>,
        lang: Option<&'b str>,
        n: &'b Node<'a>,
    ) -> El<'b> {
        let mut el = if self.caption_inner == Some(crate::resolve::order::addr(n)) {
            El::new(None, None)
        } else {
            El::new(attrs, lang)
        };
        if self.caption_inner != Some(crate::resolve::order::addr(n)) {
            el.dir = n.dir();
        }
        if self.opts.source_lines {
            el.source_line = if self.caption_inner == Some(crate::resolve::order::addr(n)) {
                self.caption_inner_line(n)
            } else {
                n.pos().map(|p| p.start_line)
            };
        }
        el
    }

    /// C2-H-062: the inner block starts after its attribute and comment lines.
    fn caption_inner_line(&self, n: &Node) -> Option<u32> {
        let pos = n.pos()?;
        let skipped = self
            .src_of(n)
            .and_then(|source| source.slice(*pos))
            .map_or(0, |text| {
                text.lines()
                    .take_while(|line| {
                        let trimmed = line.trim_start_matches([' ', '\t']);
                        trimmed.starts_with('{') || trimmed.starts_with("%%")
                    })
                    .count()
            });
        Some(
            pos.start_line
                .saturating_add(u32::try_from(skipped).unwrap_or(u32::MAX)),
        )
    }

    /// The line of a paragraph, bare or wrapped; `None` when it writes nothing.
    /// A paragraph counts one nesting level (C2-H-166) wherever its container
    /// writes it; outside the profile it is its plain text (C2-H-165).
    pub(super) fn paragraph(&mut self, n: &'r Node<'a>, bare: bool) -> Option<String> {
        let mut s = String::new();
        self.paragraph_into(n, bare, &mut s).then_some(s)
    }

    /// `paragraph`, appended to `out`; false when it writes nothing.
    pub(super) fn paragraph_into(&mut self, n: &'r Node<'a>, bare: bool, out: &mut String) -> bool {
        let Node::Paragraph(p) = n else { return false };
        if !self.profile_allows(n, true) {
            if self.only_comments(p.children) {
                self.report_empty_paragraph(n);
                return false;
            }
            self.losses.push("profile-construct-degraded");
            let t = esc::text_str(&crate::resolve::plain::text_of(&*p.children));
            if t.is_empty() {
                return false;
            }
            if bare {
                out.push_str(&t);
            } else {
                out.push_str("<p>");
                out.push_str(&t);
                out.push_str("</p>");
            }
            return true;
        }
        self.level += 1;
        let s = self.paragraph_inner(n, bare, out);
        self.level -= 1;
        s
    }

    fn paragraph_inner(&mut self, n: &'r Node<'a>, bare: bool, out: &mut String) -> bool {
        let Node::Paragraph(p) = n else { return false };
        if self.only_comments(p.children) {
            self.report_empty_paragraph(n);
            return false;
        }
        if self.empty_after_degrade(p.children) {
            self.losses.push("profile-construct-degraded");
            return false;
        }
        if p.role == Some(ParagraphRole::Image) && self.image_block_bare(p.children) {
            if has_attrs(p.attrs, p.lang)
                && self.caption_inner != Some(crate::resolve::order::addr(n))
            {
                for child in p.children.iter() {
                    if let Node::Image(image) = child {
                        self.image(image, p.attrs, p.lang, out);
                    }
                }
            } else {
                self.leaf_into(p.children, out);
            }
            return true;
        }
        let mut el = self.el(p.attrs, p.lang, n);
        if bare && !has_attrs(p.attrs, p.lang) {
            self.leaf_into(p.children, out);
            return true;
        }
        el.align_style = true;
        out.push_str("<p");
        el.write_into(self.filter(), out);
        out.push('>');
        self.leaf_into(&*p.children, out);
        out.push_str("</p>");
        true
    }

    /// A paragraph written as a bare `<img>` (C2-H-020).
    pub(super) fn bare_image(&self, p: &ast::Paragraph) -> bool {
        p.role == Some(ParagraphRole::Image) && self.image_block_bare(p.children)
    }

    /// C2-H-020: a block image is written without `<p>` unless it renders as
    /// the text of an unresolved reference.
    fn image_block_bare(&self, children: &[Node]) -> bool {
        children.iter().all(|c| match c {
            Node::Image(i) => i.src.is_some() || i.key.is_none(),
            Node::Comment(_) | Node::SoftBreak(_) => true,
            _ => false,
        }) && self.profile_allows_type("image", false)
    }

    /// C2-H-021, C2-H-027, C2-H-028. `own_id` is false for a heading that
    /// opened a section, whose id stands on the section.
    pub(super) fn heading(&mut self, n: &'r Node<'a>, depth: usize, own_id: bool) {
        let Node::Heading(h) = n else { return };
        let mut el = self.el(h.attrs, h.lang, n);
        el.align_style = true;
        el.id = if own_id {
            super::heading_id(n).map(Cow::Borrowed)
        } else {
            None
        };
        if own_id {
            self.id_carriers(n, &mut el);
        }
        let tag = match h.level {
            1 => "h1",
            2 => "h2",
            3 => "h3",
            4 => "h4",
            5 => "h5",
            6 => "h6",
            _ => {
                el.structural.push(("role", Some("heading".into())));
                el.structural
                    .push(("aria-level", Some(h.level.to_string().into())));
                "div"
            }
        };
        let f = self.filter();
        self.line_with(depth, |r, out| {
            out.push('<');
            out.push_str(tag);
            el.write_into(f, out);
            out.push('>');
            r.leaf_into(h.children, out);
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
            true
        });
    }

    /// Opens `open`, writes `first` lines and the blocks one level deeper,
    /// closes with `close`; an empty body keeps one empty line (C2-H-008).
    pub(super) fn container(
        &mut self,
        open: &str,
        close: &str,
        first: Vec<String>,
        children: &'r [Node<'a>],
        depth: usize,
    ) {
        self.line(depth, open);
        let before = self.out.len();
        for f in first {
            self.line(depth + 1, &f);
        }
        for c in children {
            self.block(c, depth + 1);
        }
        // C2-H-008: judged on the output, as a raw block of another format writes nothing.
        if self.out.len() == before {
            self.blank();
        }
        self.line(depth, close);
    }

    /// C2-H-006, C2-H-041.
    fn quote(&mut self, n: &'r Node<'a>, children: &'r [Node<'a>], depth: usize) {
        let Node::BlockQuote(q) = n else { return };
        let el = self.el(q.attrs, q.lang, n);
        let open = format!("<blockquote{}>", el.write(self.filter()));
        self.report_suppressed(children);
        let vis = self.shown(children);
        // C2-H-165 with C2-H-006: a block a profile degrades is a paragraph.
        if let [only] = vis.as_slice()
            && crate::resolve::order::caption_of(only).is_none()
        {
            if let Some([p]) = self.degraded(only).as_deref() {
                self.losses.push("profile-construct-degraded");
                self.line(depth, &format!("{open}<p>{p}</p></blockquote>"));
                return;
            }
        }
        if let [only @ Node::Paragraph(p)] = vis.as_slice() {
            // A display math paragraph is a paragraph here; a bare image is not.
            if crate::resolve::order::caption_of(only).is_none() && !self.bare_image(p) {
                if let Some(s) = self.paragraph(only, false) {
                    self.line(depth, &format!("{open}{s}</blockquote>"));
                    return;
                }
            }
        }
        if let [Node::Include(i)] = vis.as_slice()
            && i.caption.is_none()
        {
            self.losses.push("include-unresolved");
            let text = esc::text_str(&include_text(i));
            self.line(depth, &format!("{open}<p>{text}</p></blockquote>"));
            return;
        }
        self.container(&open, "</blockquote>", Vec::new(), children, depth);
    }

    /// C2-H-029 to C2-H-037.
    fn list(&mut self, n: &'r Node<'a>, depth: usize) {
        let Node::List(l) = n else { return };
        let ordered = l.marker.kind == "ordered";
        let mut el = self.el(l.attrs, l.lang, n);
        if ordered {
            if let Some(t) = l.marker.style.and_then(html_type) {
                el.structural.push(("type", Some(t.into())));
            }
            if let Some(s) = l.marker.start.filter(|s| *s != 1) {
                el.structural.push(("start", Some(s.to_string().into())));
            }
            if l.marker.delimiter == Some(")") {
                el.structural.push(("data-delim", Some(")".into())));
            }
        }
        if self.opts.roundtrip {
            if let (false, Some(c)) = (ordered, l.marker.bullet.filter(|c| *c != "-")) {
                el.carriers.push(("data-marker", Some(c.into())));
            }
            if l.spacing == ListSpacing::Tight {
                el.carriers.push(("data-carve-tight", None));
            }
        }
        if l.children
            .iter()
            .any(|i| matches!(i, Node::ListItem(i) if i.task.is_some()))
        {
            el.base.push("task-list".into());
        }
        let tag = if ordered { "ol" } else { "ul" };
        let f = self.filter();
        self.line_with(depth, |_, out| {
            out.push('<');
            out.push_str(tag);
            el.write_into(f, out);
            out.push('>');
            true
        });
        let tight = l.spacing == ListSpacing::Tight;
        for item in l.children.iter() {
            self.list_item_at_depth(item, depth, tight, ordered);
        }
        self.line(depth, if ordered { "</ol>" } else { "</ul>" });
    }

    fn list_item_at_depth(&mut self, item: &'r Node<'a>, depth: usize, tight: bool, ordered: bool) {
        if !self.show_item(item) {
            if let Node::ListItem(i) = item {
                let outside = !self.profile_allows(item, true);
                self.level += 1;
                self.degrading += usize::from(outside);
                self.report_suppressed(i.children);
                self.degrading -= usize::from(outside);
                self.level -= 1;
            }
            return;
        }
        if self.profile_allows(item, true) {
            self.level += 1;
            self.item(item, depth + 1, tight, ordered);
            self.level -= 1;
        } else {
            // Gap HTML-007: an item past the bound keeps its `<li>` and degrades its blocks.
            self.degrading += 1;
            self.item(item, depth + 1, tight, ordered);
            self.degrading -= 1;
        }
    }

    fn show_item(&self, n: &'r Node<'a>) -> bool {
        if self.profile_allows(n, true) {
            return true;
        }
        let Node::ListItem(i) = n else { return true };
        i.task.is_some() || !self.shown(i.children).is_empty()
    }

    /// C2-H-007, C2-H-032 to C2-H-035.
    fn item(&mut self, n: &'r Node<'a>, depth: usize, tight: bool, ordered: bool) {
        let Node::ListItem(i) = n else { return };
        let mut el = self.el(i.attrs, i.lang, n);
        let head = Self::item_task(i, ordered, &mut el);
        let mut open = String::from("<li");
        el.write_into(self.filter(), &mut open);
        open.push('>');
        open.push_str(&head);
        self.report_suppressed(i.children);
        let vis = self.shown(i.children);
        let Some((first, rest)) = vis.split_first() else {
            self.line(depth, &format!("{open}</li>"));
            return;
        };
        self.item_visible(first, rest, &open, depth, tight);
    }

    fn item_visible(
        &mut self,
        first: &'r Node<'a>,
        rest: &[&'r Node<'a>],
        open: &str,
        depth: usize,
        tight: bool,
    ) {
        if self.item_captioned_include(first, rest, open, depth, tight) {
            return;
        }
        // C2-H-165 with C2-H-007: a block a profile degrades is paragraphs.
        if let Some(paras) = self.degraded(first) {
            self.losses.push("profile-construct-degraded");
            let mut it = paras.iter();
            let head = it.next().map_or_else(String::new, |p| {
                if tight {
                    p.clone()
                } else {
                    format!("<p>{p}</p>")
                }
            });
            if rest.is_empty() && paras.len() < 2 {
                self.line(depth, &format!("{open}{head}</li>"));
                return;
            }
            self.line(depth, &format!("{open}{head}"));
            for p in it {
                self.line(depth + 1, &format!("<p>{p}</p>"));
            }
        } else if let Node::Include(i) = first {
            if self.item_include(i, open, depth, tight, rest.is_empty()) {
                return;
            }
        // Gap G-016: a bare block image keeps its own line (C2-H-020).
        } else if matches!(first, Node::Paragraph(p) if crate::resolve::order::caption_of(first).is_none() && !self.bare_image(p))
        {
            let last = rest.is_empty();
            self.line_with(depth, |r, out| {
                out.push_str(open);
                r.paragraph_into(first, tight, out);
                if last {
                    out.push_str("</li>");
                }
                true
            });
            if last {
                return;
            }
        } else {
            self.line(depth, open);
            self.block(first, depth + 1);
        }
        self.item_rest(rest, depth);
    }

    fn item_rest(&mut self, rest: &[&'r Node<'a>], depth: usize) {
        for block in rest {
            self.block(block, depth + 1);
        }
        self.line(depth, "</li>");
    }

    fn item_captioned_include(
        &mut self,
        first: &'r Node<'a>,
        rest: &[&'r Node<'a>],
        open: &str,
        depth: usize,
        tight: bool,
    ) -> bool {
        let Node::Include(include) = first else {
            return false;
        };
        if include.children.is_some() || include.caption.is_none() {
            return false;
        }
        self.item_include(include, open, depth, tight, false);
        for block in rest {
            self.block(block, depth + 1);
        }
        self.line(depth, "</li>");
        true
    }

    fn item_include(
        &mut self,
        include: &'r ast::Include<'a>,
        open: &str,
        depth: usize,
        tight: bool,
        only: bool,
    ) -> bool {
        self.losses.push("include-unresolved");
        let body = esc::text_str(&include_text(include));
        if let Some(caption) = include.caption.as_deref() {
            self.line(depth, open);
            self.line(depth + 1, &format!("<p>{body}</p>"));
            self.degraded_caption(caption, depth + 1);
            return false;
        }
        let head = if tight && include.caption.is_none() {
            body
        } else {
            format!("<p>{body}</p>")
        };
        if only && include.caption.is_none() {
            self.line(depth, &format!("{open}{head}</li>"));
            return true;
        }
        self.line(depth, &format!("{open}{head}"));
        false
    }

    fn item_task(i: &ast::ListItem, ordered: bool, el: &mut El<'_>) -> String {
        let Some(task) = &i.task else {
            return String::new();
        };
        let state = TASK_STATES.iter().find(|s| s.1.to_string() == task.mark);
        if let Some(v) = state.and_then(|s| s.2) {
            el.structural
                .push(("data-task-state", Some(v.to_string().into())));
        }
        el.base.push("task".into());
        if !ordered {
            el.style_before = Some("list-style: none".into());
        }
        Self::task_box(&*i.children, state.is_some_and(|s| s.3))
    }

    /// The checkbox of a task item (C2-H-032, C2-H-174).
    fn task_box(children: &[Node], checked: bool) -> String {
        let mut texts = Vec::new();
        for c in children {
            let children = match c {
                Node::Paragraph(n) => Some(&n.children),
                Node::Heading(n) => Some(&n.children),
                _ => None,
            };
            if let Some(children) = children {
                let t = crate::resolve::plain::text_of(children);
                if !t.is_empty() {
                    texts.push(t);
                }
            }
        }
        let label = texts.join(" ");
        let mut s = String::from("<input type=\"checkbox\"");
        if checked {
            s.push_str(" checked");
        }
        s.push_str(" disabled");
        if !label.is_empty() {
            s.push_str(" aria-label=\"");
            esc::attr(&label, &mut s);
            s.push('"');
        }
        s.push_str("> ");
        s
    }

    /// C2-H-038 to C2-H-040.
    fn definition_list(&mut self, n: &'r Node<'a>, depth: usize) {
        let Node::DefinitionList(d) = n else { return };
        let el = self.el(d.attrs, d.lang, n);
        let open = format!("<dl{}>", el.write(self.filter()));
        self.line(depth, &open);
        for c in d.children.iter() {
            self.level += 1;
            match c {
                Node::DefinitionTerm(t) => {
                    let el = self.el(t.attrs, t.lang, c);
                    let inner = self.leaf(t.children);
                    let s = format!("<dt{}>{inner}</dt>", el.write(self.filter()));
                    self.line(depth + 1, &s);
                }
                Node::DefinitionDescription(x) => self.description(c, &*x.children, depth + 1),
                _ => {}
            }
            self.level -= 1;
        }
        self.line(depth, "</dl>");
    }

    fn description(&mut self, n: &'r Node<'a>, children: &'r [Node<'a>], depth: usize) {
        let Node::DefinitionDescription(x) = n else {
            return;
        };
        let el = self.el(x.attrs, x.lang, n);
        let open = format!("<dd{}>", el.write(self.filter()));
        let vis: Vec<&'r Node<'a>> = children.iter().filter(|c| visible(c)).collect();
        match vis.as_slice() {
            [] => self.line(depth, &format!("{open}</dd>")),
            [p @ Node::Paragraph(_)]
                if crate::resolve::order::caption_of(p).is_none()
                    && !matches!(p, Node::Paragraph(x) if self.bare_image(x)) =>
            {
                let s = self.paragraph(p, true).unwrap_or_default();
                self.line(depth, &format!("{open}{s}</dd>"));
            }
            [Node::Include(i)] if i.caption.is_none() => {
                self.losses.push("include-unresolved");
                let text = esc::text_str(&include_text(i));
                self.line(depth, &format!("{open}{text}</dd>"));
            }
            _ => {
                self.line(depth, &open);
                for c in children {
                    self.block(c, depth + 1);
                }
                self.line(depth, "</dd>");
            }
        }
    }

    /// Divs, admonitions and directives (C2-H-042, C2-H-154 to C2-H-158).
    fn div_like(&mut self, n: &'r Node<'a>, depth: usize) {
        if self.placing == Some(addr(n)) {
            self.placing_directive(n, depth);
            return;
        }
        let (attrs, lang, children, title, label) = div_parts(n);
        let mut el = self.el(attrs, lang, n);
        let tag = match n {
            Node::Admonition(a) => {
                el.base = vec!["admonition".into(), a.kind.as_str().into()];
                if !super::authored_name(attrs) {
                    match self.adm_ids.get(&addr(n)) {
                        Some(id) => el
                            .generated
                            .push(("aria-labelledby", Some(id.clone().into()))),
                        None if title.is_none() => {
                            let key = format!("admonition{}", capitalize(a.kind.as_str()));
                            el.generated
                                .push(("aria-label", Some(self.opts.label(&key).into())));
                        }
                        None => {}
                    }
                }
                "aside"
            }
            Node::Directive(d) => {
                el.base = vec![d.kind.as_str().into()];
                "div"
            }
            _ => {
                el.align_style = true;
                // Gap G-027: as in the oracle, `data-source-line` counts as another attribute.
                let line = self.opts.source_lines && n.pos().is_some();
                if self.opts.roundtrip && attrs.is_none_or(only_reserved) && lang.is_none() && !line
                {
                    el.carriers.push(("data-carve-div", None));
                }
                "div"
            }
        };
        let first = self.title_lines(title, label, self.adm_ids.get(&addr(n)).cloned());
        let open = format!("<{tag}{}>", el.write(self.filter()));
        self.container(&open, &format!("</{tag}>"), first, children, depth);
    }

    /// The title and label paragraphs of a div (C2-H-154).
    pub(super) fn title_lines(
        &mut self,
        title: Option<&'r [Node<'a>]>,
        label: Option<&str>,
        id: Option<String>,
    ) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(t) = title {
            let inner = self.leaf(t);
            let id = id
                .map(|i| format!(" id=\"{}\"", attr_str(&i)))
                .unwrap_or_default();
            out.push(format!("<p class=\"admonition-title\"{id}>{inner}</p>"));
        }
        if let Some(l) = label {
            out.push(format!("<p class=\"div-label\">{}</p>", esc::text_str(l)));
        }
        out
    }

    /// C2-H-043, C2-H-136: one `<p>` per stanza.
    fn line_block(&mut self, n: &'r Node<'a>, depth: usize) {
        let Node::LineBlock(lb) = n else { return };
        let mut el = self.el(lb.attrs, lb.lang, n);
        el.align_style = true;
        el.trailing.push("line-block".into());
        if self.opts.roundtrip {
            el.carriers.push(("data-carve-line-block", None));
        }
        let open = format!("<div{}>", el.write(self.filter()));
        let mut stanzas: Vec<Vec<String>> = vec![Vec::new()];
        let mut previous_hard = false;
        for l in lb.children.iter() {
            let Node::Line(line) = l else { continue };
            if line.children.is_empty() {
                stanzas.push(Vec::new());
                previous_hard = false;
                continue;
            }
            let s = self.line_text(line.children);
            if let Some(last) = stanzas.last_mut() {
                if previous_hard
                    && let Some(previous) = last.last_mut()
                    && previous.ends_with("<br>\n")
                {
                    previous.truncate(previous.len() - 5);
                }
                last.push(s);
            }
            previous_hard = matches!(line.children.last(), Some(Node::HardBreak(_)));
        }
        let paras: Vec<String> = stanzas
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| format!("<p>{}</p>", s.join("<br>\n")))
            .collect();
        self.line(depth, &open);
        if paras.is_empty() {
            self.blank();
        }
        for p in paras {
            self.line(depth + 1, &p);
        }
        self.line(depth, "</div>");
    }

    /// One line of a line block: leading spaces become `&nbsp;`.
    fn line_text(&mut self, children: &'r [Node<'a>]) -> String {
        // C2-B-130: only spaces the line starts with lead it, not those behind a comment.
        let lead = match children.first() {
            Some(Node::Text(t)) => t.value.bytes().take_while(|&b| b == b' ').count(),
            _ => 0,
        };
        let s = self.leaf(children);
        let spaces = s.bytes().take(lead).take_while(|&b| b == b' ').count();
        format!(
            "{}{}",
            "&nbsp;".repeat(spaces),
            s.get(spaces..).unwrap_or("")
        )
    }

    /// C2-H-047, C2-H-048.
    pub(super) fn code_block(&mut self, n: &'r Node<'a>, depth: usize) {
        let Node::CodeBlock(c) = n else { return };
        let mut el = self.el(c.attrs, c.lang, n);
        if let Some(t) = &c.title {
            el.structural
                .push(("title", Some(crate::resolve::plain::text_of(t).into())));
            el.authored_wins = &["title"];
        }
        if self.opts.roundtrip {
            if let Some(l) = &c.group_label {
                el.carriers.push(("data-carve-label", Some((*l).into())));
            }
        }
        let class = c
            .syntax
            .map(|s| format!(" class=\"language-{}\"", attr_str(s)))
            .unwrap_or_default();
        let s = format!(
            "<pre{}><code{class}>{}</code></pre>",
            el.write(self.filter()),
            esc::text_str(c.value)
        );
        self.line(depth, &s);
    }

    /// C2-H-050, C2-H-053, C2-H-054, C2-H-134.
    fn raw_block(&mut self, n: &'r Node<'a>, depth: usize) {
        let Node::RawBlock(r) = n else { return };
        if r.format != "html" {
            self.losses.push("raw-format-dropped");
            self.raw_losses.push(super::RawLoss {
                kind: "block",
                format: r.format.to_string(),
                pos: r.pos,
                file: self.file_of(r.pos.as_ref()),
            });
            return;
        }
        if self.opts.raw_html {
            let v = r.value.strip_suffix('\n').unwrap_or(r.value);
            self.line(0, v);
            return;
        }
        let s = format!(
            "<pre{}><code class=\"language-html\">{}</code></pre>",
            self.el(r.attrs, r.lang, n).write(self.filter()),
            esc::text_str(r.value)
        );
        self.line(depth, &s);
    }

    /// C2-H-062 to C2-H-064.
    pub(super) fn figure(&mut self, n: &'r Node<'a>, depth: usize, panel: bool) {
        let Some(c) = crate::resolve::order::caption_of(n) else {
            return;
        };
        let lang = match n {
            Node::Paragraph(p) => p.lang,
            Node::CodeBlock(c) => c.lang,
            Node::BlockQuote(q) => q.lang,
            _ => None,
        };
        let mut el = self.el(n.attrs(), lang, n);
        if panel {
            el.base.push("carve-figure-panel".into());
        }
        let open = format!("<figure{}>", el.write(self.filter()));
        self.line(depth, &open);
        self.caption_inner = Some(crate::resolve::order::addr(n));
        if matches!(n, Node::Paragraph(_)) {
            self.line_with(depth + 1, |r, out| r.paragraph_into(n, false, out));
        } else {
            self.block_inner(n, depth + 1);
        }
        self.caption_inner = None;
        self.caption(c, "figcaption", depth + 1);
        self.line(depth, "</figure>");
    }

    fn figure_group(&mut self, n: &'r Node<'a>, depth: usize) {
        let Node::FigureGroup(g) = n else { return };
        let mut el = self.el(g.attrs, g.lang, n);
        el.base.push("carve-figure-group".into());
        let open = format!("<figure{}>", el.write(self.filter()));
        self.line(depth, &open);
        let any = g.children.iter().any(visible) || g.caption.is_some();
        for c in g.children.iter() {
            if matches!(
                c,
                Node::Paragraph(_) | Node::CodeBlock(_) | Node::BlockQuote(_)
            ) && crate::resolve::order::caption_of(c).is_some()
                && self.profile_allows(c, true)
            {
                self.level += 1;
                self.figure(c, depth + 1, true);
                self.level -= 1;
                continue;
            }
            if let Node::Table(_) = c {
                if self.profile_allows(c, true) {
                    self.level += 1;
                    let mut panel = self.el(None, None, c);
                    panel.base.push("carve-figure-panel".into());
                    self.line(
                        depth + 1,
                        &format!("<figure{}>", panel.write(self.filter())),
                    );
                    self.table(c, depth + 2, false);
                    self.line(depth + 1, "</figure>");
                    self.level -= 1;
                    continue;
                }
            }
            self.block(c, depth + 1);
        }
        if let Some(c) = &g.caption {
            self.caption(c, "figcaption", depth + 1);
        }
        if !any {
            self.blank();
        }
        self.line(depth, "</figure>");
    }

    /// C2-H-065: one paragraph is written bare inside the caption element.
    pub(super) fn caption(&mut self, c: &'r Node<'a>, tag: &str, depth: usize) {
        let Node::Caption(cap) = c else { return };
        let vis: Vec<&'r Node<'a>> = cap.children.iter().filter(|x| visible(x)).collect();
        if let [p @ Node::Paragraph(_)] = vis.as_slice() {
            let s = self.paragraph(p, true).unwrap_or_default();
            self.line(depth, &format!("<{tag}>{s}</{tag}>"));
            return;
        }
        self.line(depth, &format!("<{tag}>"));
        for b in cap.children.iter() {
            self.block(b, depth + 1);
        }
        self.line(depth, &format!("</{tag}>"));
    }
}

/// The parts of a div-like node.
type DivParts<'r, 'a> = (
    Option<&'a Attrs<'a>>,
    Option<&'a str>,
    &'r [Node<'a>],
    Option<&'r [Node<'a>]>,
    Option<&'a str>,
);

pub(super) fn div_parts<'r, 'a>(n: &'r Node<'a>) -> DivParts<'r, 'a> {
    match n {
        Node::Div(d) => (
            d.attrs,
            d.lang,
            &*d.children,
            d.title.as_deref(),
            d.group_label,
        ),
        Node::Admonition(d) => (
            d.attrs,
            d.lang,
            &*d.children,
            d.title.as_deref(),
            d.group_label,
        ),
        Node::Directive(d) => (
            d.attrs,
            d.lang,
            &*d.children,
            d.title.as_deref(),
            d.group_label,
        ),
        _ => (None, None, &[], None, None),
    }
}

/// A div that writes no attribute once the reserved names are dropped
/// (C2-H-114 `data-carve-div`, C2-H-116).
fn only_reserved(a: &Attrs) -> bool {
    a.id.is_none()
        && a.classes.is_empty()
        && a.key_values.iter().all(|(k, _)| {
            super::attrs::RESERVED
                .iter()
                .any(|r| r.eq_ignore_ascii_case(k))
        })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

pub(super) fn attr_str(s: &str) -> String {
    let mut out = String::new();
    esc::attr(s, &mut out);
    out
}

/// The `type` of an ordered list style (C2-H-030, `ordered-styles.json`).
fn html_type(style: &str) -> Option<&'static str> {
    crate::tables::ORDERED_STYLES
        .iter()
        .find(|(s, _)| *s == style)
        .and_then(|(_, t)| *t)
}

/// The directive text of an unexpanded include (C2-H-133, C2-H-171).
pub(super) fn include_text(i: &ast::Include) -> String {
    let mut s = String::from("{{ ");
    s.push_str(&quoted_path(i.path));
    if let Some(sec) = &i.section {
        s.push_str(" #");
        s.push_str(sec);
    }
    if let Some(serde_json::Value::Object(opts)) = &i.options {
        let mut keys: Vec<(&String, &serde_json::Value)> = opts.iter().collect();
        keys.sort_by_key(|(k, _)| {
            crate::tables::INCLUDE_OPTIONS
                .iter()
                .find(|o| o.1 == k.as_str())
                .map_or(u64::MAX, |o| o.0)
        });
        for (k, v) in keys {
            s.push_str(" @");
            s.push_str(k);
            if let Some(v) = include_value(k, v) {
                s.push(':');
                s.push_str(&v);
            }
        }
    }
    if let Some(invalid) = i.invalid {
        for option in invalid {
            s.push(' ');
            s.push_str(option);
        }
    }
    s.push_str(" }}");
    s
}

fn quoted_path(p: &str) -> String {
    if p.chars()
        .any(|c| matches!(c, ' ' | '#' | '@' | '}' | '"' | '\u{201C}'))
    {
        format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        p.to_string()
    }
}

fn include_value(key: &str, v: &serde_json::Value) -> Option<String> {
    let raw = match v {
        serde_json::Value::Bool(true) => return None,
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(a) if key == "cols" => {
            let names: Vec<String> = a
                .iter()
                .filter_map(|x| x.as_str())
                .map(|x| {
                    x.replace('\\', "\\\\")
                        .replace(',', "\\,")
                        .replace('"', "\\\"")
                })
                .collect();
            return Some(format!("\"{}\"", names.join(",")));
        }
        other => other.to_string(),
    };
    let stored_quotes =
        key == "cols" && raw.starts_with('"') && raw.ends_with('"') && raw.len() > 1;
    if !stored_quotes
        && (raw.chars().any(|c| matches!(c, ' ' | '\t' | '@' | '}'))
            || raw.starts_with(['"', '\'']))
    {
        return Some(format!(
            "\"{}\"",
            raw.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }
    Some(raw)
}
