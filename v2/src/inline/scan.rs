//! Step one and two of the scan (C2-I-004): atoms and the bracket stack, in
//! one left-to-right pass. Delimiters are only recorded here; they pair in
//! `delim.rs` once every bracket construct is settled (C2-I-007).

use super::atoms::{self, Angle, Label};
use super::content::Content;
use super::token::{Delim, K, Kind, Sort, Tok};
use super::{ContentKind, InlineEnv, Join};
use crate::ast::{
    Autolink, Code, Comment, CommentForm, CriticComment, FootnoteRef, Image, ImageRefForm,
    LinkRefForm, LiteralInline, Math, Mention, Node, RawInline, Symbol, Tag, Xref, XrefNamespace,
};
use crate::chars::{ByteSet, fold_label, is_flank_space, is_joining, is_word};
use crate::tables;

#[derive(Debug, Clone, PartialEq, Eq)]
enum BKind {
    Plain,
    /// `![`; `bang` is the token of the `!`.
    Image {
        bang: usize,
    },
    Footnote,
    Ext(String),
}

struct Bracket {
    tok: usize,
    kind: BKind,
    /// Offset behind the opener markup.
    inner: usize,
    delim_base: usize,
    last_star: Option<usize>,
    last_slash: Option<usize>,
}

#[derive(Clone, Copy)]
struct DepthMark {
    mark: char,
    braced: bool,
    at: usize,
}

/// The result of the scan.
pub struct Scanned<'a> {
    pub toks: Vec<Tok<'a>>,
    /// Content ranges of braced comments, in order (C2-I-026).
    pub comments: Vec<(usize, usize)>,
}

struct Scanner<'r, 'e, 'a> {
    c: &'r Content<'e, 'a>,
    t: &'r str,
    b: &'r [u8],
    env: &'r InlineEnv<'e, 'a>,
    toks: Vec<Tok<'a>>,
    text_from: Option<usize>,
    brackets: Vec<Bracket>,
    open_delims: Vec<DepthMark>,
    last_star: Option<usize>,
    last_slash: Option<usize>,
    /// No unescaped `]` follows this offset.
    no_label_from: Option<usize>,
    /// An unquoted value run behind which attribute blocks fail.
    attr_fail: Option<(usize, usize)>,
    comments: Vec<(usize, usize)>,
    /// A comment-only line ends an atom opened above it (C2-B-138).
    barriers: Vec<usize>,
    /// Last start of `%}` and of `#}`, to keep the closer search linear;
    /// searched when the first opener asks.
    last_pct: Option<Option<usize>>,
    last_hash: Option<Option<usize>>,
    caption_done: bool,
    i: usize,
}

/// Runs the scan over one content.
#[must_use]
pub fn scan<'a>(
    c: &Content<'_, 'a>,
    env: &InlineEnv<'_, 'a>,
    mut toks: Vec<Tok<'a>>,
) -> Scanned<'a> {
    let t = c.text.as_str();
    toks.clear();
    // Each byte that starts a form makes about one token and ends a text run.
    toks.reserve(2 * t.bytes().filter(|&b| FORM_START.has(b)).count() + 1);
    let mut s = Scanner {
        c,
        t,
        b: t.as_bytes(),
        env,
        toks,
        text_from: None,
        brackets: Vec::new(),
        open_delims: Vec::new(),
        last_star: None,
        last_slash: None,
        no_label_from: None,
        attr_fail: None,
        comments: Vec::new(),
        barriers: comment_only_starts(t),
        last_pct: None,
        last_hash: None,
        caption_done: env.kind != ContentKind::Caption,
        i: 0,
    };
    while s.i < s.b.len() {
        if !s.step() {
            s.text_char();
            s.skip_plain();
        }
    }
    s.flush();
    s.finish_brackets();
    Scanned {
        toks: s.toks,
        comments: s.comments,
    }
}

fn comment_only_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if crate::block::shapes::comment_only(line.trim_start_matches([' ', '\t'])) {
            starts.push(at);
        }
        at += line.len();
    }
    starts
}

/// The first bytes `Scanner::step` reads a form at.
static FORM_START: ByteSet = ByteSet::new(
    b"\\\n`!$\x7b%<@#:[^]*/",
    tables::BRACED_MARKS.as_bytes(),
    None,
);

impl<'a> Scanner<'_, '_, 'a> {
    fn at(&self, i: usize) -> Option<u8> {
        self.b.get(i).copied()
    }

    fn char_at(&self, i: usize) -> Option<char> {
        self.t.get(i..).and_then(|s| s.chars().next())
    }

    /// The character before `i`, a braced comment skipped (C2-I-026).
    fn char_before(&self, i: usize) -> Option<char> {
        super::delim::char_before(self.t, &self.comments, i)
    }

