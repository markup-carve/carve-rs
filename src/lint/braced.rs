//! `braced-span-opener-as-text`: a braced opener left as text in a bare strong or emphasis span,
//! reported when a braced closer of its mark later in the same leaf block would pair with it
//! (C2-I-055, C2-I-058, C2-I-059, C2-I-060).

use super::data_attrs::block_extent;
use crate::view::V;

/// C2-I-043: the marks of a braced opener.
const BRACED_MARKS: &[u8] = b"*/_~=+-^,";

pub(super) const LEAVES: [&str; 6] = [
    "paragraph",
    "heading",
    "definition_term",
    "cell_paragraph",
    "line",
    "citation_definition",
];

const DELIMITED_SPANS: [&str; 9] = [
    "emphasis",
    "strong",
    "underline",
    "strike",
    "highlight",
    "superscript",
    "subscript",
    "insert",
    "delete",
];

/// C2-I-007: a delimiter pairs only inside its bracket.
const BRACKETS: [&str; 4] = ["link", "span", "inline_footnote", "inline_extension"];

fn byte(bytes: &[u8], at: usize) -> u8 {
    bytes.get(at).copied().unwrap_or(0)
}

fn mark_index(mark: u8) -> Option<usize> {
    BRACED_MARKS.iter().position(|&m| m == mark)
}

/// Whether `raw` holds a brace followed by a braced mark, the precondition of any finding.
pub(super) fn may_hold_opener(raw: &str) -> bool {
    raw.as_bytes().windows(2).any(|pair| {
        pair.first() == Some(&b'{') && pair.get(1).is_some_and(|m| BRACED_MARKS.contains(m))
    })
}

fn escaped(bytes: &[u8], at: usize) -> bool {
    bytes
        .get(..at)
        .unwrap_or(&[])
        .iter()
        .rev()
        .take_while(|b| **b == b'\\')
        .count()
        % 2
        == 1
}

struct Opener {
    at: usize,
    scope: u32,
    /// The walk step of the text node, for its path.
    step: usize,
}

#[derive(Default)]
struct Leaf {
    openers: Vec<Opener>,
    /// Braced openers of formed spans, which take closers too (C2-I-058).
    formed: Vec<(usize, u32)>,
    /// Where a closer can stand, as start, end and scope: text and span marks, no atom (C2-W-107).
    content: Vec<(usize, usize, u32)>,
}

/// One walked node: its parent's step and its index in the parent's `children`.
struct Step {
    up: Option<usize>,
    index: usize,
}

/// Node, scope, whether its parent is a bare strong or emphasis, and its step.
type Pending<'a> = (V<'a>, u32, bool, usize);

/// The openers of the leaf at `leaf_path` that a braced closer of their content would pair with,
/// as offset and node path.
pub(super) fn openers(source: &str, leaf: V<'_>, leaf_path: &str) -> Vec<(usize, String)> {
    let Some((start, end)) = leaf.offsets() else {
        return Vec::new();
    };
    let bytes = source.as_bytes();
    let mut record = Leaf::default();
    let mut steps: Vec<Step> = Vec::new();
    let mut scopes = 0u32;
    let mut pending: Vec<Pending<'_>> = Vec::new();
    push_children(leaf, (0, false), None, &mut steps, &mut pending);
    while let Some((node, scope, bare_parent, step)) = pending.pop() {
        let kind = node.ty().as_str().unwrap_or("");
        if LEAVES.contains(&kind) {
            continue;
        }
        let Some(range) = node.offsets() else {
            push_children(node, (scope, false), Some(step), &mut steps, &mut pending);
            continue;
        };
        if BRACKETS.contains(&kind) {
            scopes += 1;
            push_children(node, (scopes, false), Some(step), &mut steps, &mut pending);
            continue;
        }
        if kind == "text" && bare_parent {
            text_openers(source, range, scope, step, &mut record);
        }
        let mut bare = false;
        if DELIMITED_SPANS.contains(&kind) {
            bare = matches!(kind, "strong" | "emphasis") && byte(bytes, range.0) != b'{';
            span_marks(bytes, node, range, scope, &mut record);
        } else if !record.openers.is_empty() {
            if kind == "text" {
                text_content(source, range, scope, &mut record);
            } else if kind == "smart_punctuation" {
                record.content.push((range.0, range.1, scope));
            }
        }
        push_children(node, (scope, bare), Some(step), &mut steps, &mut pending);
    }
    if record.openers.is_empty() {
        return Vec::new();
    }
    let paired = paired_openers(bytes, (start, end), scopes, &record);
    record
        .openers
        .iter()
        .zip(paired)
        .filter(|(_, paired)| *paired)
        .map(|(opener, _)| (opener.at, step_path(leaf_path, &steps, opener.step)))
        .collect()
}

