//! C2-W-088: the kind of a reported opener, and every potential opener of that
//! kind in the leaf block that holds it. Adjacent text nodes are read as one
//! run of text, as the parser reads them.

use super::node::{N, children, id, is_break, is_inline_parent, text, ty};
use crate::hash::AddrMap;
use crate::view::Arr;

/// What stands on either side of a run of text.
#[derive(Clone, Copy, PartialEq)]
enum Edge {
    /// A line start or end: a block edge or a break.
    Line,
    /// The markup of another inline node.
    Markup,
}

/// Adjacent text nodes of one block, as one string.
struct Run<'t> {
    text: String,
    /// Each text node and the offset in `text` where it starts.
    nodes: Vec<(N<'t>, usize)>,
    before: Edge,
    after: Edge,
}

impl Run<'_> {
    fn line_start(&self) -> bool {
        self.before == Edge::Line
    }
}

pub struct KindBlocks<'t> {
    /// The runs of each block.
    blocks: Vec<Vec<Run<'t>>>,
    /// The block, run and offset in the run of each text node.
    owners: AddrMap<usize, (usize, usize, usize)>,
}

impl<'t> KindBlocks<'t> {
    /// The text runs of each leaf block of `group`.
    pub fn new(group: Arr<'t>) -> Self {
        let mut blocks = Vec::new();
        let mut owners = AddrMap::default();
        let mut pending: Vec<N<'t>> = group.iter().collect();
        while let Some(node) = pending.pop() {
            if !is_inline_parent(node) {
                pending.extend(children(node).iter());
                continue;
            }
            let mut runs = Vec::new();
            let mut edge = Edge::Line;
            collect(children(node), &mut runs, &mut edge);
            if let Some(last) = runs.last_mut() {
                if edge == Edge::Line {
                    last.after = Edge::Line;
                }
            }
            for (r, run) in runs.iter().enumerate() {
                for &(n, start) in &run.nodes {
                    owners.insert(id(n), (blocks.len(), r, start));
                }
            }
            blocks.push(runs);
        }
        Self { blocks, owners }
    }

    pub fn block_of(&self, node: N<'_>) -> Option<usize> {
        self.owners.get(&id(node)).map(|&(block, _, _)| block)
    }

    /// The kind of the opener escaped at `at` of `node`: `L` and the shape for
    /// a line start, else the escaped character, with the mark of a braced
    /// opener, and `--` for the second hyphen of a run (C2-W-090, C2-W-099).
    pub fn key(&self, node: N<'_>, at: usize, line: bool) -> Option<String> {
        let &(block, r, start) = self.owners.get(&id(node))?;
        let run = self.blocks.get(block)?.get(r)?;
        if line {
            return Some(format!("L{}", line_shape(&run.text)));
        }
        let at = start + at;
        let b = run.text.as_bytes();
        let ch = *b.get(at)?;
        if ch == b'{' {
            if let Some(mark) = run.text.get(at + 1..).and_then(|rest| rest.chars().next()) {
                return Some(format!("{{{mark}"));
            }
        }
        if ch == b'-' && at > 0 && b[at - 1] == b'-' {
            return Some("--".into());
        }
        Some(char::from(ch).to_string())
    }

    /// Each text node of block `block` with an offset of an opener of kind `key`.
    pub fn openers(&self, block: usize, key: &str) -> Vec<(N<'t>, usize)> {
        let mut out = Vec::new();
        for run in self.blocks.get(block).into_iter().flatten() {
            for at in run_offsets(key, run) {
                let i = run.nodes.partition_point(|&(_, start)| start <= at);
                if let Some(&(n, start)) = i.checked_sub(1).and_then(|i| run.nodes.get(i)) {
                    out.push((n, at - start));
                }
            }
        }
        out
    }
}

