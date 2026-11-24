//! Step one and two of the scan (C2-I-004): atoms and the bracket stack, in
//! one left-to-right pass. Delimiters are only recorded here; they pair in
//! `delim.rs` once every bracket construct is settled (C2-I-007).

use super::atoms::{self, Angle, Label};
use super::content::Content;
use super::token::{Delim, K, Kind, LinkTarget, Sort, Tok};
use super::{ContentKind, Env, Join};
use crate::ast::{
    Autolink, Code, Comment, CommentForm, CriticComment, FootnoteRef, Image, ImageRefForm,
    LinkRefForm, LiteralInline, Math, Mention, Node, RawInline, Symbol, Tag, Xref, XrefNamespace,
};
use crate::chars::{ByteSet, fold_label, is_flank_space, is_joining, is_word};
use crate::tables;
use std::collections::{HashMap, HashSet, VecDeque};

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
    /// Offset of the opener.
    start: usize,
    /// The caret pairs read before this opener, to see one inside it.
    carets: usize,
    /// C2-I-089: a note of `Notes::demote`, in which a caret pair opens nothing.
    demotes: bool,
    /// C2-I-089: the `[` of a `Notes::demote` note read inside a demoting note.
    /// It closes where that note would, so it settles the note in this pass.
    probes: Option<usize>,
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

/// The stacks of one scan, kept for the next content.
#[derive(Default)]
pub struct Bufs {
    brackets: Vec<Bracket>,
    open_delims: Vec<DepthMark>,
    barriers: Vec<usize>,
    pub comments: Vec<(usize, usize)>,
}

impl Bufs {
    /// Whether a buffer holds more than `cap` entries.
    pub(super) fn exceeds(&self, cap: usize) -> bool {
        self.brackets.capacity() > cap
            || self.open_delims.capacity() > cap
            || self.barriers.capacity() > cap
            || self.comments.capacity() > cap
    }
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
    env: &'r Env<'e, 'a>,
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
    /// Last `]`, for the lookahead at the depth cap; searched when first asked.
    last_close: Option<Option<usize>>,
    caption_done: bool,
    /// C2-I-074: the images that formed in the last pass, whose alt keeps a
    /// `%%` as source; `None` in the first pass, which keeps it in every one.
    formed_images: Option<&'r HashSet<usize>>,
    /// The first pass kept a `%%` inside an open image.
    pct_in_image: bool,
    formed_now: Vec<usize>,
    notes: &'r Notes,
    /// Caret pairs read so far that opened.
    carets: usize,
    /// Open brackets with `demotes`.
    demoting_open: usize,
    /// C2-I-089 tie-break: caret pairs before this offset open nothing.
    demote_until: usize,
    pass_notes: PassNotes,
    /// Open brackets with `probes`.
    probes_open: usize,
    own: OwnReads,
    i: usize,
}

/// C2-I-089: the notes the passes settled.
#[derive(Default)]
struct Notes {
    /// Starts of notes that closed with a caret pair inside, and their end.
    demote: HashMap<usize, usize>,
    /// Starts of `^[` that close only when a caret pair inside opens: text,
    /// and caret pairs before the end open nothing.
    tie: HashMap<usize, usize>,
}

/// What one pass read of the notes.
#[derive(Default)]
struct PassNotes {
    /// `(start, end)` of notes that closed with a caret pair inside that opened.
    caret_closed: Vec<(usize, usize)>,
    /// Starts of demoting notes that did not close.
    failed: Vec<usize>,
}

impl Notes {
    /// Takes what a pass read; returns whether the next pass reads otherwise.
    fn narrow(&mut self, pass: PassNotes) -> bool {
        let mut changed = false;
        for (start, end) in pass.caret_closed {
            if !self.demote.contains_key(&start) && !self.tie.contains_key(&start) {
                self.demote.insert(start, end);
                changed = true;
            }
        }
        for start in pass.failed {
            if let Some(end) = self.demote.remove(&start) {
                self.tie.insert(start, end);
                changed = true;
            }
        }
        changed
    }
}

/// C2-I-089 with C2-I-118: the swallowed notes of the outermost demoting
/// note, each read as it would read on its own, opening where the demoting
/// note opened. The cap leaves that reading more room than the swallowed one,
/// so a closer that pops a swallowed `[` may not pop it there. Each note
/// keeps the count of brackets above its `[`; counts that meet the cap merge,
/// so the groups stay sorted (lowest first) and share one shift.
#[derive(Default)]
struct OwnReads {
    on: bool,
    /// Open entries below the demoting note.
    base: usize,
    cap: usize,
    /// `(count - shift, first, last)` of each group; members are linked in `next`.
    groups: VecDeque<(isize, usize, usize)>,
    shift: isize,
    starts: Vec<usize>,
    next: Vec<usize>,
}