fn push_children<'a>(
    node: V<'a>,
    (scope, bare): (u32, bool),
    up: Option<usize>,
    steps: &mut Vec<Step>,
    pending: &mut Vec<Pending<'a>>,
) {
    let Some(children) = node.at("children").as_array() else {
        return;
    };
    let from = pending.len();
    for (index, child) in children.iter().enumerate() {
        steps.push(Step { up, index });
        pending.push((child, scope, bare, steps.len() - 1));
    }
    if let Some(added) = pending.get_mut(from..) {
        added.reverse();
    }
}

fn step_path(leaf_path: &str, steps: &[Step], at: usize) -> String {
    let mut indexes = Vec::new();
    let mut current = Some(at);
    while let Some(step) = current.and_then(|at| steps.get(at)) {
        indexes.push(step.index);
        current = step.up;
    }
    let mut path = leaf_path.to_string();
    for index in indexes.into_iter().rev() {
        path.push_str("/children/");
        path.push_str(&index.to_string());
    }
    path
}

/// The braced openers left as text in a text child of a bare strong or emphasis.
fn text_openers(
    source: &str,
    (start, end): (usize, usize),
    scope: u32,
    step: usize,
    leaf: &mut Leaf,
) {
    let bytes = source.as_bytes();
    let mut at = start;
    while at + 1 < end {
        if byte(bytes, at) != b'{' || escaped(bytes, at) {
            at += 1;
            continue;
        }
        // C2-I-043: a valid attribute block at a brace comes before a braced opener.
        if let Some(after) = block_extent(source, at, end) {
            at = after;
            continue;
        }
        if BRACED_MARKS.contains(&byte(bytes, at + 1)) {
            leaf.openers.push(Opener { at, scope, step });
        }
        at += 1;
    }
}

/// C2-I-043: a valid attribute block left as text is still an atom, so no closer stands in it.
fn text_content(source: &str, (start, end): (usize, usize), scope: u32, leaf: &mut Leaf) {
    let bytes = source.as_bytes();
    let mut from = start;
    let mut at = start;
    while at < end {
        if byte(bytes, at) == b'{'
            && !escaped(bytes, at)
            && let Some(after) = block_extent(source, at, end)
        {
            if at > from {
                leaf.content.push((from, at, scope));
            }
            from = after;
            at = after;
            continue;
        }
        at += 1;
    }
    if end > from {
        leaf.content.push((from, end, scope));
    }
}

fn span_marks(bytes: &[u8], node: V<'_>, range: (usize, usize), scope: u32, leaf: &mut Leaf) {
    let Some(last) = node
        .at("children")
        .as_array()
        .and_then(crate::view::Arr::last)
    else {
        return;
    };
    let width = if byte(bytes, range.0) == b'{' { 2 } else { 1 };
    if width == 2 {
        leaf.formed.push((range.0, scope));
    }
    leaf.content.push((range.0, range.0 + width, scope));
    if let Some((_, last_end)) = last.offsets() {
        leaf.content.push((last_end, last_end + width, scope));
    }
}

/// The open braced openers of one scope: offset or opener index, mark index, and whether it is a
/// text opener.
#[derive(Default)]
struct OpenStack {
    open: Vec<(usize, usize, bool)>,
    counts: [u32; BRACED_MARKS.len()],
}

