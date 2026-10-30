//! Block ownership, rule A (markup-carve/carve#3885), ported from the spec
//! model's `ownLines()` walk. A line belongs to the deepest open container whose
//! content column it reaches; an opener past that column still opens there, so
//! the walk moves it, and every line its block owns, onto the owner's content
//! column. It only ever removes indentation.
//!
//! The walk rewrites the source; [`OwnedSource::authored_column`] maps a
//! position in the rewritten text back to the authored line.

use super::*;

const MAX_NESTING_DEPTH: usize = 200;
const FOOTNOTE_BODY_COLUMN: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Doc,
    Quote,
    Item,
    Note,
    Dd,
    Fence,
    Colon,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Last {
    None,
    Para,
    Block,
    Term,
    Dd,
    Other,
}

#[derive(Clone, Copy)]
enum Closer {
    Fence(u8, usize),
    Colon(usize),
    Comment(usize),
}

impl Closer {
    fn closes(self, rest: &str) -> bool {
        match self {
            Closer::Fence(ch, len) => pure_fence(rest).is_some_and(|(c, n)| c == ch && n >= len),
            Closer::Colon(run) => colon_closer(rest) == Some(run),
            Closer::Comment(run) => comment_run(rest) == Some(run),
        }
    }
}

struct Frame {
    kind: Kind,
    col: usize,
    canon: usize,
    cut: usize,
    para: bool,
    last: Last,
    marker: Option<(usize, usize)>,
    axes: String,
    closer: Option<Closer>,
    run: usize,
    term_cut: usize,
    /// The open paragraph absorbed an invalid colon opener (CARVE-P9-016).
    absorbs: bool,
    line_block: bool,
    /// A fence or comment span open inside a line-block payload.
    span: Option<(u8, usize)>,
    /// Columns added on output, so a block in a task item past its marker-line
    /// child lands where the item reader measures that child.
    shift: usize,
    /// The output shift its marker was written with.
    marker_shift: usize,
    /// 1-based line that opened the frame, for the decision trace.
    line: usize,
    /// A fence's trace family: `code`, `comment`, or `div` for a line block.
    family: &'static str,
}

impl Frame {
    fn new(kind: Kind) -> Self {
        Frame {
            kind,
            col: 0,
            canon: 0,
            cut: 0,
            para: false,
            last: Last::None,
            marker: None,
            axes: String::new(),
            closer: None,
            run: 0,
            term_cut: 0,
            absorbs: false,
            line_block: false,
            span: None,
            shift: 0,
            marker_shift: 0,
            line: 0,
            family: "code",
        }
    }
}

enum Form {
    Blank,
    Quote,
    ColonClose(usize),
    Fence(Closer, bool, bool),
    Colon(usize, bool),
    Leaf(bool),
    Note(usize),
    Item(usize, usize, String),
    Term,
    Dd(usize),
    Text,
}

impl Form {
    fn interrupts(&self) -> bool {
        match self {
            Form::Blank | Form::Quote | Form::ColonClose(_) | Form::Note(_) | Form::Term => true,
            Form::Fence(_, i, _) | Form::Colon(_, i) | Form::Leaf(i) => *i,
            Form::Item(..) | Form::Dd(_) | Form::Text => false,
        }
    }
}

/// Indentation columns (tabs to multiples of 4) and the byte length of the run.
fn indent_cols(s: &str) -> (usize, usize) {
    let mut col = 0;
    let mut len = 0;
    for b in s.bytes() {
        match b {
            b' ' => col += 1,
            b'\t' => col += 4 - col % 4,
            _ => break,
        }
        len += 1;
    }
    (col, len)
}

fn is_ws_only(s: &str) -> bool {
    s.bytes().all(|b| b == b' ' || b == b'\t')
}

/// `^>(?: (.*)|[ \t]*$)`: bytes the marker consumes.
fn quote_marker(rest: &str) -> Option<usize> {
    let tail = rest.strip_prefix('>')?;
    if tail.starts_with(' ') {
        Some(2)
    } else if is_ws_only(tail) {
        Some(rest.len())
    } else {
        None
    }
}

fn pure_fence(rest: &str) -> Option<(u8, usize)> {
    let ch = *rest.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let n = rest.bytes().take_while(|&b| b == ch).count();
    (n >= 3 && is_ws_only(&rest[n..])).then_some((ch, n))
}

fn colon_run(rest: &str) -> usize {
    rest.bytes().take_while(|&b| b == b':').count()
}

fn colon_closer(rest: &str) -> Option<usize> {
    let n = colon_run(rest);
    (n >= 3 && is_ws_only(&rest[n..])).then_some(n)
}

fn comment_run(rest: &str) -> Option<usize> {
    let n = rest.bytes().take_while(|&b| b == b'%').count();
    (n >= 3).then_some(n)
}

fn quoted_rest(line: &str) -> &str {
    quoted_measure(line).1
}

