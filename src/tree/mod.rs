//! The tree builder: block events to the typed AST 2.0, calling the inline
//! pass for each content (C2-I-001).

mod delimited;
mod table;
pub(crate) mod view;

use crate::Arena;
use crate::ast::{self, Node, owned::Attrs};
use crate::attr::AttrBlock;
use crate::block::events::{ContainerKind, Event, LeafKind, LineBlockEntry, Role};
use crate::block::shapes::{DivInfo, MarkerKind};
use crate::chars::fold_label;
use crate::inline::{self, ContentKind, Join, Piece};
use crate::source::Source;
use std::collections::HashSet;

/// Builds the document from the events of the block pass.
#[must_use]
pub fn build<'a>(
    src: &Source<'a>,
    arena: &'a Arena,
    events: Vec<Event>,
    _own_starts: &[(usize, usize)],
) -> ast::Document<'a> {
    build_scoped(src, arena, events, None).0
}

/// The definition keys a file's inline pass sees: its own, then those of the
/// files that include it (C2-N-023, C2-N-054).
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub link_keys: HashSet<String>,
    /// Sorted by key; a term the file defines keeps its own expansion.
    pub abbreviations: Vec<(String, String)>,
    /// Findings of the tree pass (C2-B-181); never passed on to another file.
    pub findings: Vec<crate::block::Finding>,
}

/// `build` with the scope of the including files; returns the scope of this file.
#[must_use]
pub fn build_scoped<'a>(
    src: &Source<'a>,
    arena: &'a Arena,
    events: Vec<Event>,
    outer: Option<&Scope>,
) -> (ast::Document<'a>, Scope) {
    let (doc, mut scope) = build_layered(
        src,
        arena,
        events,
        outer.map(|o| &o.link_keys),
        outer.map_or(&[], |o| &o.abbreviations),
    );
    if let Some(outer) = outer {
        scope.link_keys.extend(outer.link_keys.iter().cloned());
        let own: HashSet<String> = scope.abbreviations.iter().map(|(k, _)| k.clone()).collect();
        scope.abbreviations.extend(
            outer
                .abbreviations
                .iter()
                .filter(|(k, _)| !own.contains(k))
                .cloned(),
        );
        scope.abbreviations.sort_by(|a, b| a.0.cmp(&b.0));
    }
    (doc, scope)
}

/// `build` with the link keys and abbreviations (sorted by term) of the
/// documents around this one, read in place; the scope returned holds this
/// file's own definitions only.
#[must_use]
#[allow(clippy::implicit_hasher)] // the inline pass reads a `HashSet<String>`
pub fn build_layered<'a>(
    src: &Source<'a>,
    arena: &'a Arena,
    events: Vec<Event>,
    outer_keys: Option<&HashSet<String>>,
    outer_abbreviations: &[(String, String)],
) -> (ast::Document<'a>, Scope) {
    let (link_keys, abbreviations) = definitions(&events);
    let mut b = Builder {
        src,
        arena,
        link_keys,
        abbreviations,
        outer_keys,
        outer_abbreviations,
        stack: Vec::new(),
        root: Vec::new(),
        hard: 0,
        spare: Vec::new(),
        scratch: std::cell::RefCell::default(),
        findings: std::cell::RefCell::default(),
    };
    for ev in events {
        b.event(ev);
    }
    while b.stack.pop().is_some() {}
    let doc = ast::Document {
        children: arena.nodes(&mut b.root),
        ast_version: "2.0",
        src_byte_length: i64::try_from(src.original_len()).ok(),
        ..ast::Document::default()
    };
    let scope = Scope {
        link_keys: b.link_keys,
        abbreviations: b.abbreviations,
        findings: b.findings.into_inner(),
    };
    (doc, scope)
}

