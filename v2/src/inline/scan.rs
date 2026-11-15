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
use crate::chars::{fold_label, is_flank_space, is_joining, is_word};
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
    /// An opener past the depth cap (C2-I-126): its closer is text.
    Dead,
}

struct Bracket {
    tok: usize,
    kind: BKind,
    /// Offset behind the opener markup.
    inner: usize,
}

/// The result of the scan.
pub struct Scanned {
    pub toks: Vec<Tok>,
    /// Content ranges of braced comments, in order (C2-I-026).
    pub comments: Vec<(usize, usize)>,
}

struct Scanner<'a, 'e> {
    c: &'a Content<'e>,
    t: &'a str,
    b: &'a [u8],
    env: &'a InlineEnv<'e>,
    toks: Vec<Tok>,
    text_from: Option<usize>,
    brackets: Vec<Bracket>,
    /// How many entries of `brackets` are past the depth cap.
    dead: usize,
    /// No unescaped `]` follows this offset.
    no_label_from: Option<usize>,
    comments: Vec<(usize, usize)>,
    /// Last start of `%}` and of `#}`, to keep the closer search linear.
    last_pct: Option<usize>,
    last_hash: Option<usize>,
    caption_done: bool,
    i: usize,
}

/// Runs the scan over one content.
#[must_use]
pub fn scan(c: &Content<'_>, env: &InlineEnv<'_>) -> Scanned {
    let t = c.text.as_str();
    let mut s = Scanner {
        c,
        t,
        b: t.as_bytes(),
        env,
        toks: Vec::new(),
        text_from: None,
        brackets: Vec::new(),
        dead: 0,
        no_label_from: None,
        comments: Vec::new(),
        last_pct: t.rfind("%}"),
        last_hash: t.rfind("#}"),
        caption_done: env.kind != ContentKind::Caption,
        i: 0,
    };
    while s.i < s.b.len() {
        if !s.step() {
            s.text_char();
        }
    }
    s.flush();
    s.finish_brackets();
    Scanned {
        toks: s.toks,
        comments: s.comments,
    }
}

impl Scanner<'_, '_> {
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

    fn push(&mut self, v0: usize, v1: usize, k: K) -> usize {
        self.flush();
        self.toks.push(Tok::new(v0, v1, k));
        self.i = v1;
        self.toks.len() - 1
    }

    fn leaf(&mut self, v0: usize, v1: usize, node: Node, host: bool) -> usize {
        self.push(
            v0,
            v1,
            K::Leaf {
                node: Box::new(node),
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
            b'@' | b'#' => self.sigil(),
            b':' => self.colon(),
            b'[' => self.open_bracket(),
            b'^' if self.at(i + 1) == Some(b'[') => self.push_bracket(i, i + 2, BKind::Footnote),
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
                self.push(i, i + 2, K::Nbsp);
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
        let span = atoms::code_span(self.t, at);
        if !span.closed && self.env.kind == ContentKind::Cell {
            // C2-B-223: an unclosed code span at the end of a cell is text.
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
                        Node::RawInline(RawInline {
                            format,
                            value,
                            ..Default::default()
                        }),
                        false,
                    );
                    return true;
                }
                Node::Code(Code {
                    value,
                    ..Default::default()
                })
            }
            CodeKind::Literal => Node::LiteralInline(LiteralInline {
                value,
                ..Default::default()
            }),
            CodeKind::Math(display) => Node::Math(Math {
                value,
                display,
                ..Default::default()
            }),
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

    /// C2-I-043: the opening brace, first rule that matches.
    fn brace(&mut self) -> bool {
        let i = self.i;
        if let Some((block, end)) = crate::attr::parse_block(self.t, i) {
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
                self.push(i, i + 2, K::Delim(Delim::BracedOpen(char::from(m))));
                true
            }
            _ => false,
        }
    }

    fn braced_comment(&mut self) -> bool {
        let i = self.i;
        if self.last_pct.is_none_or(|l| l < i + 2) {
            return false;
        }
        let Some(k) = self
            .t
            .get(i + 2..)
            .and_then(|s| s.find("%}"))
            .map(|k| k + i + 2)
        else {
            return false;
        };
        let raw = self.t.get(i + 2..k).unwrap_or("");
        let raw = raw.strip_prefix(' ').unwrap_or(raw);
        let raw = raw.strip_suffix(' ').unwrap_or(raw);
        let node = Node::Comment(Comment {
            form: CommentForm::Braced,
            value: raw.to_string(),
            ..Default::default()
        });
        self.push(
            i,
            k + 2,
            K::Leaf {
                node: Box::new(node),
                host: false,
                transparent: true,
            },
        );
        self.comments.push((i, k + 2));
        true
    }

