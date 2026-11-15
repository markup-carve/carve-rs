//! The tree builder: block events to the typed AST 2.0, calling the inline
//! pass for each content (C2-I-001).

mod delimited;
mod table;

use crate::ast::{self, Attrs, Node};
use crate::attr::AttrBlock;
use crate::block::events::{ContainerKind, Event, LeafKind, Role};
use crate::block::shapes::{DivInfo, MarkerKind};
use crate::chars::fold_label;
use crate::inline::{self, ContentKind, InlineEnv, Join, Piece};
use crate::source::Source;
use std::collections::HashSet;

/// Builds the document from the events of the block pass.
#[must_use]
pub fn build(src: &Source, events: Vec<Event>) -> ast::Document {
    let (link_keys, abbreviations) = definitions(&events);
    let mut b = Builder {
        src,
        link_keys,
        abbreviations,
        stack: Vec::new(),
        root: Vec::new(),
        hard: 0,
    };
    for ev in events {
        b.event(ev);
    }
    while b.stack.pop().is_some() {}
    ast::Document {
        children: b.root,
        ast_version: "2.0".into(),
        src_byte_length: i64::try_from(src.original_len()).ok(),
        ..ast::Document::default()
    }
}

/// The definition keys the inline pass needs (C2-I-125, C2-I-098).
fn definitions(events: &[Event]) -> (HashSet<String>, Vec<(String, String)>) {
    let mut links = HashSet::new();
    let mut abbrs: Vec<(String, String)> = Vec::new();
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
            } => {
                if !abbrs.iter().any(|(k, _)| k == key) {
                    abbrs.push((key.clone(), expansion.clone()));
                }
            }
            _ => {}
        }
    }
    (links, abbrs)
}

struct Frame {
    node: Node,
    start: usize,
    hard: bool,
}

struct Builder<'a> {
    src: &'a Source,
    link_keys: HashSet<String>,
    abbreviations: Vec<(String, String)>,
    stack: Vec<Frame>,
    root: Vec<Node>,
    hard: usize,
}

/// Splits an attribute block into the node's `attrs` and `lang`.
fn split_attrs(a: Option<AttrBlock>) -> (Option<Attrs>, Option<String>) {
    match a {
        Some(b) => ((!b.attrs.is_empty()).then_some(b.attrs), b.lang),
        None => (None, None),
    }
}