    fn text_char(&mut self) {
        if self.text_from.is_none() {
            self.text_from = Some(self.i);
        }
        self.i += self.char_at(self.i).map_or(1, char::len_utf8);
    }

    /// Moves over bytes no form starts with: `step` is false on each of them.
    fn skip_plain(&mut self) {
        let rest = self.b.get(self.i..).unwrap_or(&[]);
        self.i += FORM_START.find(rest).unwrap_or(rest.len());
    }

    fn flush(&mut self) {
        if let Some(f) = self.text_from.take()
            && f < self.i
        {
            self.toks.push(Tok::new(
                f,
                self.i,
                K::Text {
                    val: None,
                    plain: true,
                },
            ));
        }
    }

    fn push(&mut self, v0: usize, v1: usize, k: K<'a>) -> usize {
        self.flush();
        self.toks.push(Tok::new(v0, v1, k));
        self.i = v1;
        self.toks.len() - 1
    }

    fn leaf(&mut self, v0: usize, v1: usize, node: Node<'a>, host: bool) -> usize {
        self.push(
            v0,
            v1,
            K::Leaf {
                node,
                host,
                transparent: false,
            },
        )
    }

    /// Tries every form at the current offset; false means plain text.
    fn step(&mut self) -> bool {
        let i = self.i;
        match self.b.get(i).copied().unwrap_or(0) {
            b'\\' => self.backslash(),
            b'\n' => self.line_break(),
            b'`' => self.code(i, i, CodeKind::Plain),
            b'!' => self.bang(),
            b'$' => self.dollar(),
            b'{' => self.brace(),
            b'%' => self.line_comment(),
            b'<' => self.angle(),
            c => self.step_marks(i, c),
        }
    }

    #[inline]
    fn step_marks(&mut self, i: usize, c: u8) -> bool {
        match c {
            b'@' | b'#' => self.sigil(),
            b':' => self.colon(),
            b'[' => self.open_bracket(),
            b'^' if self.at(i + 1) == Some(b'[') && !self.in_inline_note() => {
                self.push_bracket(i, i + 2, BKind::Footnote)
            }
            b']' => self.close_bracket(),
            b'*' | b'/' => self.bare(),
            c if tables::BRACED_MARKS.as_bytes().contains(&c) => self.mark_run(c),
            _ => false,
        }
    }

    fn backslash(&mut self) -> bool {
        let i = self.i;
        match self.char_at(i + 1) {
            None => {
                self.push(i, i + 1, K::Break { hard: true });
            }
            Some('\n') => {
                self.push(i, i + 2, K::Break { hard: true });
            }
            Some(' ') => {
                if self.char_at(i + 2) == Some('\n') {
                    self.push(i, i + 3, K::Break { hard: true });
                } else if self.char_at(i + 2).is_none() {
                    self.push(i, i + 2, K::Break { hard: true });
                } else {
                    self.push(i, i + 2, K::Nbsp);
                }
            }
            Some(p) if p.is_ascii_punctuation() => {
                self.push(
                    i,
                    i + 2,
                    K::Text {
                        val: Some(p.to_string()),
                        plain: false,
                    },
                );
            }
            Some(_) => return false,
        }
        true
    }

    fn line_break(&mut self) -> bool {
        let i = self.i;
        let k = match self.c.join_at(i) {
            Some(Join::Cell) => K::CellJoin,
            _ => K::Break {
                hard: self.env.hard_breaks,
            },
        };
        self.push(i, i + 1, k);
        true
    }

    fn code(&mut self, v0: usize, at: usize, kind: CodeKind) -> bool {
        let span = atoms::code_span(self.t.get(..self.atom_limit(at)).unwrap_or(self.t), at);
        if !span.closed {
            // C2-I-017: an unclosed run is text in every content.
            let run = self
                .b
                .get(at..)
                .map_or(0, |r| r.iter().take_while(|&&x| x == b'`').count());
            if self.text_from.is_none() {
                self.text_from = Some(v0);
            }
            self.i = at + run;
            return true;
        }
        let value = span.value;
        let end = span.end;
        let node = match kind {
            CodeKind::Plain => {
                if span.closed
                    && let Some((format, e)) = atoms::raw_marker(self.t, end)
                {
                    self.leaf(
                        v0,
                        e,
                        Node::RawInline(self.c.arena().alloc(RawInline {
                            format: self.c.keep(&format),
                            value: self.c.keep(value),
                            ..Default::default()
                        })),
                        false,
                    );
                    return true;
                }
                Node::Code(self.c.arena().alloc(Code {
                    value: self.c.keep(value),
                    ..Default::default()
                }))
            }
            CodeKind::Literal => Node::LiteralInline(self.c.arena().alloc(LiteralInline {
                value: self.c.keep(value),
                ..Default::default()
            })),
            CodeKind::Math(display) => Node::Math(self.c.arena().alloc(Math {
                value: self.c.keep(value),
                display,
                ..Default::default()
            })),
        };
        self.leaf(v0, end, node, true);
        true
    }