impl OwnReads {
    fn start(&mut self, base: usize, cap: usize) {
        self.on = true;
        self.base = base;
        self.cap = cap;
        self.drop_groups();
    }

    fn drop_groups(&mut self) {
        self.groups.clear();
        self.shift = 0;
        self.starts.clear();
        self.next.clear();
    }

    /// The most brackets above a note's `[` before an opener is text.
    fn ceiling(&self) -> isize {
        isize::try_from(self.cap.saturating_sub(self.base + 1)).unwrap_or(0)
    }

    /// Links member `id` behind the last member of a group ending at `last`.
    fn link(&mut self, last: usize, id: usize) {
        if let Some(n) = self.next.get_mut(last) {
            *n = id;
        }
    }

    fn add(&mut self, start: usize) {
        let id = self.starts.len();
        self.starts.push(start);
        self.next.push(usize::MAX);
        let shift = self.shift;
        match self.groups.front().copied() {
            Some((level, _, last)) if level + shift == 0 => {
                self.link(last, id);
                if let Some(g) = self.groups.front_mut() {
                    g.2 = id;
                }
            }
            _ => self.groups.push_front((-shift, id, id)),
        }
    }

    fn open(&mut self) {
        self.shift += 1;
        let top = self.ceiling() - self.shift;
        let Some(last) = self.groups.back_mut() else {
            return;
        };
        if last.0 <= top {
            return;
        }
        last.0 = top;
        let (_, first, end) = *last;
        let n = self.groups.len();
        if let Some(&(level, _, below_last)) = n.checked_sub(2).and_then(|i| self.groups.get(i))
            && level == top
        {
            self.groups.pop_back();
            self.link(below_last, first);
            if let Some(below) = self.groups.back_mut() {
                below.2 = end;
            }
        }
    }

    fn close(&mut self) {
        if self.groups.front().is_some_and(|g| g.0 + self.shift == 0) {
            self.groups.pop_front();
        }
        self.shift -= 1;
    }

    /// The notes whose `[` no closer popped.
    fn unclosed(&self, out: &mut Vec<usize>) {
        for &(_, first, _) in &self.groups {
            let mut id = first;
            while let Some(&start) = self.starts.get(id) {
                out.push(start);
                id = self.next.get(id).copied().unwrap_or(usize::MAX);
            }
        }
    }
}

/// Bounds the passes that narrow the images keeping a `%%` (C2-I-074) and
/// the notes in which a caret pair opens nothing (C2-I-089).
const MAX_PASSES: usize = 8;

/// Runs the scan over one content. A `%%` inside an open image is alt source
/// only when that image forms, so a scan that met one runs again with the
/// images that formed, until that set stops narrowing (C2-I-024, C2-I-074).
/// Caret pairs open in the first pass; a note that closed with one inside
/// reads it as text in the next, and one that then stays open is text with
/// its caret pairs ordinary (C2-I-089).
#[must_use]
pub fn scan<'a>(
    c: &Content<'_, 'a>,
    env: &Env<'_, 'a>,
    toks: Vec<Tok<'a>>,
    bufs: &mut Bufs,
    known: usize,
) -> Scanned<'a> {
    let mut notes = Notes::default();
    let (mut out, mut pct, formed_now, pass) = scan_pass(c, env, toks, bufs, known, None, &notes);
    let mut notes_moved = notes.narrow(pass);
    if !pct && !notes_moved {
        return out;
    }
    let mut formed: Option<HashSet<usize>> = pct.then(|| formed_now.into_iter().collect());
    for _ in 0..MAX_PASSES {
        bufs.comments = std::mem::take(&mut out.comments);
        let (next, p, now, pass) =
            scan_pass(c, env, out.toks, bufs, known, formed.as_ref(), &notes);
        out = next;
        notes_moved = notes.narrow(pass);
        let images_moved = match formed.as_mut() {
            Some(f) => {
                let before = f.len();
                f.retain(|start| now.contains(start));
                f.len() != before
            }
            None if p && !pct => {
                formed = Some(now.into_iter().collect());
                true
            }
            None => false,
        };
        pct |= p;
        if !notes_moved && !images_moved {
            break;
        }
    }
    out
}