/// The definition keys the inline pass needs (C2-I-125, C2-I-098).
fn definitions(events: &[Event]) -> (HashSet<String>, Vec<(String, String)>) {
    let mut links = HashSet::new();
    let mut abbrs: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for ev in events {
        match ev {
            Event::Leaf {
                kind: LeafKind::LinkDef { label, .. },
                ..
            } => {
                links.insert(fold_label(label));
            }
            Event::Leaf {
                kind: LeafKind::AbbrDef { key, expansion },
                ..
            } if seen.insert(key.as_str()) => {
                abbrs.push((key.clone(), expansion.clone()));
            }
            _ => {}
        }
    }
    abbrs.sort_by(|a, b| a.0.cmp(&b.0));
    (links, abbrs)
}

struct Frame<'a> {
    node: Node<'a>,
    /// The children so far, set into the node when it closes.
    kids: Vec<Node<'a>>,
    start: usize,
    hard: bool,
}

struct Builder<'s, 'a> {
    src: &'s Source<'a>,
    arena: &'a Arena,
    link_keys: HashSet<String>,
    abbreviations: Vec<(String, String)>,
    outer_keys: Option<&'s HashSet<String>>,
    outer_abbreviations: &'s [(String, String)],
    stack: Vec<Frame<'a>>,
    root: Vec<Node<'a>>,
    hard: usize,
    /// Child lists of closed frames, kept for the next frame.
    spare: Vec<Vec<Node<'a>>>,
    scratch: std::cell::RefCell<inline::Scratch<'a>>,
    /// Findings of the tree pass, as byte offsets of the source.
    findings: std::cell::RefCell<Vec<crate::block::Finding>>,
}