/// The innermost quote level's indentation column and rest.
fn quoted_measure(line: &str) -> (usize, &str) {
    let (mut col, len) = indent_cols(line);
    let mut rest = &line[len..];
    while let Some(n) = quote_marker(rest) {
        rest = &rest[n..];
        let (c, len) = indent_cols(rest);
        col = c;
        rest = &rest[len..];
    }
    (col, rest)
}

fn is_heading(rest: &str) -> bool {
    let n = rest.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&n) && rest[n..].starts_with(' ') && !rest[n..].trim().is_empty()
}

fn is_caption(rest: &str) -> bool {
    rest.strip_prefix('^')
        .is_some_and(|t| t.starts_with(' ') && !t.trim().is_empty())
}

fn is_cont_marker(rest: &str) -> bool {
    rest.strip_prefix('+').is_some_and(is_ws_only)
}

/// The bullet or ordered marker's content column offset, the bytes up to its
/// content, and the sibling axes.
fn list_marker(rest: &str) -> Option<(usize, usize, String)> {
    let marker = detect_list_marker_full(rest)?;
    let bytes = rest.as_bytes();
    let token = bytes
        .iter()
        .take_while(|&&b| b != b' ' && b != b'{' && b != b'\t')
        .count();
    let mut i = token;
    if bytes.get(i) == Some(&b'{') {
        i += bytes[i..].iter().position(|&b| b == b'}')? + 1;
    }
    let spaces = bytes[i..].iter().take_while(|&&b| b == b' ').count();
    let width = token + if marker.checked.is_some() { 1 } else { spaces };
    let head = if marker.content.is_empty() {
        rest.len()
    } else {
        marker.content.as_ptr() as usize - rest.as_ptr() as usize
    };
    let axes = match marker.delim {
        Some(d) => (d as char).to_string(),
        None => marker.marker.to_owned(),
    };
    Some((width, head, axes))
}

struct Runs {
    backtick: Vec<usize>,
    tilde: Vec<usize>,
    last_comment: std::collections::HashMap<usize, usize>,
}

fn closer_runs(lines: &[&str]) -> Runs {
    let mut backtick = vec![0; lines.len() + 1];
    let mut tilde = vec![0; lines.len() + 1];
    let mut last_comment = std::collections::HashMap::new();
    for j in (0..lines.len()).rev() {
        let rest = quoted_rest(lines[j]);
        backtick[j] = backtick[j + 1];
        tilde[j] = tilde[j + 1];
        if let Some((ch, n)) = pure_fence(rest) {
            let slot = if ch == b'`' {
                &mut backtick[j]
            } else {
                &mut tilde[j]
            };
            *slot = (*slot).max(n);
        }
        if let Some(n) = comment_run(rest) {
            last_comment.entry(n).or_insert(j);
        }
    }
    Runs {
        backtick,
        tilde,
        last_comment,
    }
}

/// One block record of the decision trace, format version 1.
pub(crate) struct TraceRecord {
    line: usize,
    step: &'static str,
    owner: String,
    rule: Option<&'static str>,
    form: &'static str,
    cell: Option<String>,
    closed: Option<usize>,
    reached: Option<String>,
    plus: Option<usize>,
}

impl TraceRecord {
    fn new(
        line: usize,
        step: &'static str,
        owner: String,
        rule: Option<&'static str>,
        form: &'static str,
    ) -> Self {
        TraceRecord {
            line,
            step,
            owner,
            rule,
            form,
            cell: None,
            closed: None,
            reached: None,
            plus: None,
        }
    }

    fn cell(mut self, cell: Option<String>) -> Self {
        self.cell = cell;
        self
    }

    /// Field order and spelling follow the spec model's records.
    fn write_json(&self, out: &mut String) {
        use std::fmt::Write as _;
        let rule = self.rule.map_or("null".to_owned(), |r| format!("\"{r}\""));
        let _ = write!(
            out,
            "{{\"line\":{},\"step\":\"{}\",\"owner\":\"{}\",\"rule\":{},\"form\":\"{}\"",
            self.line, self.step, self.owner, rule, self.form
        );
        if let Some(cell) = &self.cell {
            let _ = write!(out, ",\"cell\":\"{cell}\"");
        }
        if let Some(closed) = self.closed {
            let _ = write!(out, ",\"closed\":{closed}");
        }
        if let Some(reached) = &self.reached {
            let _ = write!(out, ",\"reached\":\"{reached}\"");
        }
        if let Some(plus) = self.plus {
            let _ = write!(out, ",\"plus\":{plus}");
        }
        out.push('}');
    }
}

/// The decision trace of `source` as JSON lines: the header, then one block
/// record per source line from the ownership walk.
pub fn decision_trace(source: &str) -> String {
    let source = normalize_source(source);
    let records = walk_traced(source.as_ref(), true, Some(Vec::new()))
        .1
        .unwrap_or_default();
    let mut out = String::from(
        "{\"trace\":\"carve-decision-trace\",\"version\":1,\"engine\":\"carve-rs\"}\n",
    );
    for record in &records {
        record.write_json(&mut out);
        out.push('\n');
    }
    out
}