fn collect<'t>(nodes: Arr<'t>, runs: &mut Vec<Run<'t>>, edge: &mut Edge) {
    for node in nodes.iter() {
        if ty(node) == "text" {
            match runs.last_mut() {
                Some(run) if run.after == Edge::Markup && *edge == Edge::Markup && run.open() => {
                    run.nodes.push((node, run.text.len()));
                    run.text.push_str(text(node));
                }
                _ => runs.push(Run {
                    text: text(node).to_string(),
                    nodes: vec![(node, 0)],
                    before: *edge,
                    after: Edge::Markup,
                }),
            }
            runs.last_mut().map(Run::keep_open);
            *edge = Edge::Markup;
            continue;
        }
        close(runs);
        if is_break(Some(node)) {
            if let Some(run) = runs.last_mut() {
                run.after = Edge::Line;
            }
            *edge = Edge::Line;
        } else {
            *edge = Edge::Markup;
            collect(children(node), runs, edge);
            close(runs);
            *edge = Edge::Markup;
        }
    }
}

impl Run<'_> {
    /// A run stays open to the next text node while nothing stands between them.
    fn open(&self) -> bool {
        self.nodes.last().is_some_and(|_| self.after == Edge::Markup) && !self.text.is_empty()
            || self.nodes.is_empty()
    }

    fn keep_open(&mut self) {}
}

fn close(_runs: &mut [Run<'_>]) {}

/// The shape of a line opener: its first character, or `0` and the delimiter
/// for an ordered marker.
fn line_shape(value: &str) -> String {
    let end = alphanumeric_prefix(value);
    if end > 0 {
        return format!(
            "0{}",
            value
                .get(end..)
                .and_then(|r| r.chars().next())
                .map_or(String::new(), String::from)
        );
    }
    value.chars().next().map_or(String::new(), String::from)
}

fn alphanumeric_prefix(value: &str) -> usize {
    value.bytes().take_while(u8::is_ascii_alphanumeric).count()
}

fn run_offsets(key: &str, run: &Run<'_>) -> Vec<usize> {
    let value = run.text.as_str();
    let b = value.as_bytes();
    if key.starts_with('L') {
        if !run.line_start() || value.is_empty() || format!("L{}", line_shape(value)) != key {
            return Vec::new();
        }
        if key.starts_with("L0") {
            return vec![alphanumeric_prefix(value)];
        }
        return vec![usize::from(value.starts_with("--"))];
    }
    if key == "--" {
        return (1..b.len())
            .filter(|&at| b[at] == b'-' && b[at - 1] == b'-' && (at < 2 || b[at - 2] != b'-'))
            .collect();
    }
    let Some(&ch) = key.as_bytes().first() else {
        return Vec::new();
    };
    let mark = key.get(1..).filter(|m| !m.is_empty());
    let mut offsets = Vec::new();
    for at in 0..b.len() {
        if b[at] != ch {
            continue;
        }
        if let Some(mark) = mark {
            if !value.get(at + 1..).is_some_and(|rest| rest.starts_with(mark)) {
                continue;
            }
        }
        if ch == b'/' || ch == b'*' {
            if !can_open(run, at) {
                continue;
            }
        } else if at > 0 && b[at - 1] == ch {
            continue;
        }
        offsets.push(at);
    }
    offsets
}

/// C2-I-047, C2-I-049: a delimiter whose neighbors let it open, by the
/// characters written around it.
fn can_open(run: &Run<'_>, at: usize) -> bool {
    let value = run.text.as_str();
    let ch = char::from(value.as_bytes()[at]);
    let side = |edge: Edge| (edge == Edge::Markup).then_some('!');
    let before = value
        .get(..at)
        .and_then(|r| r.chars().next_back())
        .or_else(|| side(run.before));
    let after = value
        .get(at + 1..)
        .and_then(|r| r.chars().next())
        .or_else(|| side(run.after));
    if before == Some(ch) || after == Some(ch) {
        return false;
    }
    !before.is_some_and(crate::chars::is_word)
        && after.is_some_and(|c| !matches!(c, ' ' | '\t' | '\n' | '\u{a0}'))
}