fn scan_pass<'a>(
    c: &Content<'_, 'a>,
    env: &Env<'_, 'a>,
    mut toks: Vec<Tok<'a>>,
    bufs: &mut Bufs,
    known: usize,
    formed_images: Option<&HashSet<usize>>,
    notes: &Notes,
) -> (Scanned<'a>, bool, Vec<usize>, PassNotes) {
    let t = c.text.as_str();
    toks.clear();
    let mut brackets = std::mem::take(&mut bufs.brackets);
    brackets.clear();
    let mut open_delims = std::mem::take(&mut bufs.open_delims);
    open_delims.clear();
    let mut comments = std::mem::take(&mut bufs.comments);
    comments.clear();
    let mut barriers = std::mem::take(&mut bufs.barriers);
    comment_only_starts(t, &mut barriers);
    let mut s = Scanner {
        c,
        t,
        b: t.as_bytes(),
        env,
        toks,
        text_from: None,
        brackets,
        open_delims,
        last_star: None,
        last_slash: None,
        no_label_from: None,
        attr_fail: None,
        comments,
        barriers,
        last_pct: None,
        last_hash: None,
        last_close: None,
        caption_done: env.kind != ContentKind::Caption,
        formed_images,
        pct_in_image: false,
        formed_now: Vec::new(),
        notes,
        carets: 0,
        demoting_open: 0,
        demote_until: 0,
        pass_notes: PassNotes::default(),
        probes_open: 0,
        own: OwnReads::default(),
        i: 0,
    };
    // `known` bytes hold no form start: `step` would be false on each of them.
    if known > 0 && known <= s.b.len() {
        s.text_from = Some(0);
        s.i = known;
    }
    while s.i < s.b.len() {
        if !s.step() {
            s.text_char();
            s.skip_plain();
        }
    }
    s.flush();
    s.finish_brackets();
    bufs.brackets = s.brackets;
    bufs.open_delims = s.open_delims;
    bufs.barriers = s.barriers;
    (
        Scanned {
            toks: s.toks,
            comments: s.comments,
        },
        s.pct_in_image,
        s.formed_now,
        s.pass_notes,
    )
}

