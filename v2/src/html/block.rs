//! Block elements (C2-H-005 to C2-H-071, C2-H-133, C2-H-142, C2-H-154 to
//! C2-H-158).

use super::Renderer;
use super::attrs::El;
use super::esc;
use crate::ast::{self, Attrs, ListSpacing, Node, ParagraphRole};
use crate::options::Mode;
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
    // C2-H-019: a paragraph with no comment, as one hard break, is written.
    nodes.iter().any(|n| matches!(n, Node::Comment(_)))
        && nodes.iter().all(|n| match n {
            Node::Comment(_) | Node::SoftBreak(_) | Node::HardBreak(_) => true,
            Node::Text(t) => t.value.trim_matches([' ', '\t', '\n']).is_empty(),
            _ => false,
        })
}

fn has_attrs(attrs: Option<&Attrs<'_>>, lang: Option<&str>) -> bool {
    attrs.is_some_and(|a| !a.is_empty()) || lang.is_some()
}

impl<'r, 'a> Renderer<'r, 'a> {
    /// Renders one block at `depth`.
    pub(super) fn block(&mut self, n: &'r Node<'a>, depth: usize) {
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
            Node::Figure(_) => self.figure(n, depth, false),
            Node::FigureGroup(_) => self.figure_group(n, depth),
            Node::Include(i) => {
                let s = format!(
                    "<p{}>{}</p>",
                    self.el(i.attrs, i.lang, n).write(self.filter()),
                    esc::text_str(&include_text(i))
                );
                self.losses.push("include-unresolved");
                self.line(depth, &s);
            }
            Node::Section(s) => {
                let el = self.el(s.attrs, s.lang, n);
                let open = format!("<section{}>", el.write(self.filter()));
                self.container(&open, "</section>", Vec::new(), &*s.children, depth);
            }
            Node::BlockExtension(e) => {
                self.losses.push("extension-fallback-rendered");
                for c in e.fallback.iter() {
                    self.block(c, depth);
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
        n: &Node,
    ) -> El<'b> {
        let mut el = El::new(attrs, lang);
        if self.opts.source_lines {
            el.source_line = n.pos().map(|p| p.start_line);
        }
        el
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
            if comments_only(p.children) {
                return false;
            }
            self.losses.push("profile-construct-degraded");
            let t = esc::text_str(&super::profile::inlines_plain(&*p.children, self.src));
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
        if comments_only(p.children) {
            return false;
        }
        if p.role == Some(ParagraphRole::Image)
            && !has_attrs(p.attrs, p.lang)
            && self.image_block_bare(p.children)
        {
            self.leaf_into(p.children, out);
            return true;
        }
        let static_math = p.role == Some(ParagraphRole::Math) && self.opts.mode == Mode::Static;
        let mut el = self.el(p.attrs, p.lang, n);
        if static_math {
            out.push_str(&self.static_display_math(&*p.children, el));
            return true;
        }
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
    fn bare_image(&self, p: &ast::Paragraph) -> bool {
        p.role == Some(ParagraphRole::Image)
            && !has_attrs(p.attrs, p.lang)
            && self.image_block_bare(p.children)
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

    fn static_display_math(&mut self, children: &[Node], mut el: El<'_>) -> String {
        el.source_line = None;
        let value = children
            .iter()
            .find_map(|c| {
                if let Node::Math(m) = c {
                    Some(m.value)
                } else {
                    None
                }
            })
            .unwrap_or("");
        format!(
            "<pre{}><code class=\"language-math\">{}\n</code></pre>",
            el.write(self.filter()),
            esc::text_str(value)
        )
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
        let vis = self.shown(children);
        // C2-H-165 with C2-H-006: a block a profile degrades is a paragraph.
        if let [only] = vis.as_slice() {
            if let Some([p]) = self.degraded(only).as_deref() {
                self.losses.push("profile-construct-degraded");
                self.line(depth, &format!("{open}<p>{p}</p></blockquote>"));
                return;
            }
        }
        if let [only @ Node::Paragraph(p)] = vis.as_slice() {
            // A display math paragraph is a paragraph here; a bare image is not.
            if !self.bare_image(p) {
                if let Some(s) = self.paragraph(only, false) {
                    self.line(depth, &format!("{open}{s}</blockquote>"));
                    return;
                }
            }
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
            if self.profile_allows(item, true) {
                self.level += 1;
                self.item(item, depth + 1, tight, ordered);
                self.level -= 1;
            } else {
                // Gap HTML-007: an item past the bound keeps its `<li>` and degrades its blocks.
                self.losses.push("profile-construct-degraded");
                self.degrading += 1;
                self.item(item, depth + 1, tight, ordered);
                self.degrading -= 1;
            }
        }
        self.line(depth, if ordered { "</ol>" } else { "</ul>" });
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
        let vis = self.shown(i.children);
        let Some((first, rest)) = vis.split_first() else {
            self.line(depth, &format!("{open}</li>"));
            return;
        };
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
        // Gap G-016: a bare block image keeps its own line (C2-H-020).
        } else if matches!(first, Node::Paragraph(p) if !self.bare_image(p)) {
            let last = rest.is_empty();
            self.line_with(depth, |r, out| {
                out.push_str(&open);
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
            self.line(depth, &open);
            self.block(first, depth + 1);
        }
        for b in rest {
            self.block(b, depth + 1);
        }
        self.line(depth, "</li>");
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
            [p @ Node::Paragraph(_)] => {
                let s = self.paragraph(p, true).unwrap_or_default();
                self.line(depth, &format!("{open}{s}</dd>"));
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
        el.trailing.push("line-block".into());
        if self.opts.roundtrip {
            el.carriers.push(("data-carve-line-block", None));
        }
        let open = format!("<div{}>", el.write(self.filter()));
        let mut stanzas: Vec<Vec<String>> = vec![Vec::new()];
        for l in lb.children.iter() {
            let Node::Line(line) = l else { continue };
            if line.children.is_empty() {
                stanzas.push(Vec::new());
                continue;
            }
            let s = self.line_text(line.children);
            if let Some(last) = stanzas.last_mut() {
                last.push(s);
            }
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
        let s = self.leaf(children);
        let spaces = s.chars().take_while(|c| *c == ' ').count();
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
        let Node::Figure(f) = n else { return };
        let mut el = self.el(f.attrs, f.lang, n);
        if panel {
            el.base.push("carve-figure-panel".into());
        }
        let open = format!("<figure{}>", el.write(self.filter()));
        self.line(depth, &open);
        for c in f.children.iter() {
            self.block(c, depth + 1);
        }
        if let Some(c) = &f.caption {
            self.caption(c, "figcaption", depth + 1);
        }
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
            if let Node::Figure(_) = c {
                if self.profile_allows(c, true) {
                    self.level += 1;
                    self.figure(c, depth + 1, true);
                    self.level -= 1;
                    continue;
                }
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
