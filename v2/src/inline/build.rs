//! The builder: events to typed inline nodes with positions (C2-A-138 to
//! C2-A-158), adjacent text merged (C2-A-040), smart punctuation and
//! abbreviations split out of plain text.

use super::content::Content;
use super::delim::Ctx;
use super::events::Ev;
use super::smart::{Out, Prev, Smart};
use super::token::Kind;
use super::{InlineEnv, Scratch, abbr};
use crate::Arena;
use crate::ast::{
    Abbreviation, CommentForm, HardBreak, InlineExtension, InlineFootnote, Link, Node,
    NonBreakingSpace, Pos, SmartPunctuation, SoftBreak, Span, Text,
};
use crate::attr::AttrBlock;
use crate::tables;

pub(super) struct Frame<'a> {
    kind: Kind,
    v0: usize,
    v1: usize,
    children: Vec<Node<'a>>,
    /// Past the depth cap (C2-I-118): written as its source.
    dead: bool,
}

struct Builder<'r, 'e, 'a> {
    c: &'r Content<'e, 'a>,
    ctx: &'r Ctx<'r>,
    env: &'r InlineEnv<'e, 'a>,
    a: &'a Arena,
    root: Vec<Node<'a>>,
    frames: Vec<Frame<'a>>,
    /// Child lists of closed frames, kept for the next frame.
    lists: Vec<Vec<Node<'a>>>,
    /// While set, the value of the last text node of the open list is `join`:
    /// pieces that are no one slice of the source gather here and are written
    /// once, so merging stays linear (C2-A-040).
    joining: bool,
    join: String,
    smart: Smart,
    /// The flow character before the current event, for abbreviations.
    flow_prev: Option<char>,
    /// A line comment ended at this join (GAP INL-011).
    comment_to: Option<usize>,
    /// The pieces of one smart typography run, kept for the next run.
    outs: Vec<Out>,
}

/// Builds the inline nodes of one content.
#[must_use]
pub(super) fn build<'a>(
    c: &Content<'_, 'a>,
    ctx: &Ctx<'_>,
    env: &InlineEnv<'_, 'a>,
    s: &mut Scratch<'a>,
) -> &'a mut [Node<'a>] {
    let mut evs = std::mem::take(&mut s.evs);
    let next_flow = if env.abbreviations.is_empty() && ctx.comments.is_empty() {
        Vec::new()
    } else {
        next_flow(ctx.t, &evs)
    };
    let mut lists = std::mem::take(&mut s.lists);
    let mut b = Builder {
        c,
        ctx,
        env,
        a: env.arena,
        root: lists.pop().unwrap_or_default(),
        frames: std::mem::take(&mut s.frames),
        lists,
        joining: false,
        join: std::mem::take(&mut s.join),
        smart: Smart::new(),
        flow_prev: None,
        comment_to: None,
        outs: std::mem::take(&mut s.outs),
    };
    for (i, ev) in evs.drain(..).enumerate() {
        b.event(ev, next_flow.get(i).copied().flatten());
    }
    b.flush();
    while let Some(f) = b.frames.pop() {
        let end = f.v1;
        b.exit(f, end, end, None);
        b.flush();
    }
    let out = b.a.nodes(&mut b.root);
    b.lists.push(b.root);
    s.evs = evs;
    s.lists = b.lists;
    s.join = b.join;
    s.outs = b.outs;
    s.frames = b.frames;
    out
}

fn first_char(t: &str, ev: &Ev<'_>) -> Option<Option<char>> {
    match ev {
        Ev::Text { v0, v1, val, .. } => Some(
            val.as_deref()
                .or_else(|| t.get(*v0..*v1))
                .and_then(|s| s.chars().next()),
        ),
        Ev::Break { .. } | Ev::CellJoin { .. } | Ev::Nbsp { .. } => Some(Some(' ')),
        Ev::Leaf {
            transparent: false, ..
        }
        | Ev::Enter {
            kind: Kind::InlineFootnote,
            ..
        }
        | Ev::Exit { note: true, .. } => Some(None),
        _ => None,
    }
}

