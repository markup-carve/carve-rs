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
    /// Fence character, run length, and the opener's column.
    Fence(u8, usize, usize),
    Colon(usize),
    Comment(usize),
}

impl Closer {
    fn closes(self, rest: &str, col: usize) -> bool {
        match self {
            // A run indented past its opener is code content (CARVE-P2-006).
            Closer::Fence(ch, len, opener) => {
                col <= opener && pure_fence(rest).is_some_and(|(c, n)| c == ch && n >= len)
            }
            Closer::Colon(run) => colon_closer(rest) == Some(run),
            Closer::Comment(run) => comment_run(rest) == Some(run),
        }
    }
}

struct Frame {
    kind: Kind,
    col: usize,
    canon: usize,
    /// The column the block readers measure the frame at: a note's
    /// marker-line content starts at its body column.
    reader: usize,
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
    /// The marker line ends in a lone `+`, which may attach the next line (§17 L3).
    plus: Option<usize>,
}

impl Frame {
    fn new(kind: Kind) -> Self {
        Frame {
            kind,
            col: 0,
            canon: 0,
            reader: 0,
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
            plus: None,
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
    let head = if marker.checked.is_some() {
        // The task box is content: the head stops after the separator run.
        i + spaces
    } else {
        head
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

/// Per fence character, a max segment tree over columns holding the widest
/// pure fence run still ahead at each column. Lines leave it in order.
struct AheadTree {
    size: usize,
    /// Per column: the runs in line order and their suffix maxima.
    columns: std::collections::HashMap<usize, (Vec<usize>, usize)>,
    max: Vec<usize>,
}

struct Ahead {
    next: usize,
    /// Per line: the fence character and column of its pure run.
    at: Vec<Option<(u8, usize)>>,
    backtick: AheadTree,
    tilde: AheadTree,
}

impl AheadTree {
    fn new(found: &[(usize, usize)]) -> Self {
        let mut size = 1;
        for &(col, _) in found {
            while size <= col {
                size *= 2;
            }
        }
        let mut columns: std::collections::HashMap<usize, (Vec<usize>, usize)> =
            std::collections::HashMap::new();
        for &(col, len) in found {
            columns.entry(col).or_default().0.push(len);
        }
        let mut max = vec![0; 2 * size];
        for (&col, (lens, _)) in columns.iter_mut() {
            for k in (0..lens.len().saturating_sub(1)).rev() {
                lens[k] = lens[k].max(lens[k + 1]);
            }
            max[size + col] = lens.first().copied().unwrap_or(0);
        }
        for k in (1..size).rev() {
            max[k] = max[2 * k].max(max[2 * k + 1]);
        }
        AheadTree { size, columns, max }
    }

    fn pop(&mut self, col: usize) {
        let (lens, from) = self.columns.get_mut(&col).expect("indexed column");
        *from += 1;
        let mut k = self.size + col;
        self.max[k] = lens.get(*from).copied().unwrap_or(0);
        k >>= 1;
        while k > 0 {
            self.max[k] = self.max[2 * k].max(self.max[2 * k + 1]);
            k >>= 1;
        }
    }

    fn widest(&self, lo: usize, hi: usize) -> usize {
        let mut widest = 0;
        let (mut l, mut r) = (self.size + lo, self.size + hi.min(self.size - 1) + 1);
        while l < r {
            if l & 1 == 1 {
                widest = widest.max(self.max[l]);
                l += 1;
            }
            if r & 1 == 1 {
                r -= 1;
                widest = widest.max(self.max[r]);
            }
            l >>= 1;
            r >>= 1;
        }
        widest
    }
}

impl Ahead {
    fn new(lines: &[&str]) -> Self {
        let mut found_b = Vec::new();
        let mut found_t = Vec::new();
        let mut at = Vec::with_capacity(lines.len());
        for line in lines {
            let (col, rest) = quoted_measure(line);
            match pure_fence(rest) {
                Some((ch, n)) => {
                    if ch == b'`' {
                        found_b.push((col, n));
                    } else {
                        found_t.push((col, n));
                    }
                    at.push(Some((ch, col)));
                }
                None => at.push(None),
            }
        }
        Ahead {
            next: 0,
            at,
            backtick: AheadTree::new(&found_b),
            tilde: AheadTree::new(&found_t),
        }
    }

    /// Drop every line up to and including `at`.
    fn advance(&mut self, at: usize) {
        while self.next <= at && self.next < self.at.len() {
            if let Some((ch, col)) = self.at[self.next] {
                if ch == b'`' {
                    self.backtick.pop(col);
                } else {
                    self.tilde.pop(col);
                }
            }
            self.next += 1;
        }
    }

    fn widest(&self, ch: u8, lo: usize, hi: usize) -> usize {
        if lo > hi {
            return 0;
        }
        if ch == b'`' {
            self.backtick.widest(lo, hi)
        } else {
            self.tilde.widest(lo, hi)
        }
    }
}

struct Walk<'a> {
    lines: Vec<&'a str>,
    frames: Vec<Frame>,
    reach: usize,
    runs: Option<Runs>,
    ahead: Option<Ahead>,
    /// Fence-shaped lines the walk read as paragraph text, with their owner's
    /// content column.
    text_fences: Vec<(usize, usize)>,
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

    /// Write the leading run at `target`; ownership only removes indentation.
    fn lead(&mut self, target: usize) {
        self.lead_as(target, false);
    }

    /// `spaces` writes a leading tab as the columns it reaches, so a term and
    /// its body read alike in both spellings.
    fn lead_as(&mut self, target: usize, spaces: bool) {
        let col = target.min(self.col);
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

    /// Replace the leading run with `frame`, which the readers take as the
    /// line's container prefix.
    fn frame(&mut self, frame: &str) {
        let authored = &self.src[..self.src.len() - self.rest.len()];
        self.out.push_str(frame);
        let at = self.out.chars().count();
        self.edits
            .push((at, authored.len() as isize - frame.chars().count() as isize));
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
        ahead: None,
        text_fences: Vec::new(),
    };
    let mut out_lines = Vec::with_capacity(walk.lines.len());
    let mut line_edits = Vec::with_capacity(walk.lines.len());
    let mut after_plus = false;
    for at in 0..walk.lines.len() {
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
        c.out.push_str(c.src);
        out_lines.push(c.out);
        line_edits.push(c.edits);
    }
    frame_text_fences(&mut out_lines, &mut line_edits, &walk.text_fences);
    let mut result = String::with_capacity(source.len());
    result.push_str(prefix);
    let mut edits = Vec::new();
    for (at, (line, line_edit)) in out_lines.iter().zip(line_edits).enumerate() {
        if at > 0 {
            result.push('\n');
        }
        result.push_str(line);
        edits.extend(line_edit.into_iter().map(|(at_col, delta)| LineEdit {
            line: first_line + at,
            at: at_col,
            delta,
        }));
    }
    (!edits.is_empty()).then_some(OwnedSource {
        source: result,
        edits,
    })
}

/// `^(`{3,}|~{3,})`: the fence character and run length.
fn fence_run(rest: &str) -> Option<(u8, usize)> {
    let ch = *rest.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let n = rest.bytes().take_while(|&b| b == ch).count();
    (n >= 3).then_some((ch, n))
}

/// The readers re-derive a fence line's reading from the walk's output, where
/// moved lines can supply a closer the source did not have (CARVE-P2-006). A
/// fence-shaped line the walk read as text is framed LAZY at its owner's column
/// when a closer-shaped run lies below it at its column or deeper.
fn frame_text_fences(
    lines: &mut [String],
    edits: &mut [Vec<(usize, isize)>],
    text_fences: &[(usize, usize)],
) {
    if text_fences.is_empty() {
        return;
    }
    let segment = |line: &str| -> (usize, usize, usize) {
        let mut s = line;
        let (mut col, mut len) = indent_cols(s);
        while let Some(n) = quote_marker(&s[len..]) {
            s = &s[len + n..];
            (col, len) = indent_cols(s);
        }
        (line.len() - s.len(), col, len)
    };
    let size = lines.iter().map(|line| segment(line).1).max().unwrap_or(0) + 2;
    // Index `size - 1 - col`, so a prefix holds every column at or past `col`.
    let mut backtick = vec![0usize; size];
    let mut tilde = vec![0usize; size];
    let add = |tree: &mut [usize], col: usize, run: usize| {
        let mut k = size - 1 - col;
        while k < size {
            tree[k] = tree[k].max(run);
            k |= k + 1;
        }
    };
    let widest = |tree: &[usize], col: usize| {
        let mut best = 0;
        let mut k = (size - 1 - col) as isize;
        while k >= 0 {
            best = best.max(tree[k as usize]);
            k = ((k as usize & (k as usize + 1)) as isize) - 1;
        }
        best
    };
    let mut next = text_fences.len();
    for j in (0..lines.len()).rev() {
        if next == 0 {
            break;
        }
        let (prefix_len, col, indent_len) = segment(&lines[j]);
        let rest_at = prefix_len + indent_len;
        if text_fences[next - 1].0 == j {
            let frame_col = text_fences[next - 1].1;
            next -= 1;
            if let Some((ch, n)) = fence_run(&lines[j][rest_at..]) {
                let tree = if ch == b'`' { &backtick } else { &tilde };
                if widest(tree, col) >= n {
                    let line = &lines[j];
                    let prefix_chars = line[..prefix_len].chars().count();
                    let old_rest = line[..rest_at].chars().count();
                    let authored_rest = old_rest as isize
                        + edits[j]
                            .iter()
                            .filter(|e| e.0 <= old_rest)
                            .map(|e| e.1)
                            .sum::<isize>();
                    edits[j].retain(|e| e.0 < prefix_chars);
                    let kept: isize = edits[j].iter().map(|e| e.1).sum();
                    let new_rest = prefix_chars + frame_col + LAZY.chars().count();
                    let delta = authored_rest - new_rest as isize - kept;
                    if delta != 0 {
                        edits[j].push((new_rest, delta));
                    }
                    let mut framed = String::with_capacity(line.len() + frame_col + LAZY.len());
                    framed.push_str(&line[..prefix_len]);
                    framed.extend(std::iter::repeat(' ').take(frame_col));
                    framed.push_str(LAZY);
                    framed.push_str(&line[rest_at..]);
                    lines[j] = framed;
                }
            }
        }
        let (prefix_len, col, indent_len) = segment(&lines[j]);
        if let Some((ch, n)) = pure_fence(&lines[j][prefix_len + indent_len..]) {
            let tree = if ch == b'`' {
                &mut backtick
            } else {
                &mut tilde
            };
            add(tree, col, n);
        }
    }
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
    /// Whether a closer for a `len` run of `ch` lies after line `at`, written
    /// from column `lo` to column `hi`. The walk asks in line order.
    fn closer_ahead(&mut self, ch: u8, len: usize, at: usize, lo: usize, hi: usize) -> bool {
        let lines = &self.lines;
        let ahead = self.ahead.get_or_insert_with(|| Ahead::new(lines));
        ahead.advance(at);
        ahead.widest(ch, lo, hi) >= len
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
                let canon = self.frames[k - 1].canon;
                c.lead(canon);
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
            c.lead(canon);
            *after_plus = true;
            return;
        }
        let plus_follower = *after_plus;
        *after_plus = false;
        let missed = reached < self.frames.len() - 1;
        self.reach = reached;
        let inner = self.frames.last().expect("document frame");
        if inner.kind == Kind::Fence && !missed {
            let target = c.col.saturating_sub(inner.cut).max(inner.canon);
            let (closer, span, line_block) = (inner.closer, inner.span, inner.line_block);
            c.lead(target);
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
            } else if closer.is_some_and(|cl| cl.closes(c.rest, c.col)) {
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
        let col = c.col;
        let form = self.classify(c.rest, reached, at, col);
        let top_para = self.frames.last().expect("frame").para;
        // Past a missed note a marker opens where an item or a description body
        // owns it (§10 I2), or beside a missed list.
        let missed_note = missed
            && self.frames[reached + 1..]
                .iter()
                .any(|f| f.kind == Kind::Note);
        if missed
            && !c.rest.is_empty()
            && top_para
            && !form.interrupts()
            && !matches!(&form, Form::Item(_, _, axes)
                if self.frames.get(reached + 1).is_some_and(|f| f.kind == Kind::Item && &f.axes == axes))
            && !(missed_note
                && matches!(form, Form::Item(..))
                && (matches!(self.frames[reached].kind, Kind::Item | Kind::Dd)
                    || self.frames[reached + 1..]
                        .iter()
                        .any(|f| f.kind == Kind::Item)))
        {
            // Past a missed note the line is framed LAZY in the deepest
            // container, so every reader folds it there (carve#3885).
            if missed_note && self.top().plus != Some(at.wrapping_sub(1)) {
                let mut frame = String::new();
                for k in reached + 1..self.frames.len() {
                    if self.frames[k].kind == Kind::Quote {
                        frame.extend(std::iter::repeat(' ').take(self.frames[k - 1].reader));
                        frame.push_str("> ");
                    }
                }
                let leaf = self.frames.last().expect("frame");
                let col = if leaf.kind == Kind::Quote {
                    0
                } else {
                    leaf.reader
                };
                frame.extend(std::iter::repeat(' ').take(col));
                frame.push_str(LAZY);
                c.frame(&frame);
                return;
            }
            let owner = &self.frames[reached];
            let next = &self.frames[reached + 1];
            let limit = if next.kind == Kind::Quote {
                usize::MAX
            } else {
                next.canon.saturating_sub(1)
            };
            let target = c.col.saturating_sub(owner.cut).max(owner.canon).min(limit);
            let canon = owner.canon;
            if fence_run(c.rest).is_some() {
                self.text_fences.push((at, canon));
            }
            c.lead(target);
            return;
        }
        let closed_column = self.frames[reached + 1..]
            .iter()
            .any(|f| f.kind != Kind::Quote);
        self.frames.truncate(reached + 1);
        if c.rest.is_empty() {
            self.top().para = false;
            let col = c.col;
            c.lead(col);
            return;
        }
        match form {
            Form::ColonClose(k) => {
                let canon = self.frames[k].canon;
                c.lead(canon);
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
                if owner.para && fence_run(c.rest).is_some() {
                    let canon = owner.canon;
                    self.text_fences.push((at, canon));
                }
                let owner = self.top();
                owner.absorbs = (owner.para && owner.absorbs) || absorbs_colon(c.rest);
                owner.para = true;
                if !term {
                    owner.last = Last::Para;
                }
                c.lead_as(target, term);
            }
            form => {
                let canon = self.top().canon;
                let col = c.col;
                c.lead_as(canon, matches!(form, Form::Term | Form::Dd(_)));
                let owner = self.frames.len() - 1;
                let reader = self.frames[owner].reader;
                self.apply(owner, col, form, canon, reader, at, c);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        owner: usize,
        col: usize,
        form: Form,
        canon: usize,
        reader: usize,
        at: usize,
        c: &mut Cursor<'_>,
    ) {
        let (o_col, o_canon) = (self.frames[owner].col, self.frames[owner].canon);
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
                f.reader = reader;
                f.cut = col.saturating_sub(canon);
                f.closer = Some(closer);
                f.line_block = line_block;
                self.frames.push(f);
            }
            Form::Colon(run, _) => {
                let mut f = Frame::new(Kind::Colon);
                f.col = o_col;
                f.canon = o_canon;
                f.reader = reader;
                f.cut = col.saturating_sub(canon);
                f.run = run;
                self.frames.push(f);
            }
            Form::Quote => {
                let mut f = Frame::new(Kind::Quote);
                f.marker = Some((col, canon));
                self.frames.push(f);
                let n = quote_marker(c.src).expect("classified as a quote");
                c.take(n);
                c.measure();
                let at_frame = self.frames.len() - 1;
                self.content(at_frame, 0, 0, at, c);
            }
            Form::Item(width, head, axes) => {
                self.push_marker(Kind::Item, col, canon, reader, width, head, axes, at, c);
            }
            Form::Note(head) => {
                self.push_marker(
                    Kind::Note,
                    col,
                    canon,
                    reader,
                    FOOTNOTE_BODY_COLUMN,
                    head,
                    String::new(),
                    at,
                    c,
                );
            }
            Form::Dd(w) => {
                self.push_marker(Kind::Dd, col, canon, reader, w, w, String::new(), at, c);
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
        reader: usize,
        width: usize,
        head: usize,
        axes: String,
        at: usize,
        c: &mut Cursor<'_>,
    ) {
        let mut f = Frame::new(kind);
        f.col = col + width;
        f.canon = canon + width;
        f.reader = reader + width;
        f.cut = col.saturating_sub(canon);
        f.marker = Some((col, canon));
        f.axes = axes;
        self.frames.push(f);
        let head = head.min(c.src.len());
        c.take(head);
        c.measure();
        let at_frame = self.frames.len() - 1;
        self.content(at_frame, col + head, canon + head, at, c);
    }

    /// The rest of a marker line. Only a quote starts new coordinates.
    fn content(&mut self, frame: usize, start: usize, canon: usize, at: usize, c: &mut Cursor<'_>) {
        if c.rest.is_empty() || self.frames.len() > MAX_NESTING_DEPTH {
            return;
        }
        let opener_col = if self.frames[frame].kind == Kind::Quote {
            c.col
        } else {
            start + c.col
        };
        let form = self.classify(c.rest, frame, at, opener_col);
        if matches!(form, Form::Text) {
            let col = c.col;
            let absorbs = absorbs_colon(c.rest);
            c.lead(col);
            let plus = is_cont_marker(c.rest.trim_end_matches([' ', '\t']));
            let f = &mut self.frames[frame];
            f.absorbs = absorbs;
            f.para = true;
            if plus {
                f.plus = Some(at);
            }
            f.last = Last::Para;
            return;
        }
        if self.frames[frame].kind == Kind::Quote {
            let col = c.col;
            c.lead(0);
            return self.apply(frame, col, form, 0, 0, at, c);
        }
        let col = start + c.col;
        let m = c.col;
        c.lead(m);
        let f = &self.frames[frame];
        let reader = if f.kind == Kind::Note {
            f.reader
        } else {
            f.reader + canon + m - f.canon
        };
        self.apply(frame, col, form, canon + m, reader, at, c);
    }

    fn classify(&mut self, rest: &str, owner: usize, at: usize, col: usize) -> Form {
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
            let reachable = self.closer_ahead(ch, len, at, owner_col, col);
            if owner_para && !reachable {
                return Form::Text;
            }
            return Form::Fence(Closer::Fence(ch, len, col), reachable, false);
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