    fn bang(&mut self) -> bool {
        let i = self.i;
        match self.at(i + 1) {
            Some(b'`') => self.code(i, i + 1, CodeKind::Literal),
            Some(b'[') => {
                let bang = self.push(
                    i,
                    i + 1,
                    K::Text {
                        val: None,
                        plain: true,
                    },
                );
                self.push_bracket(i + 1, i + 2, BKind::Image { bang })
            }
            _ => false,
        }
    }

    fn dollar(&mut self) -> bool {
        let i = self.i;
        match (self.at(i + 1), self.at(i + 2)) {
            (Some(b'`'), _) => self.code(i, i + 1, CodeKind::Math(false)),
            (Some(b'$'), Some(b'`')) => self.code(i, i + 2, CodeKind::Math(true)),
            _ => false,
        }
    }

    /// `attr::parse_block` with a memo that keeps the scan linear: a failed
    /// block whose first part is `key=value` fails behind the end of that
    /// unquoted run, so every block that starts inside the run and ends its
    /// first value there fails too (`{k={k={k=...`).
    fn attr_block(&mut self, i: usize) -> Option<(crate::attr::AttrBlock, usize)> {
        let eq = key_eq(self.b, i);
        if let (Some((a, r)), Some(e)) = (self.attr_fail, eq)
            && i >= a
            && e < r
        {
            return None;
        }
        let res = crate::attr::parse_block(self.t, i);
        if res.is_none()
            && let Some(e) = eq
        {
            let r = self.b.get(e + 1..).map_or(e + 1, |rest| {
                e + 1
                    + rest
                        .iter()
                        .take_while(|&&c| {
                            !matches!(c, b' ' | b'\t' | b'"' | b'\'' | b'\\' | b'|' | b'}' | b'\n')
                        })
                        .count()
            });
            if r > e + 1 {
                self.attr_fail = Some((e + 1, r));
            }
        }
        res
    }