/// The trace name of what a classified line opens or is.
fn trace_form(form: &Form) -> &'static str {
    match form {
        Form::Blank => "blank",
        Form::Quote => "quote",
        Form::ColonClose(_) => "closer",
        Form::Fence(Closer::Fence(..), ..) => "code",
        Form::Fence(Closer::Comment(_), ..) => "comment",
        Form::Fence(Closer::Colon(_), ..) | Form::Colon(..) => "div",
        Form::Leaf(_) => "leaf",
        Form::Note(_) => "footnote",
        Form::Item(..) => "item",
        Form::Term => "term",
        Form::Dd(_) => "dd",
        Form::Text => "text",
    }
}

struct Walk<'a> {
    lines: Vec<&'a str>,
    frames: Vec<Frame>,
    reach: usize,
    runs: Option<Runs>,
    /// `widest_from` tables per (fence char, column).
    by_column: std::collections::HashMap<(u8, usize), Vec<usize>>,
    /// Decision trace records (carve `docs/decision-trace.md`); `None` on a parse.
    trace: Option<Vec<TraceRecord>>,
    /// 1-based line of `lines[0]`, past any frontmatter.
    first_line: usize,
}

/// One line being rewritten: `out` is written, `src` remains.
struct Cursor<'s> {
    out: String,
    src: &'s str,
    col: usize,
    rest: &'s str,
    /// `(codepoint after the rewritten run, codepoints removed)` per rewrite.
    edits: Vec<(usize, isize)>,
}

impl<'s> Cursor<'s> {
    fn measure(&mut self) {
        let (col, len) = indent_cols(self.src);
        self.col = col;
        self.rest = &self.src[len..];
    }

    /// Write the leading run at `target`; ownership only removes indentation,
    /// apart from a frame's output `shift`.
    fn lead(&mut self, target: usize) {
        self.lead_as(target, false);
    }

    /// `spaces` writes a leading tab as the columns it reaches, so a term and
    /// its body read alike in both spellings.
    fn lead_as(&mut self, target: usize, spaces: bool) {
        self.lead_at(target.min(self.col), spaces);
    }

    /// Like `lead_as`, plus the owner's output shift.
    fn lead_in(&mut self, target: usize, shift: usize, spaces: bool) {
        self.lead_at(target.min(self.col) + shift, spaces);
    }

    fn lead_at(&mut self, col: usize, spaces: bool) {
        let authored = &self.src[..self.src.len() - self.rest.len()];
        if col == self.col && !(spaces && self.out.is_empty() && authored.contains('\t')) {
            self.out.push_str(authored);
        } else {
            self.out.extend(std::iter::repeat(' ').take(col));
            if authored.len() != col || authored.contains('\t') {
                let at = self.out.chars().count();
                self.edits
                    .push((at, authored.len() as isize - col as isize));
            }
        }
        self.src = self.rest;
    }

    fn take(&mut self, count: usize) {
        self.out.push_str(&self.src[..count]);
        self.src = &self.src[count..];
    }
}

/// The source the block readers see after the ownership walk, with the column
/// edits that map a position in it back to the authored source.
#[derive(Clone)]
pub(crate) struct OwnedSource {
    pub(crate) source: String,
    edits: Vec<LineEdit>,
}

#[derive(Clone, Copy)]
struct LineEdit {
    line: usize,
    at: usize,
    delta: isize,
}

/// Run the ownership walk. `None` when it moves no line, which is the common
/// case and lets every fast path keep reading the source as written.
fn walk(source: &str) -> Option<OwnedSource> {
    walk_from(source, true)
}

/// Walk a body a collector cut out, so an opener past the body's column opens
/// there as it does in a document. Returns the walked body and, per 0-based
/// line, the leading columns it removed.
pub(crate) fn walk_body(source: &str) -> Option<(String, Vec<(usize, isize)>)> {
    let owned = walk_from(source, false)?;
    let mut shifts: Vec<(usize, isize)> = Vec::new();
    for edit in &owned.edits {
        match shifts.last_mut() {
            Some((line, delta)) if *line == edit.line - 1 => *delta += edit.delta,
            _ => shifts.push((edit.line - 1, edit.delta)),
        }
    }
    Some((owned.source, shifts))
}

fn walk_from(source: &str, frontmatter: bool) -> Option<OwnedSource> {
    walk_traced(source, frontmatter, None).0
}