fn comment_only_starts(text: &str, starts: &mut Vec<usize>) {
    starts.clear();
    if !text.contains("%%") {
        return;
    }
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if crate::block::shapes::comment_only(line.trim_start_matches([' ', '\t'])) {
            starts.push(at);
        }
        at += line.len();
    }
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
            b'^' if self.at(i + 1) == Some(b'[') => self.note_open(i),
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
        let limit = self.atom_limit(i);
        if self
            .last_hash
            .get_or_insert_with(|| t.rfind("#}"))
            .is_none_or(|l| l < i + 2)
        {
            return false;
        }
        let Some(k) = self
            .t
            .get(i + 2..limit)
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

    /// C2-I-023, C2-I-024: a line comment takes the spaces before it and the
    /// rest of its line, every closer on it included; `%%` directly after an
    /// opener is text. An image alt is source, so `%%` in it is text (C2-I-074).
    fn line_comment(&mut self) -> bool {
        let i = self.i;
        if self.env.kind == ContentKind::LineBlockLine
            || !atoms::comment_start(self.t, i)
            || self.in_image_alt()
        {
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
        let node = Node::Comment(self.c.arena().alloc(Comment {
            form: CommentForm::Line,
            value: self.c.keep(raw),
            ..Default::default()
        }));
        self.i = start;
        self.leaf(start, end, node, false);
        true
    }

    /// C2-I-024, C2-I-074: whether an open image keeps a `%%` as alt source.
    fn in_image_alt(&mut self) -> bool {
        let formed = self.formed_images;
        for b in self.brackets.iter().rev() {
            if !matches!(b.kind, BKind::Image { .. }) {
                continue;
            }
            match formed {
                None => {
                    self.pct_in_image = true;
                    return true;
                }
                Some(f) if f.contains(&b.inner) => return true,
                Some(_) => {}
            }
        }
        false
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
        if self.at(i + 1) == Some(b'^') && !self.caret_demoted(i) {
            self.carets += 1;
            return self.footnote_ref(i);
        }
        self.push_bracket(i, i + 1, BKind::Plain)
    }

    /// C2-I-089: a caret pair here opens nothing.
    fn caret_demoted(&self, i: usize) -> bool {
        self.demoting_open > 0 || i < self.demote_until
    }

    /// C2-I-089: `^[`; a tie-break opener is text, both characters.
    fn note_open(&mut self, i: usize) -> bool {
        if self.caret_demoted(i) {
            return false;
        }
        if let Some(&end) = self.notes.tie.get(&i) {
            self.demote_until = self.demote_until.max(end);
            self.flush();
            self.text_from = Some(i);
            self.i = i + 2;
            return true;
        }
        self.carets += 1;
        self.push_bracket(i, i + 2, BKind::Footnote)
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
        let probes = (kind == BKind::Plain && self.demoting_open > 0)
            .then(|| v0.wrapping_sub(1))
            .filter(|&at| self.at(at) == Some(b'^') && self.notes.demote.contains_key(&at));
        if self.own.on {
            self.own.open();
            // Only swallowed notes stand between it and the demoting note,
            // so read on its own it opens where the demoting note did.
            if let Some(at) = probes
                && open == self.own.base + 1 + self.probes_open
                && self.own.ceiling() > 0
            {
                self.own.add(at);
            }
        }
        if open >= self.env.depth_cap {
            self.flush();
            self.text_from = Some(v0);
            self.i = v1;
            return true;
        }
        let tok = self.push(v0, v1, K::Pending);
        let demotes = kind == BKind::Footnote && self.notes.demote.contains_key(&v0);
        if demotes && self.demoting_open == 0 {
            self.own.start(open, self.env.depth_cap);
        }
        self.demoting_open += usize::from(demotes);
        self.probes_open += usize::from(probes.is_some());
        self.brackets.push(Bracket {
            tok,
            kind,
            start: v0,
            carets: self.carets,
            demotes,
            probes,
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
        if self.own.on {
            self.own.close();
        }
        self.demoting_open -= usize::from(top.demotes);
        self.probes_open -= usize::from(top.probes.is_some());
        if top.demotes && self.demoting_open == 0 {
            // Its swallowed notes would read on with caret pairs that open.
            self.own.on = false;
            self.own.drop_groups();
        }
        let consumed = match top.kind.clone() {
            BKind::Footnote => {
                if self.carets > top.carets {
                    self.pass_notes.caret_closed.push((top.start, i + 1));
                }
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
        // Without a definition no label resolves; folding every one made nested brackets quadratic.
        if self.env.link_keys.is_empty() {
            return false;
        }
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
            Target::Direct { href, tooltip } => Kind::Link(Box::new(LinkTarget {
                href: Some(href),
                tooltip,
                key: None,
                form: None,
            })),
            Target::Ref { key, form } => Kind::Link(Box::new(LinkTarget {
                href: None,
                tooltip: None,
                key: Some(key),
                form: Some(form),
            })),
        };
        self.form(top, i, end, kind, None);
    }

    /// An image is one leaf; its alt is the source of the bracket content.
    fn image(&mut self, top: &Bracket, bang: usize, i: usize, end: usize, target: Target) {
        self.flush();
        self.formed_now.push(top.inner);
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
        if self.own.on {
            self.own.unclosed(&mut self.pass_notes.failed);
        }
        for top in self.brackets.drain(..) {
            if top.demotes {
                self.pass_notes.failed.push(top.start);
            }
            if let Some(start) = top.probes {
                self.pass_notes.failed.push(start);
            }
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

    /// An open delimiter takes depth the own readings do not count.
    fn own_delim(&mut self) {
        if !self.own.groups.is_empty() {
            self.own.drop_groups();
        }
    }

    fn depth_base(&self) -> usize {
        self.brackets.last().map_or(0, |b| b.delim_base)
    }

    fn depth_room(&self) -> bool {
        self.brackets.len() + self.open_delims.len() < self.env.depth_cap
    }

    fn depth_open(&mut self, mark: char, at: usize) -> bool {
        if self.depth_room() {
            self.own_delim();
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
            self.own_delim();
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
                cap: self.env.depth_cap,
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
        let (base, inner) = (top.delim_base, top.inner);
        let saved = (top.last_star, top.last_slash);
        if inherited >= base || !self.simple_unformed_bracket(inner) {
            return false;
        }
        if let Some(top) = self.brackets.last_mut() {
            top.delim_base = inherited;
        }
        self.last_star = self.last_star.or(saved.0);
        self.last_slash = self.last_slash.or(saved.1);
        true
    }

    /// A conservative lookahead used only at the depth cap.
    fn simple_unformed_bracket(&mut self, inner: usize) -> bool {
        let b = self.b;
        let last = *self
            .last_close
            .get_or_insert_with(|| b.iter().rposition(|&c| c == b']'));
        if inner <= b.len() && last.is_none_or(|at| at < inner) {
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