    /// C2-I-043: the opening brace, first rule that matches.
    fn brace(&mut self) -> bool {
        let i = self.i;
        if let Some((block, end)) = self.attr_block(i) {
            if block.empty {
                return false;
            }
            self.push(i, end, K::Attr(Box::new(block)));
            return true;
        }
        let mark = self.at(i + 1).unwrap_or(0);
        let after_mark = self.char_at(i + 2);
        if after_mark.is_some_and(is_joining) {
            return false;
        }
        match mark {
            b'%' if self.braced_comment() => true,
            b'#' if self.editorial() => true,
            m if tables::BRACED_MARKS.as_bytes().contains(&m) => {
                let tok = self.push(i, i + 2, K::Delim(Delim::BracedOpen(char::from(m))));
                if !self.depth_open(char::from(m), i) {
                    if let Some(token) = self.toks.get_mut(tok) {
                        token.make_text(true);
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn braced_comment(&mut self) -> bool {
        let i = self.i;
        let t = self.t;
        let limit = self.atom_limit(i);
        if self
            .last_pct
            .get_or_insert_with(|| t.rfind("%}"))
            .is_none_or(|l| l < i + 2)
        {
            return false;
        }
        let Some(k) = self
            .t
            .get(i + 2..limit)
            .and_then(|s| s.find("%}"))
            .map(|k| k + i + 2)
        else {
            return false;
        };
        let raw = self.t.get(i + 2..k).unwrap_or("");
        let raw = raw.strip_prefix(' ').unwrap_or(raw);
        let raw = raw.strip_suffix(' ').unwrap_or(raw);
        let node = Node::Comment(self.c.arena().alloc(Comment {
            form: CommentForm::Braced,
            value: self.c.keep(raw),
            ..Default::default()
        }));
        self.push(
            i,
            k + 2,
            K::Leaf {
                node,
                host: false,
                transparent: true,
            },
        );
        self.comments.push((i, k + 2));
        true
    }

    fn atom_limit(&self, at: usize) -> usize {
        let i = self.barriers.partition_point(|&start| start <= at);
        self.barriers.get(i).copied().unwrap_or(self.t.len())
    }

    fn editorial(&mut self) -> bool {
        let i = self.i;
        let t = self.t;
        if self
            .last_hash
            .get_or_insert_with(|| t.rfind("#}"))
            .is_none_or(|l| l < i + 2)
        {
            return false;
        }
        let Some(k) = self
            .t
            .get(i + 2..)
            .and_then(|s| s.find("#}"))
            .map(|k| k + i + 2)
        else {
            return false;
        };
        if k == i + 2 {
            return false;
        }
        let value = self.c.slice(i + 2, k);
        self.leaf(
            i,
            k + 2,
            Node::CriticComment(self.c.arena().alloc(CriticComment {
                value,
                ..Default::default()
            })),
            false,
        );
        true
    }

    /// C2-I-023: a line comment takes the spaces before it and the rest of its line.
    fn line_comment(&mut self) -> bool {
        let i = self.i;
        let at_content_start = self
            .brackets
            .last()
            .is_some_and(|bracket| bracket.inner == i)
            || self
                .open_delims
                .last()
                .is_some_and(|mark| mark.braced && mark.at + 2 == i);
        if self.env.kind == ContentKind::LineBlockLine
            || !(atoms::comment_start(self.t, i) || at_content_start)
        {
            return false;
        }
        let line_end = self
            .t
            .get(i..)
            .and_then(|s| s.find('\n'))
            .map_or(self.t.len(), |e| e + i);
        let in_reference_label = self
            .brackets
            .last()
            .and_then(|bracket| self.toks.get(bracket.tok))
            .is_some_and(|token| token.v0 > 0 && self.at(token.v0 - 1) == Some(b']'));
        let end = if in_reference_label {
            line_end
        } else {
            self.comment_closer(i + 2, line_end).unwrap_or(line_end)
        };
        let mut start = i;
        if let Some(f) = self.text_from {
            while start > f && matches!(self.at(start - 1), Some(b' ' | b'\t')) {
                start -= 1;
            }
        }
        let raw = self.t.get(i + 2..end).unwrap_or("");
        let raw = raw
            .strip_prefix(' ')
            .unwrap_or(raw)
            .trim_end_matches([' ', '\t']);
        let node = Node::Comment(self.c.arena().alloc(Comment {
            form: CommentForm::Line,
            value: self.c.keep(raw),
            ..Default::default()
        }));
        self.i = start;
        self.leaf(start, end, node, false);
        true
    }

    fn comment_closer(&self, from: usize, end: usize) -> Option<usize> {
        let bracket = self.brackets.last();
        let braced = self.open_delims.iter().rev().find(|mark| mark.braced);
        match (bracket, braced) {
            (Some(b), Some(m)) if m.at >= b.inner => {
                find_braced_comment_close(self.t, from, end, m.mark)
            }
            (Some(_), _) => find_bracket_comment_close(self.t, from, end),
            (None, Some(m)) => find_braced_comment_close(self.t, from, end, m.mark),
            (None, None) => None,
        }
    }

    fn angle(&mut self) -> bool {
        let i = self.i;
        match atoms::angle(self.t, i) {
            Some(Angle::Xref { key, end }) => {
                let node = Node::Xref(self.c.arena().alloc(Xref {
                    key: self.c.keep(&key),
                    namespace: XrefNamespace::Heading,
                    ..Default::default()
                }));
                self.leaf(i, end, node, false);
                true
            }
            Some(Angle::Autolink { href, body, end }) => {
                // GAP INL-001: `text` only where the visible text differs from `href`.
                let text = (body != href).then(|| self.c.keep(&body));
                self.leaf(
                    i,
                    end,
                    Node::Autolink(self.c.arena().alloc(Autolink {
                        href: self.c.keep(&href),
                        text,
                        ..Default::default()
                    })),
                    true,
                );
                true
            }
            None => false,
        }
    }

    /// C2-I-090: a sigil starts a form at the start or after a non-letter-or-number.
    fn boundary(&self, i: usize) -> bool {
        self.char_before(i).is_none_or(|c| !is_word(c))
    }

    fn sigil(&mut self) -> bool {
        let i = self.i;
        let at = self.at(i) == Some(b'@');
        if !at && !self.caption_done && self.caption_mark(i) {
            return true;
        }
        if !self.boundary(i) {
            return false;
        }
        let Some(end) = atoms::sigil_name(self.t, i + 1) else {
            return false;
        };
        let name = self.c.slice(i + 1, end);
        let node = if at {
            Node::Mention(self.c.arena().alloc(Mention {
                target: name,
                kind: "user",
                ..Default::default()
            }))
        } else {
            Node::Tag(self.c.arena().alloc(Tag {
                name,
                ..Default::default()
            }))
        };
        self.leaf(i, end, node, false);
        true
    }

    /// C2-B-207: a bare `#` in a caption is a mark candidate; the delimiter
    /// pass keeps the first one outside markup.
    fn caption_mark(&mut self, i: usize) -> bool {
        let bare = self
            .at(i + 1)
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == b'_' || c == b'-'));
        if bare {
            self.push(i, i + 1, K::CaptionMark);
        }
        bare
    }

    fn colon(&mut self) -> bool {
        let i = self.i;
        if !self.boundary(i) {
            return false;
        }
        if let Some(end) = atoms::symbol(self.t, i) {
            let name = self.c.slice(i + 1, end);
            self.leaf(
                i,
                end + 1,
                Node::Symbol(self.c.arena().alloc(Symbol {
                    name,
                    ..Default::default()
                })),
                true,
            );
            return true;
        }
        if let Some(br) = atoms::extension(self.t, i) {
            let name = self.t.get(i + 1..br).unwrap_or("").to_string();
            return self.push_bracket(i, br + 1, BKind::Ext(name));
        }
        false
    }

    fn open_bracket(&mut self) -> bool {
        let i = self.i;
        if self.at(i + 1) == Some(b'^') && !self.in_inline_note() {
            return self.footnote_ref(i);
        }
        self.push_bracket(i, i + 1, BKind::Plain)
    }

    fn in_inline_note(&self) -> bool {
        self.brackets
            .iter()
            .any(|bracket| matches!(bracket.kind, BKind::Footnote))
    }

    /// C2-I-087: `[^label]`; with no label the `[` is text.
    fn footnote_ref(&mut self, i: usize) -> bool {
        if let Label::Ok(e, end) = self.label(i + 2) {
            let key = self.label_key(i + 2, e);
            if !key.is_empty() {
                let node = Node::FootnoteRef(self.c.arena().alloc(FootnoteRef {
                    key: self.c.keep(&key),
                    namespace: "footnote",
                    ..Default::default()
                }));
                self.leaf(i, end, node, true);
                return true;
            }
        }
        self.text_char();
        true
    }

    /// A label from `at`; once no `]` follows, none follows later either.
    fn label(&mut self, at: usize) -> Label {
        if self.no_label_from.is_some_and(|n| n <= at) {
            return Label::End;
        }
        let l = atoms::label(self.t, at, self.comments_on());
        if matches!(l, Label::End) {
            self.no_label_from = Some(at);
        }
        l
    }

    fn comments_on(&self) -> bool {
        self.env.kind != ContentKind::LineBlockLine
    }

    /// C2-I-079 with C2-B-224: the folded key of a label source.
    fn label_key(&self, a: usize, b: usize) -> String {
        let raw = self.t.get(a..b).unwrap_or("");
        if self.env.kind == ContentKind::Cell {
            fold_label(&raw.replace("\\|", "|"))
        } else {
            fold_label(raw)
        }
    }

    /// C2-I-118: a capped opener is text and takes no closer.
    fn push_bracket(&mut self, v0: usize, v1: usize, kind: BKind) -> bool {
        let open = self.brackets.len() + self.open_delims.len();
        let delim_base = self.open_delims.len();
        if open >= tables::INLINE_DEPTH {
            self.flush();
            self.text_from = Some(v0);
            self.i = v1;
            return true;
        }
        let tok = self.push(v0, v1, K::Pending);
        self.brackets.push(Bracket {
            tok,
            kind,
            inner: v1,
            delim_base,
            last_star: self.last_star.take(),
            last_slash: self.last_slash.take(),
        });
        true
    }

    fn close_bracket(&mut self) -> bool {
        let Some(top) = self.brackets.pop() else {
            return false;
        };
        let i = self.i;
        let consumed = match top.kind.clone() {
            BKind::Footnote => {
                self.form(&top, i, i + 1, Kind::InlineFootnote, None);
                true
            }
            BKind::Ext(name) => {
                self.form(&top, i, i + 1, Kind::Extension(name), None);
                true
            }
            BKind::Plain | BKind::Image { .. } => {
                self.close_pair(&top, i);
                true
            }
        };
        if self
            .toks
            .get(top.tok)
            .is_some_and(|t| matches!(t.k, K::Open { .. }))
        {
            self.open_delims.truncate(top.delim_base);
            self.last_star = top.last_star;
            self.last_slash = top.last_slash;
        } else {
            self.last_star = self.last_star.or(top.last_star);
            self.last_slash = self.last_slash.or(top.last_slash);
        }
        consumed
    }

    fn form(
        &mut self,
        top: &Bracket,
        v0: usize,
        v1: usize,
        kind: Kind,
        attrs: Option<crate::attr::AttrBlock>,
    ) {
        let close = self.push(
            v0,
            v1,
            K::Close {
                open: top.tok,
                attrs: attrs.map(Box::new),
            },
        );
        if let Some(t) = self.toks.get_mut(top.tok) {
            t.k = K::Open {
                kind,
                sort: Sort::Bracket,
                close,
            };
        }
    }

    fn fail_pair(&mut self, top: &Bracket) {
        if let Some(t) = self.toks.get_mut(top.tok) {
            t.make_text(true);
        }
        self.text_char();
    }

    /// C2-I-006: what is glued to the closing bracket decides the construct.
    fn close_pair(&mut self, top: &Bracket, i: usize) {
        match self.at(i + 1) {
            Some(b'(') => match atoms::link_tail(self.t, i + 1, self.env.kind == ContentKind::Cell)
            {
                Some(tail) => {
                    let target = Target::Direct {
                        href: tail.dest,
                        tooltip: tail.title,
                    };
                    self.link(top, i, tail.end, target);
                }
                None => self.fail_pair(top),
            },
            Some(b'[') => self.reference(top, i),
            Some(b'{') if self.span(top, i) => {}
            _ => {
                if !self.shortcut(top, i) {
                    self.fail_pair(top);
                }
            }
        }
    }

    fn reference(&mut self, top: &Bracket, i: usize) {
        if self.at(i + 2) == Some(b']') {
            let key = self.label_key(top.inner, i);
            if key.is_empty() {
                self.fail_pair(top);
            } else {
                self.link(
                    top,
                    i,
                    i + 3,
                    Target::Ref {
                        key,
                        form: LinkRefForm::Collapsed,
                    },
                );
            }
            return;
        }
        // C2-I-078: a label starting with an at-sign or a caret is none.
        if matches!(self.at(i + 2), Some(b'@' | b'^')) {
            self.fail_pair(top);
            return;
        }
        if let Label::Ok(e, end) = self.label(i + 2) {
            let key = self.label_key(i + 2, e);
            if !key.is_empty() {
                self.link(
                    top,
                    i,
                    end,
                    Target::Ref {
                        key,
                        form: LinkRefForm::Full,
                    },
                );
                return;
            }
        }
        // GAP INL-004: an unclosed or empty label still lets the pair be a shortcut.
        if !self.shortcut(top, i) {
            self.fail_pair(top);
        }
    }

    /// C2-I-064: a bracket pair with a glued attribute run is a span.
    fn span(&mut self, top: &Bracket, i: usize) -> bool {
        let Some((mut block, mut end)) = self.attr_block(i + 1) else {
            return false;
        };
        while let Some((next, e)) = self.attr_block(end) {
            block.merge(next);
            end = e;
        }
        self.form(top, i, end, Kind::Span, Some(block));
        true
    }

    /// C2-I-072, C2-I-101: a pair whose folded text is a defined key.
    fn shortcut(&mut self, top: &Bracket, i: usize) -> bool {
        let raw = self.t.get(top.inner..i).unwrap_or("");
        if raw.starts_with('@') {
            return false;
        }
        let key = self.label_key(top.inner, i);
        if key.is_empty() || !self.env.link_keys.contains(&key) {
            return false;
        }
        self.link(
            top,
            i,
            i + 1,
            Target::Ref {
                key,
                form: LinkRefForm::Shortcut,
            },
        );
        true
    }

    fn link(&mut self, top: &Bracket, i: usize, end: usize, target: Target) {
        if let BKind::Image { bang } = top.kind {
            self.image(top, bang, i, end, target);
            return;
        }
        let kind = match target {
            Target::Direct { href, tooltip } => Kind::Link {
                href: Some(href),
                tooltip,
                key: None,
                form: None,
            },
            Target::Ref { key, form } => Kind::Link {
                href: None,
                tooltip: None,
                key: Some(key),
                form: Some(form),
            },
        };
        self.form(top, i, end, kind, None);
    }

    /// An image is one leaf; its alt is the source of the bracket content.
    fn image(&mut self, top: &Bracket, bang: usize, i: usize, end: usize, target: Target) {
        self.flush();
        let alt = self.t.get(top.inner..i).unwrap_or("");
        let alt = if alt.contains('\\') {
            self.c.arena().str(&unescape_alt(alt))
        } else {
            self.c.keep(alt)
        };
        let mut img = Image {
            alt: Some(alt),
            ..Default::default()
        };
        match target {
            Target::Direct { href, tooltip } => {
                img.src = Some(self.c.keep(&href));
                img.tooltip = tooltip.map(|t| self.c.keep(&t));
            }
            Target::Ref { key, form } => {
                img.key = Some(self.c.keep(&key));
                img.namespace = Some("link");
                img.ref_form = Some(match form {
                    LinkRefForm::Full => ImageRefForm::Full,
                    LinkRefForm::Collapsed => ImageRefForm::Collapsed,
                    LinkRefForm::Shortcut => ImageRefForm::Shortcut,
                });
            }
        }
        let v0 = self.toks.get(bang).map_or(i, |t| t.v0);
        for t in self.toks.iter_mut().skip(bang) {
            t.k = K::Skip;
        }
        if let Some(t) = self.toks.get_mut(bang) {
            *t = Tok::new(
                v0,
                end,
                K::Leaf {
                    node: Node::Image(self.c.arena().alloc(img)),
                    host: true,
                    transparent: false,
                },
            );
        }
        self.i = end;
    }

    fn finish_brackets(&mut self) {
        for top in std::mem::take(&mut self.brackets) {
            if let Some(t) = self.toks.get_mut(top.tok) {
                t.make_text(true);
            }
        }
    }

    /// A bare `*` or `/` (C2-I-046, C2-I-049, C2-I-050).
    fn bare(&mut self) -> bool {
        let i = self.i;
        let c = self.at(i).unwrap_or(0);
        let n = self
            .b
            .get(i..)
            .map_or(0, |s| s.iter().take_while(|&&x| x == c).count());
        if self.at(i + n) == Some(b'}') {
            return self.mark_run(c);
        }
        if n >= 2 {
            for _ in 0..n {
                self.text_char();
            }
            return true;
        }
        if self.char_at(i + 1).is_some_and(is_joining) {
            return false;
        }
        let tok = self.push(i, i + 1, K::Delim(Delim::Bare(char::from(c))));
        if !self.depth_bare(i, char::from(c)) {
            if let Some(token) = self.toks.get_mut(tok) {
                token.make_text(true);
            }
        }
        true
    }

    /// C2-I-057: a run of marks before `}`; only the last is the closer.
    fn mark_run(&mut self, c: u8) -> bool {
        let i = self.i;
        let n = self
            .b
            .get(i..)
            .map_or(0, |s| s.iter().take_while(|&&x| x == c).count());
        if self.at(i + n) != Some(b'}') {
            // Linear: the run is text but its last mark is read again (`^[`).
            for _ in 1..n {
                self.text_char();
            }
            return n > 1;
        }
        for _ in 1..n {
            self.text_char();
        }
        let at = i + n - 1;
        let tok = self.push(
            at,
            at + 2,
            K::Delim(Delim::BracedClose {
                mark: char::from(c),
                alone: n == 1,
            }),
        );
        let closed = self.depth_close(char::from(c), true, at);
        let capped =
            !closed && n == 1 && matches!(c, b'*' | b'/') && !self.depth_bare(at, char::from(c));
        // The slot the `}` takes when the mark falls back to the bare rules.
        self.toks.push(Tok::new(at + 1, at + 2, K::Skip));
        if capped {
            if let Some(token) = self.toks.get_mut(tok) {
                token.v1 = at + 1;
                token.make_text(true);
            }
            if let Some(brace) = self.toks.last_mut() {
                brace.make_text(true);
            }
        }
        true
    }

    fn depth_base(&self) -> usize {
        self.brackets.last().map_or(0, |b| b.delim_base)
    }

    fn depth_room(&self) -> bool {
        self.brackets.len() + self.open_delims.len() < tables::INLINE_DEPTH
    }

    fn depth_open(&mut self, mark: char, at: usize) -> bool {
        if self.depth_room() {
            self.open_delims.push(DepthMark {
                mark,
                braced: true,
                at,
            });
            true
        } else {
            false
        }
    }

    fn depth_close(&mut self, mark: char, braced: bool, at: usize) -> bool {
        let base = self.depth_base();
        if let Some(j) = self.open_delims.get(base..).and_then(|marks| {
            marks
                .iter()
                .rposition(|m| m.mark == mark && m.braced == braced)
        }) {
            if self
                .open_delims
                .get(base + j)
                .is_some_and(|opener| opener.at + 2 == at)
            {
                self.open_delims.remove(base + j);
            } else {
                self.open_delims.truncate(base + j);
            }
            true
        } else {
            false
        }
    }

    fn depth_bare(&mut self, at: usize, mark: char) -> bool {
        let before = self.char_before(at);
        let after = self.depth_after(at + 1);
        if before.is_some_and(is_word) && after.is_some_and(is_word) {
            if mark == '*' {
                self.last_star = Some(at);
            } else {
                self.last_slash = Some(at);
            }
            return true;
        }
        let can_close = !is_flank_space(before.unwrap_or(' ')) && after.is_none_or(|a| !is_word(a));
        let can_open = before.is_none_or(|b| !is_word(b)) && !is_flank_space(after.unwrap_or(' '));
        let base = self.depth_base();
        let pair = self
            .open_delims
            .get(base..)
            .and_then(|marks| marks.iter().rposition(|m| m.mark == mark && !m.braced));
        if can_close && pair.is_none() && !self.depth_room() && self.transparent_unformed_bracket()
        {
            return self.depth_bare(at, mark);
        }
        if can_close && let Some(j) = pair {
            let opener = self.open_delims.get(base + j).map_or(at, |entry| entry.at);
            let between = if mark == '*' {
                self.last_star
            } else {
                self.last_slash
            };
            if between.is_some_and(|pos| pos > opener) {
                self.open_delims.remove(base + j);
            } else {
                self.open_delims.truncate(base + j);
            }
            true
        } else if can_open && self.depth_room() {
            self.open_delims.push(DepthMark {
                mark,
                braced: false,
                at,
            });
            true
        } else {
            !can_open
        }
    }

    /// C2-I-026: a comment to the right has not entered `comments` yet.
    fn depth_after(&self, mut i: usize) -> Option<char> {
        loop {
            if let Some(body) = self.t.get(i..)?.strip_prefix("{%") {
                if let Some(end) = body.find("%}") {
                    i += end + 4;
                    continue;
                }
            }
            return super::delim::Ctx {
                t: self.t,
                comments: &self.comments,
            }
            .after(i);
        }
    }

    /// C2-I-006: a simple bracket that forms no construct bounds no delimiter.
    fn transparent_unformed_bracket(&mut self) -> bool {
        let Some(top) = self.brackets.last() else {
            return false;
        };
        let inherited = self
            .brackets
            .iter()
            .rev()
            .nth(1)
            .map_or(0, |b| b.delim_base);
        if inherited >= top.delim_base || !self.simple_unformed_bracket(top.inner) {
            return false;
        }
        let saved = (top.last_star, top.last_slash);
        if let Some(top) = self.brackets.last_mut() {
            top.delim_base = inherited;
        }
        self.last_star = self.last_star.or(saved.0);
        self.last_slash = self.last_slash.or(saved.1);
        true
    }

    /// A conservative lookahead used only at the depth cap.
    fn simple_unformed_bracket(&self, inner: usize) -> bool {
        if self
            .b
            .get(inner..)
            .is_some_and(|tail| !tail.contains(&b']'))
        {
            return true;
        }
        let mut i = inner;
        while let Some(&byte) = self.b.get(i) {
            match byte {
                b'[' | b'\\' | b'`' | b'{' => return false,
                b']' => {
                    if matches!(self.at(i + 1), Some(b'(' | b'[' | b'{')) {
                        return false;
                    }
                    let raw = self.t.get(inner..i).unwrap_or("");
                    if raw.starts_with('@') {
                        return true;
                    }
                    let key = self.label_key(inner, i);
                    return key.is_empty() || !self.env.link_keys.contains(&key);
                }
                _ => i += 1,
            }
        }
        true
    }
}

/// C2-I-074: alt text keeps source spelling except escapes of ASCII punctuation.
fn unescape_alt(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek().is_some_and(char::is_ascii_punctuation) {
            out.push(chars.next().unwrap_or('\\'));
        } else {
            out.push(ch);
        }
    }
    out
}

fn comment_scan_step(source: &str, at: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    match bytes.get(at) {
        Some(b'\\') => (at + 2).min(end),
        Some(b'`') => {
            let code = atoms::code_span(source, at);
            if code.closed && code.end <= end {
                code.end
            } else {
                end
            }
        }
        _ => at + 1,
    }
}

pub(crate) fn find_bracket_comment_close(source: &str, from: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut at = from;
    while at < end {
        match bytes.get(at).copied().unwrap_or_default() {
            b'[' => depth += 1,
            b']' if depth == 0 => return Some(at),
            b']' => depth -= 1,
            _ => {}
        }
        at = comment_scan_step(source, at, end);
    }
    None
}

pub(crate) fn find_braced_comment_close(
    source: &str,
    from: usize,
    end: usize,
    mark: char,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mark = u8::try_from(u32::from(mark)).ok()?;
    let mut depth = 0usize;
    let mut at = from;
    while at < end {
        if bytes.get(at) == Some(&b'{') && bytes.get(at + 1) == Some(&mark) {
            depth += 1;
            at += 2;
        } else if bytes.get(at) == Some(&mark) && bytes.get(at + 1) == Some(&b'}') {
            if depth == 0 {
                return Some(at);
            }
            depth -= 1;
            at += 2;
        } else {
            at = comment_scan_step(source, at, end);
        }
    }
    None
}

/// The `=` of a first part `key=` of the brace block at `i`.
fn key_eq(b: &[u8], i: usize) -> Option<usize> {
    let mut k = i + 1;
    while b.get(k) == Some(&b' ') {
        k += 1;
    }
    let first = *b.get(k)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    k += 1;
    while b
        .get(k)
        .is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        k += 1;
    }
    (b.get(k) == Some(&b'=')).then_some(k)
}

#[derive(Clone, Copy)]
enum CodeKind {
    Plain,
    Literal,
    Math(bool),
}

enum Target {
    Direct {
        href: String,
        tooltip: Option<String>,
    },
    Ref {
        key: String,
        form: LinkRefForm,
    },
}

/// C2-I-047, C2-I-048 helper shared with the delimiter pass.
#[must_use]
pub fn is_space(c: Option<char>) -> bool {
    c.is_none_or(is_flank_space)
}
