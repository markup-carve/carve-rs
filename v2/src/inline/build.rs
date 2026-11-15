//! The builder: events to typed inline nodes with positions (C2-A-138 to
//! C2-A-158), adjacent text merged (C2-A-040), smart punctuation and
//! abbreviations split out of plain text.

use super::content::Content;
use super::delim::Ctx;
use super::events::Ev;
use super::smart::{Out, Prev, Smart};
use super::token::Kind;
use super::{InlineEnv, abbr};
use crate::ast::{
    Abbreviation, CommentForm, HardBreak, InlineExtension, Link, Node, NonBreakingSpace, Pos,
    SmartPunctuation, SoftBreak, Text,
};
use crate::attr::AttrBlock;
use crate::tables;

struct Frame {
    kind: Kind,
    v0: usize,
    v1: usize,
    children: Vec<Node>,
    /// Past the depth cap (C2-I-118): written as its source.
    dead: bool,
    /// The single-quote state outside an inline footnote (C2-I-109).
    saved_single: Option<bool>,
}

struct Builder<'a, 'e> {
    c: &'a Content<'e>,
    ctx: &'a Ctx<'a>,
    env: &'a InlineEnv<'e>,
    root: Vec<Node>,
    frames: Vec<Frame>,
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
pub fn build(
    c: &Content<'_>,
    ctx: &Ctx<'_>,
    env: &InlineEnv<'_>,
    evs: &mut Vec<Ev>,
    outs: &mut Vec<Out>,
) -> Vec<Node> {
    let next_flow = if env.abbreviations.is_empty() {
        Vec::new()
    } else {
        next_flow(ctx.t, evs)
    };
    let mut b = Builder {
        c,
        ctx,
        env,
        root: Vec::new(),
        frames: Vec::new(),
        smart: Smart::new(),
        flow_prev: None,
        comment_to: None,
        outs: std::mem::take(outs),
    };
    for (i, ev) in evs.drain(..).enumerate() {
        b.event(ev, next_flow.get(i).copied().flatten());
    }
    while let Some(f) = b.frames.pop() {
        let end = f.v1;
        b.exit(f, end, end, None);
    }
    *outs = b.outs;
    b.root
}

fn first_char(t: &str, ev: &Ev) -> Option<Option<char>> {
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
fn next_flow(t: &str, evs: &[Ev]) -> Vec<Option<char>> {
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

fn set_pos(node: &mut Node, pos: Option<Pos>) {
    if let Some(slot) = node.pos_slot() {
        *slot = pos;
    }
}

fn apply_attrs(node: &mut Node, attrs: Option<AttrBlock>) {
    let Some(block) = attrs else { return };
    if !block.attrs.is_empty()
        && let Some(slot) = node.attrs_slot()
    {
        *slot = Some(block.attrs);
    }
    if block.lang.is_some() {
        set_lang(node, block.lang);
    }
}

/// The `lang` part of an attribute block on a host (C2-I-029, C2-I-036).
pub fn set_lang(node: &mut Node, lang: Option<String>) {
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
fn styled(name: &str) -> Option<Node> {
    Some(match name {
        "delete" => Node::Delete(Box::default()),
        "emphasis" => Node::Emphasis(Box::default()),
        "highlight" => Node::Highlight(Box::default()),
        "insert" => Node::Insert(Box::default()),
        "strike" => Node::Strike(Box::default()),
        "strong" => Node::Strong(Box::default()),
        "subscript" => Node::Subscript(Box::default()),
        "superscript" => Node::Superscript(Box::default()),
        "underline" => Node::Underline(Box::default()),
        _ => return None,
    })
}

/// A container node of `kind`, empty.
fn container(kind: Kind) -> Option<Node> {
    Some(match kind {
        Kind::Styled(name) => styled(name)?,
        Kind::Span => Node::Span(Box::default()),
        Kind::Link {
            href,
            tooltip,
            key,
            form,
        } => {
            let namespace = key.as_ref().map(|_| "link".to_string());
            Node::Link(Box::new(Link {
                href,
                tooltip,
                key,
                namespace,
                ref_form: form,
                ..Default::default()
            }))
        }
        Kind::InlineFootnote => Node::InlineFootnote(Box::default()),
        Kind::Extension(name) => Node::InlineExtension(Box::new(InlineExtension {
            name,
            ..Default::default()
        })),
    })
}

fn push_node(list: &mut Vec<Node>, node: Node) {
    if let Node::Text(new) = &node
        && let Some(Node::Text(last)) = list.last_mut()
    {
        last.value.push_str(&new.value);
        last.pos = joined(last.pos, new.pos);
        return;
    }
    list.push(node);
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

impl<'a> Builder<'a, '_> {
    fn out(&mut self) -> &mut Vec<Node> {
        match self.frames.last_mut() {
            Some(f) => &mut f.children,
            None => &mut self.root,
        }
    }

    fn add(&mut self, node: Node) {
        push_node(self.out(), node);
    }

    /// C2-A-158: the inline nodes of a cell that a continuation row joined
    /// omit `pos` (GAP INL-010: the oracle omits it on every one of them).
    fn pos(&self, v0: usize, v1: usize) -> Option<Pos> {
        (!self.c.cell_joined()).then(|| self.c.pos(v0, v1))
    }

    /// Adds text, appended to a text node before it (C2-A-040).
    fn text(&mut self, v0: usize, v1: usize, value: &str) {
        let pos = if self.c.contiguous(v0, v1) {
            self.pos(v0, v1)
        } else {
            None
        };
        if let Some(Node::Text(last)) = self.out().last_mut() {
            last.value.push_str(value);
            last.pos = joined(last.pos, pos);
            return;
        }
        self.out().push(Node::Text(Box::new(Text {
            value: value.to_string(),
            pos,
            ..Default::default()
        })));
    }

    /// The source text of a content range.
    fn src(&self, v0: usize, v1: usize) -> &'a str {
        self.ctx.t.get(v0..v1).unwrap_or("")
    }

    fn slice(&self, v0: usize, v1: usize) -> String {
        self.ctx.t.get(v0..v1).unwrap_or("").to_string()
    }

    fn event(&mut self, ev: Ev, next_flow: Option<char>) {
        let comment_to = self.comment_to.take();
        match ev {
            Ev::Enter { kind, v0, v1 } => self.enter(kind, v0, v1),
            Ev::Exit { v0, v1, attrs, .. } => {
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
                self.add(Node::NonBreakingSpace(Box::new(NonBreakingSpace {
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
                    Node::HardBreak(Box::new(HardBreak {
                        pos,
                        ..Default::default()
                    }))
                } else {
                    Node::SoftBreak(Box::new(SoftBreak {
                        pos,
                        ..Default::default()
                    }))
                });
            }
            Ev::CellJoin { .. } => {
                self.space_prev(' ');
                self.add(Node::Text(Box::new(Text {
                    value: " ".to_string(),
                    ..Default::default()
                })));
            }
            Ev::Leaf {
                v0,
                v1,
                node,
                transparent,
            } => self.leaf(*node, v0, v1, transparent),
        }
    }

    fn space_prev(&mut self, c: char) {
        self.smart.prev = Prev::Char(c);
        self.flow_prev = Some(c);
    }

    fn leaf(&mut self, mut node: Node, v0: usize, v1: usize, transparent: bool) {
        if !transparent {
            self.smart.prev = Prev::Word;
            self.flow_prev = None;
        }
        // C2-A-158: a caption number has no source text of its own.
        let pos = match &node {
            Node::CaptionNumber(_) => None,
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
        let dead = self.env.depth + open >= tables::DEPTH_CAP;
        let saved_single = (kind == Kind::InlineFootnote)
            .then(|| std::mem::replace(&mut self.smart.single_open, false));
        if saved_single.is_some() {
            // GAP INL-013: an inline footnote's edges bound abbreviations.
            self.flow_prev = None;
        }
        if dead {
            let t = self.src(v0, v1);
            self.text(v0, v1, t);
        }
        self.smart.prev = Prev::Start;
        self.frames.push(Frame {
            kind,
            v0,
            v1,
            children: Vec::new(),
            dead,
            saved_single,
        });
    }

    fn exit(&mut self, f: Frame, v0: usize, v1: usize, attrs: Option<AttrBlock>) {
        if let Some(s) = f.saved_single {
            self.smart.single_open = s;
            self.flow_prev = None;
        }
        self.smart.prev = Prev::Word;
        if f.dead {
            for ch in f.children {
                self.add(ch);
            }
            let t = self.src(v0, v1);
            self.text(v0, v1, t);
            return;
        }
        let pos = self.pos(f.v0, v1);
        let Some(mut node) = container(f.kind) else {
            return;
        };
        if let Some(ch) = node.children_mut() {
            *ch = f.children;
        }
        apply_attrs(&mut node, attrs);
        set_pos(&mut node, pos);
        self.add(node);
    }

    /// Plain text: abbreviations first, smart typography in what is left.
    fn plain(&mut self, v0: usize, v1: usize, next_flow: Option<char>) {
        let s = self.ctx.t.get(v0..v1).unwrap_or("");
        let hits = abbr::find(s, self.flow_prev, next_flow, self.env.abbreviations);
        let mut from = v0;
        for (a, b, idx) in hits {
            self.smart_text(from, v0 + a);
            let (key, expansion) = self.env.abbreviations.get(idx).cloned().unwrap_or_default();
            let pos = self.pos(v0 + a, v0 + b);
            let node = Abbreviation {
                key,
                expansion: Some(expansion),
                namespace: "abbr".to_string(),
                pos,
                ..Default::default()
            };
            self.add(Node::Abbreviation(Box::new(node)));
            if let Some(last) = s.get(..b).and_then(|x| x.chars().next_back()) {
                self.smart.prev = Prev::Char(last);
            }
            from = v0 + b;
        }
        self.smart_text(from, v1);
        if let Some(last) = s.chars().next_back() {
            self.flow_prev = Some(last);
        }
    }

    fn smart_text(&mut self, v0: usize, v1: usize) {
        if v0 >= v1 {
            return;
        }
        let mut outs = std::mem::take(&mut self.outs);
        self.smart.run(self.ctx, v0, v1, &mut outs);
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
                        glyph: Some(glyph.to_string()),
                        source: self.slice(v0, v1),
                        pos,
                        ..Default::default()
                    };
                    self.add(Node::SmartPunctuation(Box::new(node)));
                }
            }
        }
        self.outs = outs;
    }
}