impl OpenStack {
    /// C2-I-058, C2-I-059: a closer takes the nearest open opener of its mark and removes every
    /// opener above it; the index of the text opener it takes.
    fn close(&mut self, mark: usize) -> Option<usize> {
        if self.counts.get(mark).is_none_or(|count| *count == 0) {
            return None;
        }
        while let Some((opener, top, text)) = self.open.pop() {
            if let Some(count) = self.counts.get_mut(top) {
                *count -= 1;
            }
            if top == mark {
                return text.then_some(opener);
            }
        }
        None
    }

    fn push(&mut self, opener: usize, mark: usize, text: bool) {
        self.open.push((opener, mark, text));
        if let Some(count) = self.counts.get_mut(mark) {
            *count += 1;
        }
    }
}

/// Opens, in source order, every text and formed opener that starts before `end`.
fn open_before(
    end: usize,
    cursor: &mut (usize, usize),
    stacks: &mut [OpenStack],
    bytes: &[u8],
    leaf: &Leaf,
) {
    loop {
        let text = leaf.openers.get(cursor.0);
        let formed = leaf.formed.get(cursor.1);
        let text_at = text.map_or(usize::MAX, |opener| opener.at);
        let formed_at = formed.map_or(usize::MAX, |formed| formed.0);
        if text_at.min(formed_at) >= end {
            return;
        }
        let (at, scope, index, is_text) = if text_at < formed_at {
            cursor.0 += 1;
            (
                text_at,
                text.map_or(0, |opener| opener.scope),
                cursor.0 - 1,
                true,
            )
        } else {
            cursor.1 += 1;
            (
                formed_at,
                formed.map_or(0, |formed| formed.1),
                formed_at,
                false,
            )
        };
        if let (Some(mark), Some(stack)) = (
            mark_index(byte(bytes, at + 1)),
            stacks.get_mut(scope as usize),
        ) {
            stack.push(index, mark, is_text);
        }
    }
}

/// Per text opener, whether a braced closer of its mark in the same content pairs with it.
fn paired_openers(
    bytes: &[u8],
    (start, end): (usize, usize),
    scopes: u32,
    leaf: &Leaf,
) -> Vec<bool> {
    let end = end.min(bytes.len());
    // Each byte of the leaf block holds its scope plus one, or zero outside the content.
    let mut inside = vec![0u32; end.saturating_sub(start) + 1];
    for &(from, to, scope) in &leaf.content {
        let from = from.clamp(start, end) - start;
        let to = to.clamp(start, end) - start;
        if let Some(slots) = inside.get_mut(from..to) {
            slots.fill(scope + 1);
        }
    }
    let scope_at = |at: usize| inside.get(at - start).copied().unwrap_or(0);
    let mut stacks: Vec<OpenStack> = (0..=scopes).map(|_| OpenStack::default()).collect();
    let mut paired = vec![false; leaf.openers.len()];
    let mut cursor = (0, 0);
    for at in start + 1..end {
        if byte(bytes, at) != b'}' {
            continue;
        }
        open_before(at - 1, &mut cursor, &mut stacks, bytes, leaf);
        let scope = scope_at(at);
        let Some(mark) = mark_index(byte(bytes, at - 1)) else {
            continue;
        };
        if scope == 0
            || scope_at(at - 1) != scope
            || escaped_inside(bytes, at - 1, start, &scope_at)
        {
            continue;
        }
        let taken = stacks
            .get_mut((scope - 1) as usize)
            .and_then(|stack| stack.close(mark));
        if let Some(flag) = taken.and_then(|taken| paired.get_mut(taken)) {
            *flag = true;
        }
    }
    paired
}

/// Whether a backslash run of the content, right before `at`, escapes it.
fn escaped_inside(bytes: &[u8], at: usize, start: usize, scope_at: &impl Fn(usize) -> u32) -> bool {
    let mut slashes = 0;
    while at > start + slashes
        && scope_at(at - 1 - slashes) != 0
        && byte(bytes, at - 1 - slashes) == b'\\'
    {
        slashes += 1;
    }
    slashes % 2 == 1
}