/// For each event, the first flow character after it (C2-I-099).
fn next_flow(t: &str, evs: &[Ev<'_>]) -> Vec<Option<char>> {
    let mut out = vec![None; evs.len()];
    let mut next = None;
    for (i, ev) in evs.iter().enumerate().rev() {
        if let Some(slot) = out.get_mut(i) {
            *slot = next;
        }
        if let Some(c) = first_char(t, ev) {
            next = c;
        }
    }
    out
}

fn set_pos(node: &mut Node<'_>, pos: Option<Pos>) {
    if let Some(slot) = node.pos_slot() {
        *slot = pos;
    }
}

fn apply_attrs<'a>(a: &'a Arena, node: &mut Node<'a>, attrs: Option<AttrBlock>) {
    let Some(block) = attrs else { return };
    if !block.attrs.is_empty()
        && let Some(slot) = node.attrs_slot()
    {
        *slot = Some(a.attrs(&block.attrs));
    }
    if let Some(lang) = block.lang {
        set_lang(node, Some(a.str(&lang)));
    }
}

/// The `lang` part of an attribute block on a host (C2-I-029, C2-I-036).
pub fn set_lang<'a>(node: &mut Node<'a>, lang: Option<&'a str>) {
    let slot = match node {
        Node::Code(n) => &mut n.lang,
        Node::LiteralInline(n) => &mut n.lang,
        Node::Math(n) => &mut n.lang,
        Node::Autolink(n) => &mut n.lang,
        Node::FootnoteRef(n) => &mut n.lang,
        Node::Symbol(n) => &mut n.lang,
        Node::Image(n) => &mut n.lang,
        Node::Span(n) => &mut n.lang,
        Node::Link(n) => &mut n.lang,
        other => return set_lang_container(other, lang),
    };
    *slot = lang;
}

fn set_lang_container<'a>(node: &mut Node<'a>, lang: Option<&'a str>) {
    let slot = match node {
        Node::InlineFootnote(n) => &mut n.lang,
        Node::InlineExtension(n) => &mut n.lang,
        Node::Emphasis(n) => &mut n.lang,
        Node::Strong(n) => &mut n.lang,
        Node::Underline(n) => &mut n.lang,
        Node::Strike(n) => &mut n.lang,
        Node::Highlight(n) => &mut n.lang,
        Node::Superscript(n) => &mut n.lang,
        Node::Subscript(n) => &mut n.lang,
        Node::Insert(n) => &mut n.lang,
        Node::Delete(n) => &mut n.lang,
        _ => return,
    };
    *slot = lang;
}

/// An empty node of a delimiter span type (`tables::DELIMITERS`).
fn styled<'a>(a: &'a Arena, name: &str) -> Option<Node<'a>> {
    Some(match name {
        "delete" => Node::Delete(a.alloc(crate::ast::Delete::default())),
        "emphasis" => Node::Emphasis(a.alloc(crate::ast::Emphasis::default())),
        "highlight" => Node::Highlight(a.alloc(crate::ast::Highlight::default())),
        "insert" => Node::Insert(a.alloc(crate::ast::Insert::default())),
        "strike" => Node::Strike(a.alloc(crate::ast::Strike::default())),
        "strong" => Node::Strong(a.alloc(crate::ast::Strong::default())),
        "subscript" => Node::Subscript(a.alloc(crate::ast::Subscript::default())),
        "superscript" => Node::Superscript(a.alloc(crate::ast::Superscript::default())),
        "underline" => Node::Underline(a.alloc(crate::ast::Underline::default())),
        _ => return None,
    })
}

/// A container node of `kind`, empty.
fn container(a: &Arena, kind: Kind) -> Option<Node<'_>> {
    Some(match kind {
        Kind::Styled(name) => styled(a, name)?,
        Kind::Span => Node::Span(a.alloc(Span::default())),
        Kind::Link {
            href,
            tooltip,
            key,
            form,
        } => {
            let namespace = key.as_ref().map(|_| "link");
            Node::Link(a.alloc(Link {
                href: href.map(|h| a.str(&h)),
                tooltip: tooltip.map(|t| a.str(&t)),
                key: key.map(|k| a.str(&k)),
                namespace,
                ref_form: form,
                ..Default::default()
            }))
        }
        Kind::InlineFootnote => Node::InlineFootnote(a.alloc(InlineFootnote::default())),
        Kind::Extension(name) => Node::InlineExtension(a.alloc(InlineExtension {
            name: a.str(&name),
            ..Default::default()
        })),
    })
}

/// The position of two adjacent text pieces merged, none when they do not touch.
fn joined(a: Option<Pos>, b: Option<Pos>) -> Option<Pos> {
    match (a, b) {
        (Some(a), Some(b)) if a.end_offset == b.start_offset => Some(Pos {
            end_line: b.end_line,
            end_column: b.end_column,
            end_offset: b.end_offset,
            ..a
        }),
        _ => None,
    }
}