fn walk_traced(
    source: &str,
    frontmatter: bool,
    trace: Option<Vec<TraceRecord>>,
) -> (Option<OwnedSource>, Option<Vec<TraceRecord>>) {
    let (_, fm, body) = if frontmatter {
        split_frontmatter(source, false)
    } else {
        (BTreeMap::new(), None, source)
    };
    let prefix = if fm.is_some() {
        &source[..source.len() - body.len()]
    } else {
        ""
    };
    let first_line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let body = if fm.is_some() { body } else { source };
    let mut walk = Walk {
        lines: body.split('\n').collect(),
        frames: vec![Frame::new(Kind::Doc)],
        reach: 0,
        runs: None,
        by_column: std::collections::HashMap::new(),
        trace,
        first_line,
    };
    if let Some(trace) = walk.trace.as_mut() {
        // INPUT: frontmatter lines are read before ownership.
        // A closer on the last line leaves no newline after it.
        let frontmatter_lines =
            first_line - 1 + usize::from(!prefix.is_empty() && !prefix.ends_with('\n'));
        for line in 1..=frontmatter_lines {
            trace.push(TraceRecord::new(
                line,
                "INPUT",
                "doc".into(),
                None,
                "frontmatter",
            ));
        }
    }
    let mut result = String::with_capacity(source.len());
    result.push_str(prefix);
    let mut edits = Vec::new();
    let mut after_plus = false;
    for at in 0..walk.lines.len() {
        if at > 0 {
            result.push('\n');
        }
        let line = walk.lines[at];
        let mut c = Cursor {
            out: String::new(),
            src: line,
            col: 0,
            rest: line,
            edits: Vec::new(),
        };
        c.measure();
        walk.line(at, &mut c, &mut after_plus);
        result.push_str(&c.out);
        result.push_str(c.src);
        edits.extend(c.edits.into_iter().map(|(at_col, delta)| LineEdit {
            line: first_line + at,
            at: at_col,
            delta,
        }));
    }
    let owned = (!edits.is_empty()).then_some(OwnedSource {
        source: result,
        edits,
    });
    (owned, walk.trace)
}

/// Whether the walk moves any line of `source`.
pub(crate) fn walk_moves_lines(source: &str) -> bool {
    let cached = LAST_WALK.with(|cell| match &*cell.borrow() {
        Some((text, owned)) if text == source => Some(owned.is_some()),
        _ => None,
    });
    cached.unwrap_or_else(|| own_lines(source).is_some())
}

