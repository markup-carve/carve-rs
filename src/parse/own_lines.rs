//! Block ownership, rule A (markup-carve/carve#3885), ported from the spec
//! model's `ownLines()` walk. A line belongs to the deepest open container whose
//! content column it reaches; an opener past that column still opens there, so
//! the walk moves it, and every line its block owns, onto the owner's content
//! column. It only ever removes indentation.
//!
//! PROTOTYPE: rewrites the source text, so positions are not remapped yet.

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
        }
    }
}

enum Form {
    Blank,
    Quote,
    ColonClose(usize),
    Fence(Closer, bool),
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
            Form::Fence(_, i) | Form::Colon(_, i) | Form::Leaf(i) => *i,
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
    let mut rest = &line[indent_cols(line).1..];
    while let Some(n) = quote_marker(rest) {
        rest = &rest[n..];
        rest = &rest[indent_cols(rest).1..];
    }
    rest
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
            let slot = if ch == b'`' { &mut backtick[j] } else { &mut tilde[j] };
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

struct Walk<'a> {
    lines: Vec<&'a str>,
    frames: Vec<Frame>,
    reach: usize,
    runs: Option<Runs>,
}

/// One line being rewritten: `out` is written, `src` remains.
struct Cursor<'s> {
    out: String,
    src: &'s str,
    col: usize,
    rest: &'s str,
}

impl<'s> Cursor<'s> {
    fn measure(&mut self) {
        let (col, len) = indent_cols(self.src);
        self.col = col;
        self.rest = &self.src[len..];
    }

    /// Write the leading run at `target`; ownership only removes indentation.
    fn lead(&mut self, target: usize) {
        let col = target.min(self.col);
        if col == self.col {
            self.out
                .push_str(&self.src[..self.src.len() - self.rest.len()]);
        } else {
            self.out.extend(std::iter::repeat_n(' ', col));
        }
        self.src = self.rest;
    }

    fn take(&mut self, count: usize) {
        self.out.push_str(&self.src[..count]);
        self.src = &self.src[count..];
    }
}

pub(crate) fn own_lines(source: &str) -> String {
    let (_, fm, body) = split_frontmatter(source, false);
    let prefix = if fm.is_some() {
        &source[..source.len() - body.len()]
    } else {
        ""
    };
    let body = if fm.is_some() { body } else { source };
    let mut walk = Walk {
        lines: body.split('\n').collect(),
        frames: vec![Frame::new(Kind::Doc)],
        reach: 0,
        runs: None,
    };
    let mut result = String::with_capacity(source.len());
    result.push_str(prefix);
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
        };
        c.measure();
        walk.line(at, &mut c, &mut after_plus);
        result.push_str(&c.out);
        result.push_str(c.src);
    }
    result
}

impl<'a> Walk<'a> {
    fn top(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("the document frame stays")
    }

    fn line(&mut self, at: usize, c: &mut Cursor<'_>, after_plus: &mut bool) {
        let mut reached = 0;
        let mut plus = None;
        for k in 1..self.frames.len() {
            let f = &self.frames[k];
            if let Some((marker_col, _)) = f.marker {
                if is_cont_marker(c.rest)
                    && c.col == marker_col
                    && (f.kind == Kind::Quote || c.col < f.col)
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
            let closer = inner.closer;
            c.lead(target);
            if !c.rest.is_empty() && closer.is_some_and(|cl| cl.closes(c.rest)) {
                self.frames.pop();
                self.top().last = Last::Block;
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
                next.canon.saturating_sub(1)
            };
            let target = c.col.saturating_sub(owner.cut).max(owner.canon).min(limit);
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
                let target = if owner.para || !closed_column || plus_follower {
                    c.col.saturating_sub(owner.cut).max(owner.canon)
                } else {
                    owner.canon
                };
                owner.para = true;
                owner.last = Last::Para;
                c.lead(target);
            }
            form => {
                let canon = self.top().canon;
                let col = c.col;
                c.lead(canon);
                let owner = self.frames.len() - 1;
                self.apply(owner, col, form, canon, at, c);
            }
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
            }
        }
        match form {
            Form::Fence(closer, _) => {
                let mut f = Frame::new(Kind::Fence);
                f.col = o_col;
                f.canon = o_canon;
                f.cut = col.saturating_sub(canon);
                f.closer = Some(closer);
                self.frames.push(f);
            }
            Form::Colon(run, _) => {
                let mut f = Frame::new(Kind::Colon);
                f.col = o_col;
                f.canon = o_canon;
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
                self.push_marker(Kind::Item, col, canon, width, head, axes, at, c);
            }
            Form::Note(head) => {
                self.push_marker(Kind::Note, col, canon, FOOTNOTE_BODY_COLUMN, head, String::new(), at, c);
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
        let form = self.classify(c.rest, frame, at);
        if matches!(form, Form::Text) {
            let col = c.col;
            c.lead(col);
            let f = &mut self.frames[frame];
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
                for k in (1..=self.reach).rev() {
                    if self.frames[k].kind == Kind::Colon && self.frames[k].run == colons {
                        return Form::ColonClose(k);
                    }
                }
            }
            let line_block = detect_line_block_open(rest).is_some();
            let opener = detect_container_open(rest).is_some()
                || line_block
                || detect_hardbreaks_block_open(rest).is_some()
                || detect_quote_block_open(rest).is_some();
            if opener {
                let interrupts = colon_closer(rest).is_none();
                if owner_para && !interrupts {
                    return Form::Text;
                }
                if line_block {
                    return Form::Fence(Closer::Colon(colons), interrupts);
                }
                return Form::Colon(colons, interrupts);
            }
        }
        if let Some(open) = detect_fence_open(rest) {
            let _ = open;
            let ch = rest.as_bytes()[0];
            let len = rest.bytes().take_while(|&b| b == ch).count();
            let lines = &self.lines;
            let runs = self.runs.get_or_insert_with(|| closer_runs(lines));
            let widest = if ch == b'`' { &runs.backtick } else { &runs.tilde };
            let reachable = widest.get(at + 1).copied().unwrap_or(0) >= len;
            if owner_para && !reachable {
                return Form::Text;
            }
            return Form::Fence(Closer::Fence(ch, len), reachable);
        }
        if let Some(run) = comment_run(rest) {
            let lines = &self.lines;
            let runs = self.runs.get_or_insert_with(|| closer_runs(lines));
            if runs.last_comment.get(&run).is_some_and(|&j| j > at) {
                return Form::Fence(Closer::Comment(run), true);
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
            if owner_para && owner_kind != Kind::Item {
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