impl Builder<'_> {
    fn pos(&self, s: usize, e: usize) -> Option<ast::Pos> {
        Some(self.src.pos(s, e))
    }

    fn inline(&self, kind: ContentKind, pieces: &[Piece]) -> Vec<Node> {
        let env = InlineEnv {
            src: self.src,
            link_keys: &self.link_keys,
            abbreviations: &self.abbreviations,
            depth: self.stack.len(),
            kind,
            hard_breaks: self.hard > 0,
        };
        inline::parse(&env, pieces)
    }

    fn children(&mut self) -> &mut Vec<Node> {
        match self.stack.last_mut().and_then(|f| f.node.children_mut()) {
            Some(c) => c,
            None => &mut self.root,
        }
    }

    fn event(&mut self, ev: Event) {
        match ev {
            Event::Enter { kind, start, attrs } => {
                let (node, hard) = self.container(kind, attrs);
                if hard {
                    self.hard += 1;
                }
                self.stack.push(Frame { node, start, hard });
            }
            Event::Exit { end, loose } => self.exit(end, loose),
            Event::Leaf {
                kind,
                start,
                end,
                attrs,
            } => self.leaf(kind, start, end, attrs),
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
        self.children().push(f.node);
    }

    fn container(&self, kind: ContainerKind, attrs: Option<AttrBlock>) -> (Node, bool) {
        let (attrs, lang) = split_attrs(attrs);
        let node = match kind {
            ContainerKind::Quote => Node::BlockQuote(ast::BlockQuote {
                attrs,
                lang,
                ..Default::default()
            }),
            ContainerKind::List { kind, start } => Node::List(ast::List {
                marker: marker(kind, start),
                attrs,
                lang,
                ..Default::default()
            }),
            ContainerKind::Item { task } => {
                let task = task.map(|c| ast::Task { mark: task_mark(c) });
                Node::ListItem(ast::ListItem {
                    task,
                    attrs,
                    lang,
                    ..Default::default()
                })
            }
            ContainerKind::DescList => Node::DefinitionList(ast::DefinitionList {
                attrs,
                lang,
                ..Default::default()
            }),
            ContainerKind::Desc => Node::DefinitionDescription(ast::DefinitionDescription {
                attrs,
                lang,
                ..Default::default()
            }),
            ContainerKind::Footnote { label } => Node::Footnote(ast::Footnote {
                key: fold_label(&label),
                namespace: "footnote".into(),
                attrs,
                lang,
                ..Default::default()
            }),
            ContainerKind::Div { info, sigil } => {
                return (self.div(info, sigil, attrs, lang), sigil == Some('\\'));
            }
        };
        (node, false)
    }

    fn div(
        &self,
        info: DivInfo,
        sigil: Option<char>,
        attrs: Option<Attrs>,
        lang: Option<String>,
    ) -> Node {
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
        let group_label = info.label;
        match sigil {
            Some('>') => {
                return Node::BlockQuote(ast::BlockQuote {
                    attrs,
                    lang,
                    ..Default::default()
                });
            }
            Some('\\') => {
                let mut a = attrs.unwrap_or_default();
                a.classes.push("hardbreaks".into());
                return Node::Div(ast::Div {
                    attrs: Some(a),
                    lang,
                    ..Default::default()
                });
            }
            _ => {}
        }
        let name = info.name.unwrap_or_default();
        let kind = crate::tables::DIV_NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, k)| *k);
        match kind {
            Some("admonition") => Node::Admonition(ast::Admonition {
                kind: admonition_kind(&name),
                title,
                group_label,
                attrs,
                lang,
                ..Default::default()
            }),
            Some("directive") => Node::Directive(ast::Directive {
                kind: directive_kind(&name),
                title,
                group_label,
                attrs,
                lang,
                ..Default::default()
            }),
            Some("figure_group") if title.is_none() && group_label.is_none() => {
                Node::FigureGroup(ast::FigureGroup {
                    attrs,
                    lang,
                    ..Default::default()
                })
            }
            _ => {
                let mut a = attrs.unwrap_or_default();
                if !name.is_empty() {
                    a.classes.insert(0, name);
                }
                let attrs = (!a.is_empty()).then_some(a);
                Node::Div(ast::Div {
                    title,
                    group_label,
                    attrs,
                    lang,
                    ..Default::default()
                })
            }
        }
    }

    fn leaf(&mut self, kind: LeafKind, start: usize, end: usize, attrs: Option<AttrBlock>) {
        let pos = self.pos(start, end);
        if let LeafKind::Caption { pieces } = &kind {
            let children = self.inline(ContentKind::Caption, pieces);
            let content = pieces.first().map_or(start, |p| p.start);
            self.caption(children, start, content, end);
            return;
        }
        let (attrs, lang) = split_attrs(attrs);
        let node = match kind {
            LeafKind::Paragraph { pieces, role } => {
                let children = self.inline(ContentKind::Paragraph, &pieces);
                let role = role.map(|r| match r {
                    Role::Image => ast::ParagraphRole::Image,
                    Role::Math => ast::ParagraphRole::Math,
                });
                Node::Paragraph(ast::Paragraph {
                    children,
                    role,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            LeafKind::Heading { level, pieces } => {
                let children = self.inline(ContentKind::Heading, &pieces);
                Node::Heading(ast::Heading {
                    level: i64::from(level),
                    children,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            LeafKind::Term { pieces } => {
                let children = self.inline(ContentKind::Term, &pieces);
                Node::DefinitionTerm(ast::DefinitionTerm {
                    children,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            other => self.other_leaf(other, attrs, lang, pos, start, end),
        };
        self.children().push(node);
    }

    fn other_leaf(
        &self,
        kind: LeafKind,
        attrs: Option<Attrs>,
        lang: Option<String>,
        pos: Option<ast::Pos>,
        start: usize,
        end: usize,
    ) -> Node {
        match kind {
            LeafKind::Code { value, info } => {
                if let Some(format) = info.raw {
                    return Node::RawBlock(ast::RawBlock {
                        format,
                        value,
                        attrs,
                        lang,
                        pos,
                        ..Default::default()
                    });
                }
                if let Some(sep) = delimited::separator(info.language.as_deref(), attrs.as_ref()) {
                    return Self::delimited_table(&value, sep, attrs, lang, pos);
                }
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
                Node::CodeBlock(ast::CodeBlock {
                    value,
                    syntax: info.language,
                    title,
                    group_label: info.label,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            LeafKind::Comment { block, value } => {
                let form = if block {
                    ast::CommentForm::Block
                } else {
                    ast::CommentForm::Line
                };
                Node::Comment(ast::Comment {
                    form,
                    value,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            LeafKind::Thematic => Node::ThematicBreak(ast::ThematicBreak {
                attrs,
                lang,
                pos,
                ..Default::default()
            }),
            LeafKind::Table(data) => self.table(*data, attrs, lang, start, end),
            LeafKind::LinkDef { label, href, title } => {
                Node::LinkReferenceDefinition(ast::LinkReferenceDefinition {
                    key: fold_label(&label),
                    namespace: "link".into(),
                    href,
                    tooltip: title,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            LeafKind::AbbrDef { key, expansion } => Node::AbbreviationDef(ast::AbbreviationDef {
                key,
                expansion,
                namespace: "abbr".into(),
                attrs,
                lang,
                pos,
                ..Default::default()
            }),
            LeafKind::Frontmatter { format, value } => Node::Frontmatter(ast::Frontmatter {
                format,
                value,
                attrs,
                lang,
                pos,
                ..Default::default()
            }),
            LeafKind::Include {
                path,
                section,
                options,
            } => {
                let options = (!options.is_empty()).then_some(serde_json::Value::Object(options));
                Node::Include(ast::Include {
                    path,
                    section,
                    options,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            LeafKind::LineBlock { lines } => {
                let children = lines.into_iter().map(|l| self.line(l)).collect();
                Node::LineBlock(ast::LineBlock {
                    children,
                    attrs,
                    lang,
                    pos,
                    ..Default::default()
                })
            }
            LeafKind::Paragraph { .. }
            | LeafKind::Heading { .. }
            | LeafKind::Term { .. }
            | LeafKind::Caption { .. } => Node::ThematicBreak(ast::ThematicBreak::default()),
        }
    }

    fn line(&self, p: Piece) -> Node {
        if p.start == p.end {
            return Node::Line(ast::Line {
                pos: self.pos(p.start, p.end),
                ..Default::default()
            });
        }
        let children = self.inline(ContentKind::LineBlockLine, &[p]);
        let pos = self.pos(p.start, p.end);
        Node::Line(ast::Line {
            children,
            pos,
            ..Default::default()
        })
    }

    /// A caption attaches to the block before it (C2-A-082, C2-A-183).
    fn caption(&mut self, children: Vec<Node>, start: usize, content: usize, end: usize) {
        let para = Node::Paragraph(ast::Paragraph {
            children,
            pos: self.pos(content, end),
            ..Default::default()
        });
        let cap = Node::Caption(ast::Caption {
            children: vec![para],
            pos: self.pos(start, end),
            ..Default::default()
        });
        let src = self.src;
        let list = self.children();
        let Some(target) = list.pop() else { return };
        list.push(wrap_caption(src, target, cap, end));
    }
}

fn wrap_caption(src: &Source, target: Node, cap: Node, end: usize) -> Node {
    if let Node::Table(mut t) = target {
        t.caption = Some(Box::new(cap));
        if let Some(p) = &mut t.pos {
            extend(src, p, end);
        }
        return Node::Table(t);
    }
    if let Node::FigureGroup(mut g) = target {
        g.caption = Some(Box::new(cap));
        if let Some(p) = &mut g.pos {
            extend(src, p, end);
        }
        return Node::FigureGroup(g);
    }
    let mut target = target;
    let attrs = target.attrs_slot().and_then(Option::take);
    let mut pos = target.pos().copied();
    if attrs.is_some() {
        if let Some(p) = target.pos_slot().and_then(Option::as_mut) {
            skip_attr_lines(src, p);
        }
    }
    if let Some(p) = &mut pos {
        extend(src, p, end);
    }
    Node::Figure(ast::Figure {
        children: vec![target],
        caption: Some(Box::new(cap)),
        attrs,
        pos,
        ..Default::default()
    })
}

/// The figure takes the attribute lines; the block inside starts at its own markup.
fn skip_attr_lines(src: &Source, p: &mut ast::Pos) {
    let mut i = p.start_line.saturating_sub(1);
    let last = p.end_line.saturating_sub(1);
    while i < last && src.line(i).trim_start_matches([' ', '\t']).starts_with('{') {
        i += 1;
    }
    let s = src.pos(src.line_range(i).0, src.line_range(i).0);
    p.start_line = s.start_line;
    p.start_column = s.start_column;
    p.start_offset = s.start_offset;
}

fn extend(src: &Source, p: &mut ast::Pos, end: usize) {
    let e = src.pos(end, end);
    p.end_line = e.end_line;
    p.end_column = e.end_column;
    p.end_offset = e.end_offset;
}

fn marker(kind: MarkerKind, start: Option<u64>) -> ast::Marker {
    match kind {
        MarkerKind::Bullet(c) => ast::Marker {
            kind: "bullet".into(),
            bullet: Some(c.to_string()),
            ..Default::default()
        },
        MarkerKind::Ordered { style, delim, .. } => ast::Marker {
            kind: "ordered".into(),
            style: Some(style.code().into()),
            delimiter: Some(delim.to_string()),
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
    serde_json::from_value(serde_json::Value::String(name.into())).unwrap_or_default()
}

fn directive_kind(name: &str) -> ast::DirectiveKind {
    serde_json::from_value(serde_json::Value::String(name.into())).unwrap_or_default()
}