impl<'a> Builder<'_, 'a> {
    fn pos(&self, s: usize, e: usize) -> ast::Pos {
        self.src.pos(s, e)
    }

    /// Splits an attribute block into the node's `attrs` and `lang`.
    fn split_attrs(&self, a: Option<AttrBlock>) -> (Option<Attrs>, Option<&'a str>) {
        match a {
            Some(mut b) => {
                crate::ast::owned::dedupe_keys(&mut b.attrs.key_values);
                (
                    (!b.attrs.is_empty()).then_some(b.attrs),
                    self.os(b.lang.as_deref()),
                )
            }
            None => (None, None),
        }
    }

    fn s(&self, v: &str) -> &'a str {
        self.arena.str(v)
    }

    fn os(&self, v: Option<&str>) -> Option<&'a str> {
        v.map(|v| self.arena.str(v))
    }

    fn at(&self, a: Option<Attrs>) -> Option<&'a ast::Attrs<'a>> {
        a.map(|a| self.arena.attrs(&a))
    }

    fn inline(&self, kind: ContentKind, pieces: &[Piece]) -> &'a mut [Node<'a>] {
        let env = inline::Env {
            src: self.src,
            arena: self.arena,
            link_keys: inline::Keys {
                own: &self.link_keys,
                outer: self.outer_keys,
            },
            abbreviations: inline::Terms {
                own: &self.abbreviations,
                outer: self.outer_abbreviations,
            },
            kind,
            hard_breaks: self.hard > 0,
        };
        #[cfg(feature = "profile")]
        crate::profile::inline(true);
        let out = inline::parse_env(&env, pieces, &mut self.scratch.borrow_mut());
        #[cfg(feature = "profile")]
        crate::profile::inline(false);
        out
    }

    fn children(&mut self) -> &mut Vec<Node<'a>> {
        match self.stack.last_mut() {
            Some(f) if f.node.children().is_some() => &mut f.kids,
            _ => &mut self.root,
        }
    }

    fn event(&mut self, ev: Event) {
        match ev {
            Event::Enter { kind, start, attrs } => {
                let (node, hard) = self.container(kind, attrs.map(|b| *b));
                if hard {
                    self.hard += 1;
                }
                let kids = self.spare.pop().unwrap_or_default();
                self.stack.push(Frame {
                    node,
                    kids,
                    start,
                    hard,
                });
            }
            Event::Exit { end, loose } => self.exit(end, loose),
            Event::Leaf {
                kind,
                start,
                end,
                attrs,
            } => self.leaf(kind, start, end, attrs.map(|b| *b)),
        }
    }

    fn exit(&mut self, end: usize, loose: Option<bool>) {
        let Some(mut f) = self.stack.pop() else {
            return;
        };
        if f.hard {
            self.hard = self.hard.saturating_sub(1);
        }
        if let (Node::List(l), Some(loose)) = (&mut f.node, loose) {
            l.spacing = if loose {
                ast::ListSpacing::Loose
            } else {
                ast::ListSpacing::Tight
            };
        }
        if let Some(slot) = f.node.pos_slot() {
            *slot = Some(self.src.pos(f.start, end));
        }
        f.node.set_children(self.arena.nodes(&mut f.kids));
        self.spare.push(f.kids);
        self.children().push(f.node);
    }

    fn container(&self, kind: ContainerKind, attrs: Option<AttrBlock>) -> (Node<'a>, bool) {
        let (attrs, lang) = self.split_attrs(attrs);
        let node = match kind {
            ContainerKind::Quote => Node::BlockQuote(self.arena.alloc(ast::BlockQuote {
                attrs: self.at(attrs),
                lang,
                ..Default::default()
            })),
            ContainerKind::List { kind, start } => Node::List(self.arena.alloc(ast::List {
                marker: marker(self.arena, kind, start),
                attrs: self.at(attrs),
                lang,
                ..Default::default()
            })),
            ContainerKind::Item { task } => {
                let task = task.map(|c| ast::Task {
                    mark: self.s(&task_mark(c)),
                });
                Node::ListItem(self.arena.alloc(ast::ListItem {
                    task,
                    attrs: self.at(attrs),
                    lang,
                    ..Default::default()
                }))
            }
            ContainerKind::DescList => {
                Node::DefinitionList(self.arena.alloc(ast::DefinitionList {
                    attrs: self.at(attrs),
                    lang,
                    ..Default::default()
                }))
            }
            ContainerKind::Desc => {
                Node::DefinitionDescription(self.arena.alloc(ast::DefinitionDescription {
                    attrs: self.at(attrs),
                    lang,
                    ..Default::default()
                }))
            }
            ContainerKind::Footnote { label } => Node::Footnote(self.arena.alloc(ast::Footnote {
                key: self.s(&fold_label(&label)),
                namespace: "footnote",
                attrs: self.at(attrs),
                lang,
                ..Default::default()
            })),
            ContainerKind::Div { info, sigil } => {
                let bare_hard = sigil.is_none()
                    && info.name.is_none()
                    && info.title.is_none()
                    && info.label.is_none()
                    && attrs
                        .as_ref()
                        .and_then(|a| a.classes.last())
                        .is_some_and(|c| c == "hardbreaks");
                return (
                    self.div(*info, sigil, attrs, lang),
                    sigil == Some('\\') || bare_hard,
                );
            }
        };
        (node, false)
    }

    fn div(
        &self,
        info: DivInfo,
        sigil: Option<char>,
        attrs: Option<Attrs>,
        lang: Option<&'a str>,
    ) -> Node<'a> {
        let title = info.title.map(|(s, e)| {
            self.inline(
                ContentKind::Paragraph,
                &[Piece {
                    start: s,
                    end: e,
                    join: Join::Line,
                }],
            )
        });
        let group_label = self.os(info.label.as_deref());
        match sigil {
            Some('>') => {
                return Node::BlockQuote(self.arena.alloc(ast::BlockQuote {
                    attrs: self.at(attrs),
                    lang,
                    ..Default::default()
                }));
            }
            Some('\\') => {
                let mut a = attrs.unwrap_or_default();
                a.classes.push("hardbreaks".into());
                return Node::Div(self.arena.alloc(ast::Div {
                    attrs: Some(self.arena.attrs(&a)),
                    lang,
                    ..Default::default()
                }));
            }
            _ => {}
        }
        let name = info.name.unwrap_or_default();
        let kind = crate::tables::DIV_NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, k)| *k);
        match kind {
            Some("admonition") => Node::Admonition(self.arena.alloc(ast::Admonition {
                kind: admonition_kind(&name),
                title,
                group_label,
                attrs: self.at(attrs),
                lang,
                ..Default::default()
            })),
            Some("directive") => Node::Directive(self.arena.alloc(ast::Directive {
                kind: directive_kind(&name),
                title,
                group_label,
                attrs: self.at(attrs),
                lang,
                ..Default::default()
            })),
            Some("figure_group")
                if title.is_none()
                    && group_label.is_none()
                    && !self
                        .stack
                        .iter()
                        .any(|frame| matches!(frame.node, Node::FigureGroup(_))) =>
            {
                Node::FigureGroup(self.arena.alloc(ast::FigureGroup {
                    attrs: self.at(attrs),
                    lang,
                    ..Default::default()
                }))
            }
            _ => {
                let mut a = attrs.unwrap_or_default();
                if !name.is_empty() {
                    a.classes.insert(0, name);
                }
                let attrs = (!a.is_empty()).then_some(a);
                Node::Div(self.arena.alloc(ast::Div {
                    title,
                    group_label,
                    attrs: self.at(attrs),
                    lang,
                    ..Default::default()
                }))
            }
        }
    }

    fn leaf(&mut self, kind: LeafKind, start: usize, end: usize, attrs: Option<AttrBlock>) {
        let pos = Some(self.pos(start, end));
        if let LeafKind::Caption { pieces } = &kind {
            let children = self.inline(ContentKind::Caption, pieces);
            let content = pieces.first().map_or(start, |p| p.start);
            self.caption(children, start, content, end);
            return;
        }
        let (attrs, lang) = self.split_attrs(attrs);
        let node = match kind {
            LeafKind::Paragraph { pieces, role } => {
                let children = self.inline(ContentKind::Paragraph, &pieces);
                let role = role.map(|r| match r {
                    Role::Image => ast::ParagraphRole::Image,
                    Role::Math => ast::ParagraphRole::Math,
                });
                Node::Paragraph(self.arena.alloc(ast::Paragraph {
                    children,
                    role,
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::Heading { level, pieces } => {
                let children = self.inline(ContentKind::Heading, &pieces);
                Node::Heading(self.arena.alloc(ast::Heading {
                    level: i64::from(level),
                    children,
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::Term { pieces } => {
                let children = self.inline(ContentKind::Term, &pieces);
                Node::DefinitionTerm(self.arena.alloc(ast::DefinitionTerm {
                    children,
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            other => self.other_leaf(other, attrs, lang, pos, start, end),
        };
        self.children().push(node);
    }

    fn other_leaf(
        &self,
        kind: LeafKind,
        attrs: Option<Attrs>,
        lang: Option<&'a str>,
        pos: Option<ast::Pos>,
        start: usize,
        end: usize,
    ) -> Node<'a> {
        match kind {
            LeafKind::Code { value, info } => self.code_leaf(&value, *info, attrs, lang, pos),
            LeafKind::Comment { block, value } => {
                let form = if block {
                    ast::CommentForm::Block
                } else {
                    ast::CommentForm::Line
                };
                Node::Comment(self.arena.alloc(ast::Comment {
                    form,
                    value: self.s(&value),
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::Thematic => Node::ThematicBreak(self.arena.alloc(ast::ThematicBreak {
                attrs: self.at(attrs),
                lang,
                pos,
                ..Default::default()
            })),
            LeafKind::Table(data) => self.table(*data, attrs, lang, start, end),
            LeafKind::LinkDef { label, href, title } => {
                Node::LinkReferenceDefinition(self.arena.alloc(ast::LinkReferenceDefinition {
                    key: self.s(&fold_label(&label)),
                    namespace: "link",
                    href: self.s(&href),
                    tooltip: self.os(title.as_deref()),
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::AbbrDef { key, expansion } => {
                Node::AbbreviationDef(self.arena.alloc(ast::AbbreviationDef {
                    key: self.s(&key),
                    expansion: self.s(&expansion),
                    namespace: "abbr",
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::Frontmatter { format, value } => {
                Node::Frontmatter(self.arena.alloc(ast::Frontmatter {
                    format: self.s(&format),
                    value: self.s(&value),
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::Include {
                path,
                section,
                options,
                invalid,
            } => {
                let options = (!options.is_empty())
                    .then(|| self.arena.json(serde_json::Value::Object(options)));
                Node::Include(self.arena.alloc(ast::Include {
                    path: self.s(&path),
                    section: self.os(section.as_deref()),
                    options,
                    invalid: (!invalid.is_empty()).then(|| self.arena.strs(&invalid)),
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::LineBlock { entries } => {
                let children = self
                    .arena
                    .fill(entries.into_iter().map(|entry| match entry {
                        LineBlockEntry::Line { piece, comments } => {
                            self.line_with_comments(piece, comments)
                        }
                        LineBlockEntry::BlockComment { start, end, value } => {
                            Node::Comment(self.arena.alloc(ast::Comment {
                                form: ast::CommentForm::Block,
                                value: self.s(&value),
                                pos: Some(self.pos(start, end)),
                                ..Default::default()
                            }))
                        }
                    }));
                Node::LineBlock(self.arena.alloc(ast::LineBlock {
                    children,
                    attrs: self.at(attrs),
                    lang,
                    pos,
                    ..Default::default()
                }))
            }
            LeafKind::Paragraph { .. }
            | LeafKind::Heading { .. }
            | LeafKind::Term { .. }
            | LeafKind::Caption { .. } => {
                Node::ThematicBreak(self.arena.alloc(ast::ThematicBreak::default()))
            }
        }
    }

    fn code_leaf(
        &self,
        value: &str,
        info: crate::block::shapes::FenceInfo,
        attrs: Option<Attrs>,
        lang: Option<&'a str>,
        pos: Option<ast::Pos>,
    ) -> Node<'a> {
        if let Some(format) = info.raw {
            return Node::RawBlock(self.arena.alloc(ast::RawBlock {
                format: self.s(&format),
                value: self.s(value),
                attrs: self.at(attrs),
                lang,
                pos,
                ..Default::default()
            }));
        }
        if let Some(sep) = delimited::separator(info.language.as_deref(), attrs.as_ref()) {
            return self.delimited_table(value, sep, attrs, lang, pos);
        }
        let title = info.title.map(|(s, e)| {
            let raw = self.src.slice(s, e);
            let value = crate::block::include::unescape(raw);
            self.arena.one(Node::Text(self.arena.alloc(ast::Text {
                value: self.arena.str(&value),
                pos: (value == raw).then(|| self.src.pos(s, e)),
                ..Default::default()
            })))
        });
        Node::CodeBlock(self.arena.alloc(ast::CodeBlock {
            value: self.s(value),
            syntax: self.os(info.language.as_deref()),
            title,
            group_label: self.os(info.label.as_deref()),
            attrs: self.at(attrs),
            lang,
            pos,
            ..Default::default()
        }))
    }

    fn line(&self, p: Piece) -> Node<'a> {
        // C2-A-158: a line with no children has no source character and no `pos`.
        if p.start == p.end {
            return Node::Line(self.arena.alloc(ast::Line::default()));
        }
        let children = self.inline(ContentKind::LineBlockLine, &[p]);
        let pos = (!children.is_empty()).then(|| self.pos(p.start, p.end));
        Node::Line(self.arena.alloc(ast::Line {
            children,
            pos,
            ..Default::default()
        }))
    }

    fn line_with_comments(&self, piece: Piece, comments: Vec<(usize, usize, String)>) -> Node<'a> {
        if comments.is_empty() {
            return self.line(piece);
        }
        let mut children = self
            .inline(ContentKind::LineBlockLine, &[piece])
            .iter_mut()
            .map(|node| {
                std::mem::replace(
                    node,
                    Node::ThematicBreak(self.arena.alloc(ast::ThematicBreak::default())),
                )
            })
            .collect::<Vec<_>>();
        let last = comments.last().map_or(piece.end, |(_, end, _)| *end);
        for (start, end, value) in comments {
            children.push(Node::Comment(self.arena.alloc(ast::Comment {
                form: ast::CommentForm::Line,
                value: self.s(&value),
                pos: Some(self.pos(start, end)),
                ..Default::default()
            })));
        }
        Node::Line(self.arena.alloc(ast::Line {
            children: self.arena.fill(children.into_iter()),
            pos: Some(self.pos(piece.start, last)),
            ..Default::default()
        }))
    }

    /// A caption attaches to the block before it (C2-A-082, C2-A-183).
    fn caption(&mut self, children: &'a mut [Node<'a>], start: usize, content: usize, end: usize) {
        let para = Node::Paragraph(self.arena.alloc(ast::Paragraph {
            children,
            pos: Some(self.pos(content, end)),
            ..Default::default()
        }));
        let cap = Node::Caption(self.arena.alloc(ast::Caption {
            children: self.arena.one(para),
            pos: Some(self.pos(start, end)),
            ..Default::default()
        }));
        let (src, arena) = (self.src, self.arena);
        let list = self.children();
        let Some(index) = list.iter().rposition(|n| !matches!(n, Node::Comment(_))) else {
            return;
        };
        if list.len() > index + 1
            && matches!(
                list.get(index),
                Some(Node::BlockQuote(_) | Node::FigureGroup(_))
            )
        {
            let comments = list.split_off(index + 1);
            if let Some(target) = list.get_mut(index) {
                append_comments(arena, target, comments);
            }
        }
        let Some(target) = list.get_mut(index) else {
            return;
        };
        attach_caption(src, arena, target, cap, end);
    }
}

fn append_comments<'a>(arena: &'a Arena, target: &mut Node<'a>, comments: Vec<Node<'a>>) {
    let children = match target {
        Node::BlockQuote(q) => &mut q.children,
        Node::FigureGroup(g) => &mut g.children,
        _ => return,
    };
    let mut joined = Vec::with_capacity(children.len() + comments.len());
    for node in children.iter_mut() {
        joined.push(std::mem::replace(
            node,
            Node::Text(arena.alloc(ast::Text::default())),
        ));
    }
    joined.extend(comments);
    *children = arena.nodes(&mut joined);
}

fn attach_caption<'a>(
    src: &Source<'_>,
    arena: &'a Arena,
    target: &mut Node<'a>,
    cap: Node<'a>,
    end: usize,
) {
    match target {
        Node::Paragraph(p) => p.caption = Some(arena.alloc(cap)),
        Node::CodeBlock(c) => c.caption = Some(arena.alloc(cap)),
        Node::BlockQuote(q) => q.caption = Some(arena.alloc(cap)),
        Node::Table(t) => t.caption = Some(arena.alloc(cap)),
        Node::FigureGroup(g) => g.caption = Some(arena.alloc(cap)),
        // C2-B-147: a code include takes a caption, which its expansion gives its child.
        Node::Include(i) if i.options.is_some_and(|o| o.get("code").is_some()) => {
            i.caption = Some(arena.alloc(cap));
        }
        _ => return,
    }
    if let Some(p) = target.pos_slot().and_then(Option::as_mut) {
        extend(src, p, end);
    }
}

fn extend(src: &Source<'_>, p: &mut ast::Pos, end: usize) {
    let e = src.pos(end, end);
    p.end_line = e.end_line;
    p.end_column = e.end_column;
    p.end_offset = e.end_offset;
}

fn marker(arena: &Arena, kind: MarkerKind, start: Option<u64>) -> ast::Marker<'_> {
    let ch = |c: char| arena.str(c.encode_utf8(&mut [0; 4]));
    match kind {
        MarkerKind::Bullet(c) => ast::Marker {
            kind: "bullet",
            bullet: Some(ch(c)),
            ..Default::default()
        },
        MarkerKind::Ordered { style, delim, .. } => ast::Marker {
            kind: "ordered",
            style: Some(style.code()),
            delimiter: Some(ch(delim)),
            start: start
                .filter(|&s| s != 1)
                .and_then(|s| i64::try_from(s).ok()),
            ..Default::default()
        },
    }
}

fn task_mark(c: char) -> String {
    crate::tables::TASK_STATES
        .iter()
        .find(|(ch, ..)| *ch == c)
        .map_or(c, |(_, m, ..)| *m)
        .to_string()
}

fn admonition_kind(name: &str) -> ast::AdmonitionKind {
    ast::AdmonitionKind::from_name(name).unwrap_or_default()
}

fn directive_kind(name: &str) -> ast::DirectiveKind {
    ast::DirectiveKind::from_name(name).unwrap_or_default()
}