impl<'r, 'a> Builder<'r, '_, 'a> {
    fn out(&mut self) -> &mut Vec<Node<'a>> {
        match self.frames.last_mut() {
            Some(f) => &mut f.children,
            None => &mut self.root,
        }
    }

    fn add(&mut self, node: Node<'a>) {
        if let Node::Text(new) = &node
            && matches!(self.out().last(), Some(Node::Text(_)))
        {
            self.merge(new.value, new.pos);
            return;
        }
        self.flush();
        self.out().push(node);
    }

    /// Appends `value` to the text node that ends the open list (C2-A-040).
    fn merge(&mut self, value: &str, pos: Option<Pos>) {
        let (c, src) = (self.c, self.env.src);
        let joining = self.joining;
        let list = match self.frames.last_mut() {
            Some(f) => &mut f.children,
            None => &mut self.root,
        };
        let Some(Node::Text(tail)) = list.last_mut() else {
            return;
        };
        tail.pos = joined(tail.pos, pos);
        if joining {
            self.join.push_str(value);
            return;
        }
        let next = c.borrow(value);
        if let (Some(at), Some(next)) = (src.offset_of(tail.value), next)
            && src.offset_of(next) == Some(at + tail.value.len())
            && let Some(both) = src.borrowed(at, at + tail.value.len() + next.len())
        {
            tail.value = both;
            return;
        }
        self.join.clear();
        self.join.push_str(tail.value);
        self.join.push_str(value);
        self.joining = true;
    }

    /// Writes the gathered text into its node.
    fn flush(&mut self) {
        if !std::mem::take(&mut self.joining) {
            return;
        }
        let a = self.a;
        let value = a.str(&self.join);
        if let Some(Node::Text(last)) = self.out().last_mut() {
            last.value = value;
        }
    }

    /// C2-A-158: joined cell nodes crossing source pieces omit their positions.
    fn pos(&self, v0: usize, v1: usize) -> Option<Pos> {
        (!self.c.cell_joined() || self.c.contiguous(v0, v1)).then(|| self.c.pos(v0, v1))
    }

    /// Adds text, appended to a text node before it (C2-A-040).
    fn text(&mut self, v0: usize, v1: usize, value: &str) {
        let pos = if self.c.contiguous(v0, v1) {
            self.pos(v0, v1)
        } else {
            None
        };
        if matches!(self.out().last(), Some(Node::Text(_))) {
            self.merge(value, pos);
            return;
        }
        let node = Node::Text(self.a.alloc(Text {
            value: self.c.keep(value),
            pos,
            ..Default::default()
        }));
        self.out().push(node);
    }

    /// The source text of a content range.
    fn src(&self, v0: usize, v1: usize) -> &'r str {
        self.ctx.t.get(v0..v1).unwrap_or("")
    }

    fn event(&mut self, ev: Ev<'a>, next_flow: Option<char>) {
        let comment_to = self.comment_to.take();
        match ev {
            Ev::Enter { kind, v0, v1 } => {
                self.flush();
                self.enter(kind, v0, v1);
            }
            Ev::Exit { v0, v1, attrs, .. } => {
                self.flush();
                if let Some(f) = self.frames.pop() {
                    self.exit(f, v0, v1, attrs);
                }
            }
            Ev::Text {
                v0,
                v1,
                val: None,
                plain: true,
            } => self.plain(v0, v1, next_flow),
            Ev::Text { v0, v1, val, .. } => {
                let value = val.as_deref().unwrap_or_else(|| self.src(v0, v1));
                if let Some(last) = value.chars().next_back() {
                    self.smart.prev = Prev::Char(last);
                    self.flow_prev = Some(last);
                }
                self.text(v0, v1, value);
            }
            Ev::Nbsp { v0, v1 } => {
                self.space_prev('\u{a0}');
                let pos = self.pos(v0, v1);
                self.add(Node::NonBreakingSpace(self.a.alloc(NonBreakingSpace {
                    pos,
                    ..Default::default()
                })));
            }
            Ev::Break { v0, v1, hard } => {
                self.space_prev('\n');
                let pos = if comment_to == Some(v0) {
                    Some(self.c.pos_from_line_end(v0, v1))
                } else {
                    self.pos(v0, v1)
                };
                self.add(if hard {
                    Node::HardBreak(self.a.alloc(HardBreak {
                        pos,
                        ..Default::default()
                    }))
                } else {
                    Node::SoftBreak(self.a.alloc(SoftBreak {
                        pos,
                        ..Default::default()
                    }))
                });
            }
            Ev::CellJoin { .. } => {
                self.space_prev(' ');
                self.add(Node::Text(self.a.alloc(Text {
                    value: " ",
                    ..Default::default()
                })));
            }
            Ev::Leaf {
                v0,
                v1,
                node,
                transparent,
            } => self.leaf(node, v0, v1, transparent),
        }
    }

    fn space_prev(&mut self, c: char) {
        self.smart.prev = Prev::Char(c);
        self.flow_prev = Some(c);
    }

    fn leaf(&mut self, mut node: Node<'a>, v0: usize, v1: usize, transparent: bool) {
        if !transparent {
            self.smart.prev = Prev::Word;
            self.flow_prev = None;
        }
        let pos = match &node {
            Node::Comment(c) if c.form == CommentForm::Line && self.pos(v0, v1).is_some() => {
                self.comment_to = Some(v1);
                Some(self.c.pos_to_line_end(v0, v1))
            }
            _ => self.pos(v0, v1),
        };
        set_pos(&mut node, pos);
        self.add(node);
    }

    fn enter(&mut self, kind: Kind, v0: usize, v1: usize) {
        let open = self.frames.iter().filter(|f| !f.dead).count();
        let dead = open >= tables::INLINE_DEPTH;
        if kind == Kind::InlineFootnote {
            // GAP INL-013: an inline footnote's edges bound abbreviations.
            self.flow_prev = None;
        }
        if dead {
            let t = self.src(v0, v1);
            self.text(v0, v1, t);
            self.flush();
        }
        self.smart.prev = Prev::Start;
        let children = self.lists.pop().unwrap_or_default();
        self.frames.push(Frame {
            kind,
            v0,
            v1,
            children,
            dead,
        });
    }

    fn exit(&mut self, mut f: Frame<'a>, v0: usize, v1: usize, attrs: Option<AttrBlock>) {
        if f.kind == Kind::InlineFootnote {
            self.flow_prev = None;
        }
        self.smart.prev = Prev::Word;
        if f.dead {
            for ch in f.children.drain(..) {
                self.add(ch);
            }
            self.lists.push(f.children);
            let t = self.src(v0, v1);
            self.text(v0, v1, t);
            return;
        }
        let pos = self.pos(f.v0, v1);
        let Some(mut node) = container(self.a, f.kind) else {
            self.lists.push(f.children);
            return;
        };
        node.set_children(self.a.nodes(&mut f.children));
        self.lists.push(f.children);
        apply_attrs(self.a, &mut node, attrs);
        set_pos(&mut node, pos);
        self.add(node);
    }

    /// Plain text: abbreviations first, smart typography in what is left.
    fn plain(&mut self, v0: usize, v1: usize, next_flow: Option<char>) {
        let s = self.ctx.t.get(v0..v1).unwrap_or("");
        let hits = abbr::find(s, self.flow_prev, next_flow, self.env.abbreviations);
        let mut from = v0;
        for (a, b, idx) in hits {
            self.smart_text(from, v0 + a, None);
            let (key, expansion) = self
                .env
                .abbreviations
                .get(idx)
                .map_or(("", ""), |(k, e)| (k.as_str(), e.as_str()));
            let pos = self.pos(v0 + a, v0 + b);
            let node = Abbreviation {
                key: self.a.str(key),
                expansion: Some(self.a.str(expansion)),
                namespace: "abbr",
                pos,
                ..Default::default()
            };
            self.add(Node::Abbreviation(self.a.alloc(node)));
            if let Some(last) = s.get(..b).and_then(|x| x.chars().next_back()) {
                self.smart.prev = Prev::Char(last);
            }
            from = v0 + b;
        }
        self.smart_text(from, v1, next_flow);
        if let Some(last) = s.chars().next_back() {
            self.flow_prev = Some(last);
        }
    }

    fn smart_text(&mut self, v0: usize, v1: usize, next_flow: Option<char>) {
        if v0 >= v1 {
            return;
        }
        let mut outs = std::mem::take(&mut self.outs);
        self.smart.run(self.ctx, v0, v1, next_flow, &mut outs);
        for o in outs.drain(..) {
            match o {
                Out::Text(a, b) => {
                    let t = self.src(a, b);
                    self.text(a, b, t);
                }
                Out::Punct {
                    v0,
                    v1,
                    kind,
                    glyph,
                } => {
                    let Some(kind) = crate::ast::SmartPunctuationKind::from_name(kind) else {
                        continue;
                    };
                    let pos = self.pos(v0, v1);
                    let node = SmartPunctuation {
                        kind,
                        glyph,
                        source: self.c.slice(v0, v1),
                        pos,
                        ..Default::default()
                    };
                    self.add(Node::SmartPunctuation(self.a.alloc(node)));
                }
            }
        }
        self.outs = outs;
    }
}