    fn editorial(&mut self) -> bool {
        let i = self.i;
        if self.last_hash.is_none_or(|l| l < i + 2) {
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
        let value = self.t.get(i + 2..k).unwrap_or("").to_string();
        self.leaf(
            i,
            k + 2,
            Node::CriticComment(CriticComment {
                value,
                ..Default::default()
            }),
            false,
        );
        true
    }

    /// C2-I-023: a line comment takes the spaces before it and the rest of its line.
    fn line_comment(&mut self) -> bool {
        let i = self.i;
        if self.env.kind == ContentKind::LineBlockLine || !atoms::comment_start(self.t, i) {
            return false;
        }
        let end = self
            .t
            .get(i..)
            .and_then(|s| s.find('\n'))
            .map_or(self.t.len(), |e| e + i);
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
        let node = Node::Comment(Comment {
            form: CommentForm::Line,
            value: raw.to_string(),
            ..Default::default()
        });
        self.i = start;
        self.leaf(start, end, node, false);
        true
    }

    fn angle(&mut self) -> bool {
        let i = self.i;
        match atoms::angle(self.t, i) {
            Some(Angle::Xref { key, end }) => {
                let node = Node::Xref(Xref {
                    key,
                    namespace: XrefNamespace::Heading,
                    ..Default::default()
                });
                self.leaf(i, end, node, false);
                true
            }
            Some(Angle::Autolink { href, body, end }) => {
                // GAP INL-001: `text` only where the visible text differs from `href`.
                let text = (body != href).then_some(body);
                self.leaf(
                    i,
                    end,
                    Node::Autolink(Autolink {
                        href,
                        text,
                        ..Default::default()
                    }),
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
        let name = self.t.get(i + 1..end).unwrap_or("").to_string();
        let node = if at {
            Node::Mention(Mention {
                target: name,
                kind: "user".to_string(),
                ..Default::default()
            })
        } else {
            Node::Tag(Tag {
                name,
                ..Default::default()
            })
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
            let name = self.t.get(i + 1..end).unwrap_or("").to_string();
            self.leaf(
                i,
                end + 1,
                Node::Symbol(Symbol {
                    name,
                    ..Default::default()
                }),
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
        if self.at(i + 1) == Some(b'^') {
            return self.footnote_ref(i);
        }
        self.push_bracket(i, i + 1, BKind::Plain)
    }

    /// C2-I-087: `[^label]`; with no label the `[` is text.
    fn footnote_ref(&mut self, i: usize) -> bool {
        if let Label::Ok(e, end) = self.label(i + 2) {
            let key = self.label_key(i + 2, e);
            if !key.is_empty() {
                let node = Node::FootnoteRef(FootnoteRef {
                    key,
                    namespace: "footnote".to_string(),
                    ..Default::default()
                });
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

    /// C2-I-118: an opener past the cap is text, and its closer too (C2-I-126).
    fn push_bracket(&mut self, v0: usize, v1: usize, kind: BKind) -> bool {
        let open = self.brackets.len() - self.dead;
        if self.env.depth + open >= tables::DEPTH_CAP {
            self.dead += 1;
            self.brackets.push(Bracket {
                tok: usize::MAX,
                kind: BKind::Dead,
                inner: v1,
            });
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
        });
        true
    }

    fn close_bracket(&mut self) -> bool {
        let Some(top) = self.brackets.pop() else {
            return false;
        };
        let i = self.i;
        match top.kind.clone() {
            BKind::Dead => {
                self.dead -= 1;
                false
            }
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
        }
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
            t.to_text(true);
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
        let Some((mut block, mut end)) = crate::attr::parse_block(self.t, i + 1) else {
            return false;
        };
        while let Some((next, e)) = crate::attr::parse_block(self.t, end) {
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
        let mut alt = self.t.get(top.inner..i).unwrap_or("").to_string();
        if self.env.kind == ContentKind::Cell {
            alt = alt.replace("\\|", "|");
        }
        let mut img = Image {
            alt: Some(alt),
            ..Default::default()
        };
        match target {
            Target::Direct { href, tooltip } => {
                img.src = Some(href);
                img.tooltip = tooltip;
            }
            Target::Ref { key, form } => {
                img.key = Some(key);
                img.namespace = Some("link".to_string());
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
                    node: Box::new(Node::Image(img)),
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
                t.to_text(true);
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
        self.push(i, i + 1, K::Delim(Delim::Bare(char::from(c))));
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
        self.push(
            at,
            at + 2,
            K::Delim(Delim::BracedClose {
                mark: char::from(c),
                alone: n == 1,
            }),
        );
        // The slot the `}` takes when the mark falls back to the bare rules.
        self.toks.push(Tok::new(at + 1, at + 2, K::Skip));
        true
    }
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