thread_local! {
    /// The last document walked. The fast-path gates and the parser ask about
    /// the same source in turn, and the walk should run once per document.
    static LAST_WALK: std::cell::RefCell<Option<(String, Option<OwnedSource>)>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn own_lines(source: &str) -> Option<OwnedSource> {
    if let Some(hit) = LAST_WALK.with(|cell| match &*cell.borrow() {
        Some((text, owned)) if text == source => Some(owned.clone()),
        _ => None,
    }) {
        return hit;
    }
    let owned = walk(source);
    LAST_WALK.with(|cell| *cell.borrow_mut() = Some((source.to_owned(), owned.clone())));
    owned
}

fn colon_opener(rest: &str) -> bool {
    detect_container_open(rest).is_some()
        || detect_line_block_open(rest).is_some()
        || detect_hardbreaks_block_open(rest).is_some()
        || detect_quote_block_open(rest).is_some()
}

/// A colon run that opens nothing: an open paragraph absorbs it.
fn absorbs_colon(rest: &str) -> bool {
    let rest = rest.trim_end_matches([' ', '\t']);
    colon_run(rest) >= 3 && !colon_opener(rest)
}

impl<'a> Walk<'a> {
    /// The owner path of frames `1..=depth`: `doc/item:1/quote:2`.
    fn path(&self, depth: usize) -> String {
        let mut path = String::from("doc");
        for f in &self.frames[1..=depth] {
            let kind = match f.kind {
                Kind::Quote => "quote",
                Kind::Item => "item",
                Kind::Note => "footnote",
                Kind::Dd => "dd",
                Kind::Colon => "div",
                Kind::Fence => f.family,
                Kind::Doc => "doc",
            };
            path.push('/');
            path.push_str(kind);
            path.push(':');
            path.push_str(&f.line.to_string());
        }
        path
    }

    fn deepest(&self) -> String {
        self.path(self.frames.len() - 1)
    }

    fn emit(&mut self, at: usize, record: TraceRecord) {
        // The empty string after a final newline is not a line.
        if at + 1 == self.lines.len() && self.lines[at].is_empty() {
            return;
        }
        if let Some(trace) = self.trace.as_mut() {
            trace.push(record);
        }
    }

    /// The widest closer run of `ch` at or after line `from`, counting only
    /// closers written at `col` or deeper.
    fn widest_from(&mut self, ch: u8, col: usize, from: usize) -> usize {
        let lines = &self.lines;
        let widest = self.by_column.entry((ch, col)).or_insert_with(|| {
            let mut widest = vec![0; lines.len() + 1];
            for j in (0..lines.len()).rev() {
                widest[j] = widest[j + 1];
                let (m_col, rest) = quoted_measure(lines[j]);
                if m_col < col {
                    continue;
                }
                if let Some((c, n)) = pure_fence(rest) {
                    if c == ch {
                        widest[j] = widest[j].max(n);
                    }
                }
            }
            widest
        });
        widest.get(from).copied().unwrap_or(0)
    }

    /// Whether line `next` holds a block at `col` under `depth` quote levels.
    fn attaches_at(&self, next: usize, col: usize, depth: usize) -> bool {
        let Some(line) = self.lines.get(next) else {
            return false;
        };
        let (mut m_col, len) = indent_cols(line);
        let mut rest = &line[len..];
        for _ in 0..depth {
            let Some(n) = quote_marker(rest) else {
                return false;
            };
            rest = &rest[n..];
            let (c, len) = indent_cols(rest);
            m_col = c;
            rest = &rest[len..];
        }
        !rest.trim().is_empty() && m_col == col
    }

    fn top(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("the document frame stays")
    }

    fn line(&mut self, at: usize, c: &mut Cursor<'_>, after_plus: &mut bool) {
        let tracing = self.trace.is_some();
        let line = self.first_line + at;
        let mut reached = 0;
        let mut plus = None;
        for k in 1..self.frames.len() {
            let f = &self.frames[k];
            if let Some((marker_col, _)) = f.marker {
                // A marker body takes only a block at that column (§17 L3, L4).
                if is_cont_marker(c.rest)
                    && c.col == marker_col
                    && (f.kind == Kind::Quote
                        || (c.col < f.col
                            && self.attaches_at(
                                at + 1,
                                marker_col,
                                self.frames[1..k]
                                    .iter()
                                    .filter(|q| q.kind == Kind::Quote)
                                    .count(),
                            )))
                {
                    plus = Some(k);
                    break;
                }
            }
            if f.kind == Kind::Quote {
                let Some(_) = quote_marker(c.rest) else { break };
                let (canon, shift) = (self.frames[k - 1].canon, self.frames[k - 1].shift);
                c.lead_in(canon, shift, false);
                let n = quote_marker(c.src).expect("measured above");
                c.take(n);
                c.measure();
                reached = k;
                continue;
            }
            if !c.rest.is_empty() && c.col < f.col {
                break;
            }
            reached = k;
        }
        if let Some(k) = plus {
            self.frames.truncate(k + 1);
            let f = self.top();
            f.para = false;
            let canon = f.marker.expect("plus frames carry a marker").1;
            let shift = f.marker_shift;
            c.lead_in(canon, shift, false);
            *after_plus = true;
            if tracing {
                // S2, B-plus-start: a lone `+` at an open marker column.
                let record =
                    TraceRecord::new(line, "S2", self.deepest(), Some("CARVE-P9-067"), "plus")
                        .cell(Some("B-plus-start".into()));
                self.emit(at, record);
            }
            return;
        }
        let plus_follower = *after_plus;
        *after_plus = false;
        let missed = reached < self.frames.len() - 1;
        self.reach = reached;
        let inner = self.frames.last().expect("document frame");
        if inner.kind == Kind::Fence && !missed {
            // S1: an open payload tests its closer first (B-<family>-keep, -end).
            let target = c.col.saturating_sub(inner.cut).max(inner.canon);
            let (closer, span, line_block) = (inner.closer, inner.span, inner.line_block);
            let family = inner.family;
            c.lead_in(target, inner.shift, false);
            if tracing {
                let payload = self.deepest();
                let closes = !c.rest.is_empty()
                    && span.is_none()
                    && closer.is_some_and(|cl| cl.closes(c.rest));
                let (form, end) = if closes {
                    ("closer", "end")
                } else {
                    ("payload", "keep")
                };
                let record = TraceRecord::new(line, "S1", payload, Some("CARVE-P0-005"), form)
                    .cell(Some(format!("B-{family}-{end}")));
                self.emit(at, record);
            }
            if c.rest.is_empty() {
                return;
            }
            if let Some((ch, len)) = span {
                let closes = if ch == b'%' {
                    comment_run(c.rest) == Some(len)
                } else {
                    pure_fence(c.rest).is_some_and(|(c2, n)| c2 == ch && n >= len)
                };
                if closes {
                    self.top().span = None;
                }
            } else if closer.is_some_and(|cl| cl.closes(c.rest)) {
                self.frames.pop();
                self.top().last = Last::Block;
            } else if line_block {
                // A span in a line-block payload pairs past the column too (carve-php#3137).
                let rest = c.rest;
                let lines = &self.lines;
                let runs = self.runs.get_or_insert_with(|| closer_runs(lines));
                let fence = detect_fence_open(rest).map(|_| {
                    let ch = rest.as_bytes()[0];
                    (ch, rest.bytes().take_while(|&b| b == ch).count())
                });
                let new_span = match fence {
                    Some((ch, len)) => {
                        let widest = if ch == b'`' {
                            &runs.backtick
                        } else {
                            &runs.tilde
                        };
                        (widest.get(at + 1).copied().unwrap_or(0) >= len).then_some((ch, len))
                    }
                    None => comment_run(rest)
                        .filter(|run| runs.last_comment.get(run).is_some_and(|&j| j > at))
                        .map(|run| (b'%', run)),
                };
                if new_span.is_some() {
                    self.top().span = new_span;
                }
            }
            return;
        }
        if inner.kind == Kind::Fence {
            self.frames.pop();
        }
        let form = self.classify(c.rest, reached, at);
        let top_para = self.frames.last().expect("frame").para;
        if missed
            && !c.rest.is_empty()
            && top_para
            && !form.interrupts()
            && !matches!(&form, Form::Item(_, _, axes)
                if self.frames.get(reached + 1).is_some_and(|f| f.kind == Kind::Item && &f.axes == axes))
        {
            let owner = &self.frames[reached];
            let next = &self.frames[reached + 1];
            let limit = if next.kind == Kind::Quote {
                usize::MAX
            } else {
                (next.canon + next.shift).saturating_sub(1)
            };
            // A lazy line keeps its column: the item reader folds a line below a
            // marker-line child's column as lazy text. At the item's own column
            // it reads as at the child's column.
            let target = c.col.saturating_sub(owner.cut).max(owner.canon).min(limit);
            let shift = if target == owner.canon && c.col >= target {
                owner.shift
            } else {
                0
            };
            c.lead_in(target, shift, false);
            if tracing {
                // S4: a lazy line folds into the deepest open paragraph.
                let mut record =
                    TraceRecord::new(line, "S4", self.deepest(), Some("CARVE-P0-006"), "lazy");
                record.reached = Some(self.path(reached));
                self.emit(at, record);
            }
            return;
        }
        // S5: apply the boundary table to everything else.
        let closed = self.frames.len() - 1 - reached;
        let mut record = tracing.then(|| {
            let name = trace_form(&form);
            let opens = !matches!(form, Form::Blank | Form::Leaf(_) | Form::Text);
            let cell = match form {
                Form::ColonClose(_) => Some("B-div-end".to_owned()),
                _ => opens.then(|| format!("B-{name}-start")),
            };
            let rule = match form {
                Form::Blank => "CARVE-P0-003",
                Form::Leaf(_) => "CARVE-P0-011",
                _ => "CARVE-P0-024",
            };
            let mut record =
                TraceRecord::new(line, "S5", String::new(), Some(rule), name).cell(cell);
            if closed > 0 {
                record.closed = Some(closed);
                record.reached = Some(self.path(reached));
            }
            record.plus = plus_follower.then(|| line - 1);
            record
        });
        let closed_column = self.frames[reached + 1..]
            .iter()
            .any(|f| f.kind != Kind::Quote);
        self.frames.truncate(reached + 1);
        if c.rest.is_empty() {
            self.top().para = false;
            let col = c.col;
            c.lead(col);
            self.emit_applied(at, record.take(), None);
            return;
        }
        match form {
            Form::ColonClose(k) => {
                // B-div-end: the closer's owner is the div it closes.
                self.emit_applied(at, record.take(), Some(k));
                let (canon, shift) = (self.frames[k].canon, self.frames[k].shift);
                c.lead_in(canon, shift, false);
                self.frames.truncate(k);
                let top = self.top();
                top.para = false;
                top.last = Last::Block;
            }
            Form::Text => {
                let owner = self.top();
                // A term's continuation moves with the term, so a later `:` body still follows it.
                let term = owner.para && owner.last == Last::Term;
                let cut = if term { owner.term_cut } else { owner.cut };
                let target = if owner.para || !closed_column || plus_follower {
                    c.col.saturating_sub(cut).max(owner.canon)
                } else {
                    owner.canon
                };
                owner.absorbs = (owner.para && owner.absorbs) || absorbs_colon(c.rest);
                owner.para = true;
                if !term {
                    owner.last = Last::Para;
                }
                // As for a lazy line, only the item's own column moves.
                let shift = if target == owner.canon {
                    owner.shift
                } else {
                    0
                };
                c.lead_in(target, shift, term);
            }
            form => {
                let (canon, shift) = (self.top().canon, self.top().shift);
                let col = c.col;
                c.lead_in(canon, shift, matches!(form, Form::Term | Form::Dd(_)));
                let owner = self.frames.len() - 1;
                self.apply(owner, col, form, canon, at, c);
            }
        }
        self.emit_applied(at, record, None);
    }

    /// Emit an S5 record owned by the frames up to `depth`, or the deepest open one.
    fn emit_applied(&mut self, at: usize, record: Option<TraceRecord>, depth: Option<usize>) {
        if let Some(mut record) = record {
            record.owner = depth.map_or_else(|| self.deepest(), |k| self.path(k));
            self.emit(at, record);
        }
    }

    fn apply(
        &mut self,
        owner: usize,
        col: usize,
        form: Form,
        canon: usize,
        at: usize,
        c: &mut Cursor<'_>,
    ) {
        let (o_col, o_canon, o_shift) = (
            self.frames[owner].col,
            self.frames[owner].canon,
            self.frames[owner].shift,
        );
        {
            let o = &mut self.frames[owner];
            o.para = false;
            o.last = match form {
                Form::Term => Last::Term,
                Form::Dd(_) => Last::Dd,
                _ => Last::Other,
            };
            if matches!(form, Form::Term) {
                o.para = true;
                o.term_cut = col.saturating_sub(canon);
            }
        }
        match form {
            Form::Fence(closer, _, line_block) => {
                let mut f = Frame::new(Kind::Fence);
                f.col = o_col;
                f.canon = o_canon;
                f.cut = col.saturating_sub(canon);
                f.closer = Some(closer);
                f.line_block = line_block;
                f.shift = o_shift;
                f.line = self.first_line + at;
                f.family = match closer {
                    Closer::Fence(..) => "code",
                    Closer::Comment(_) => "comment",
                    Closer::Colon(_) => "div",
                };
                self.frames.push(f);
            }
            Form::Colon(run, _) => {
                let mut f = Frame::new(Kind::Colon);
                f.col = o_col;
                f.canon = o_canon;
                f.cut = col.saturating_sub(canon);
                f.run = run;
                f.shift = o_shift;
                f.line = self.first_line + at;
                self.frames.push(f);
            }
            Form::Quote => {
                let mut f = Frame::new(Kind::Quote);
                f.marker = Some((col, canon));
                f.marker_shift = o_shift;
                f.line = self.first_line + at;
                self.frames.push(f);
                let n = quote_marker(c.src).expect("classified as a quote");
                c.take(n);
                c.measure();
                let at_frame = self.frames.len() - 1;
                self.content(at_frame, 0, 0, at, c);
            }
            Form::Item(width, head, axes) => {
                self.push_marker(Kind::Item, col, canon, width, head, axes, at, c);
            }
            Form::Note(head) => {
                self.push_marker(
                    Kind::Note,
                    col,
                    canon,
                    FOOTNOTE_BODY_COLUMN,
                    head,
                    String::new(),
                    at,
                    c,
                );
            }
            Form::Dd(w) => {
                self.push_marker(Kind::Dd, col, canon, w, w, String::new(), at, c);
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_marker(
        &mut self,
        kind: Kind,
        col: usize,
        canon: usize,
        width: usize,
        head: usize,
        axes: String,
        at: usize,
        c: &mut Cursor<'_>,
    ) {
        let mut f = Frame::new(kind);
        f.col = col + width;
        f.canon = canon + width;
        f.cut = col.saturating_sub(canon);
        f.marker = Some((col, canon));
        f.axes = axes;
        f.shift = self.frames.last().map_or(0, |owner| owner.shift);
        f.marker_shift = f.shift;
        f.line = self.first_line + at;
        self.frames.push(f);
        let head = head.min(c.src.len());
        c.take(head);
        c.measure();
        let at_frame = self.frames.len() - 1;
        self.content(at_frame, col + head, canon + head, at, c);
        // The item reader measures a marker-line sub-list past a task box at
        // its source column, and the item's later blocks against that column.
        if kind == Kind::Item
            && head > width
            && self
                .frames
                .get(at_frame + 1)
                .is_some_and(|child| child.kind == Kind::Item)
        {
            self.frames[at_frame].shift += head - width;
        }
    }

    /// The rest of a marker line. Only a quote starts new coordinates.
    fn content(&mut self, frame: usize, start: usize, canon: usize, at: usize, c: &mut Cursor<'_>) {
        if c.rest.is_empty() || self.frames.len() > MAX_NESTING_DEPTH {
            return;
        }
        let form = self.classify(c.rest, frame, at);
        if matches!(form, Form::Text) {
            let col = c.col;
            let absorbs = absorbs_colon(c.rest);
            c.lead(col);
            let f = &mut self.frames[frame];
            f.absorbs = absorbs;
            f.para = true;
            f.last = Last::Para;
            return;
        }
        if self.frames[frame].kind == Kind::Quote {
            let col = c.col;
            c.lead(0);
            return self.apply(frame, col, form, 0, at, c);
        }
        let col = start + c.col;
        let m = c.col;
        c.lead(m);
        self.apply(frame, col, form, canon + m, at, c);
    }

    fn classify(&mut self, rest: &str, owner: usize, at: usize) -> Form {
        // Line-final whitespace is trimmed before a line is read.
        let rest = rest.trim_end_matches([' ', '\t']);
        if rest.is_empty() {
            return Form::Blank;
        }
        if quote_marker(rest).is_some() {
            return Form::Quote;
        }
        let owner_para = self.frames[owner].para;
        let owner_kind = self.frames[owner].kind;
        let owner_last = self.frames[owner].last;
        let colons = colon_run(rest);
        if colons >= 3 {
            if colon_closer(rest).is_some() {
                // A closer is read in its owner: only a colon body at the reached
                // depth, or holding it directly, can take it.
                for k in (1..=self.reach).rev() {
                    if self.frames[k].kind != Kind::Colon {
                        break;
                    }
                    if self.frames[k].run == colons {
                        return Form::ColonClose(k);
                    }
                }
            }
            let line_block = detect_line_block_open(rest).is_some();
            if colon_opener(rest) {
                // A bare `:::` interrupts unless the paragraph's own lines
                // absorbed an invalid opener (§10 I4, CARVE-P9-016, CARVE-P9-017).
                let absorbed =
                    owner == self.frames.len() - 1 && owner_para && self.frames[owner].absorbs;
                let interrupts = colon_closer(rest).is_none() || !absorbed;
                if owner_para && !interrupts {
                    return Form::Text;
                }
                if line_block {
                    return Form::Fence(Closer::Colon(colons), interrupts, true);
                }
                return Form::Colon(colons, interrupts);
            }
        }
        if let Some(open) = detect_fence_open(rest) {
            let _ = open;
            let ch = rest.as_bytes()[0];
            let len = rest.bytes().take_while(|&b| b == ch).count();
            // A closer below the owner's column never matches (CARVE-P0-014).
            let owner_col = self.frames[owner].col;
            let reachable = true || self.widest_from(ch, owner_col, at + 1) >= len;
            if owner_para && !reachable {
                return Form::Text;
            }
            return Form::Fence(Closer::Fence(ch, len), reachable, false);
        }
        if let Some(run) = comment_run(rest) {
            let lines = &self.lines;
            let runs = self.runs.get_or_insert_with(|| closer_runs(lines));
            if runs.last_comment.get(&run).is_some_and(|&j| j > at) {
                return Form::Fence(Closer::Comment(run), true, false);
            }
        }
        if rest.starts_with("%%") {
            return Form::Leaf(true);
        }
        if rest.starts_with("[^") && parse_footnote_def_line(rest).is_some() {
            let close = rest.find("]: ").expect("footnote separator");
            let spaces = rest[close + 2..].bytes().take_while(|&b| b == b' ').count();
            return Form::Note(close + 2 + spaces);
        }
        if let Some((width, head, axes)) = list_marker(rest) {
            // A term is not a paragraph a marker continues.
            if owner_para && owner_kind != Kind::Item && owner_last != Last::Term {
                return Form::Text;
            }
            return Form::Item(width, head, axes);
        }
        if rest.starts_with(":: ") && !rest[3..].trim().is_empty() {
            return Form::Term;
        }
        if let Some(tail) = rest.strip_prefix(':') {
            let spaces = tail.bytes().take_while(|&b| b == b' ').count();
            if spaces > 0
                && !is_ws_only(&tail[spaces..])
                && matches!(owner_last, Last::Term | Last::Dd)
            {
                return Form::Dd(1 + spaces);
            }
        }
        if is_heading(rest)
            || detect_thematic_break(rest)
            || is_table_start(rest)
            || parse_link_def_line(rest).is_some()
            || detect_abbreviation_def(rest).is_some()
            || (rest.starts_with('{') && standalone_attrs_block_len(&[rest]) == Some(1))
        {
            return Form::Leaf(true);
        }
        if is_caption(rest) {
            return Form::Leaf(false);
        }
        Form::Text
    }
}

thread_local! {
    static ACTIVE_EDITS: std::cell::RefCell<Vec<LineEdit>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Installs the walk's column edits for the offset pass of one parse and
/// restores the enclosing parse's edits on drop.
pub(crate) struct EditsGuard(Vec<LineEdit>);

impl EditsGuard {
    pub(crate) fn install(owned: Option<&OwnedSource>) -> Self {
        let edits = owned.map(|o| o.edits.clone()).unwrap_or_default();
        EditsGuard(ACTIVE_EDITS.with(|cell| std::mem::replace(&mut *cell.borrow_mut(), edits)))
    }
}

impl Drop for EditsGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.0);
        ACTIVE_EDITS.with(|cell| *cell.borrow_mut() = previous);
    }
}

/// Move a span's columns from the walked source back onto the authored lines.
pub(crate) fn restore_authored_columns(pos: &mut Pos) {
    ACTIVE_EDITS.with(|cell| {
        let edits = cell.borrow();
        if edits.is_empty() {
            return;
        }
        pos.start_column = authored_column(&edits, pos.start_line, pos.start_column);
        pos.end_column = authored_column(&edits, pos.end_line, pos.end_column);
    });
}

/// Map a 1-based `column` on 1-based `line` of the walked source to the
/// column the same character has in the authored line.
fn authored_column(edits: &[LineEdit], line: usize, column: usize) -> usize {
    let from = edits.partition_point(|e| e.line < line);
    let mut shift = 0isize;
    for e in edits[from..].iter().take_while(|e| e.line == line) {
        if e.at < column {
            shift += e.delta;
        }
    }
    (column as isize + shift).max(1) as usize
}
