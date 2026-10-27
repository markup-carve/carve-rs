//! Physical-line ownership for standalone continuation markers.

use std::cell::RefCell;
use std::collections::HashMap;

use super::{
    detect_fence_open, detect_list_marker_full, indent_columns, is_blank_line, is_fence_close,
    is_plus_marker, item_block_opener, leading_ws, line_starts_paragraph, note_lint_event,
    strip_blockquote_prefix, strip_description_prefix, trim_ascii_start, FenceOpen, LintEvent,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    List,
    Quote,
    Description,
    Footnote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    WrongColumn,
    Document,
    Blank,
    Left,
    Right,
    Sibling,
    List,
}

pub(super) fn refusal_name(reason: Refusal) -> &'static str {
    match reason {
        Refusal::WrongColumn => "wrong-column",
        Refusal::Document => "document",
        Refusal::Blank => "blank",
        Refusal::Left => "left",
        Refusal::Right => "right",
        Refusal::Sibling => "sibling",
        Refusal::List => "list",
    }
}

fn host(kind: Option<Kind>, quote_depth: usize) -> &'static str {
    match kind {
        Some(Kind::Quote) => "block quote",
        None if quote_depth > 0 => "block quote",
        Some(Kind::Description) => "description body",
        Some(Kind::Footnote) => "footnote",
        _ => "list item",
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Decision {
    pub owner: Option<Kind>,
    pub refusal: Option<Refusal>,
    pub first_block: bool,
    pub quote_depth: usize,
    pub after_quote_blank: bool,
    pub sibling_interrupt: bool,
    /// No container is open: a document-level `+` (carve#3421).
    pub unclaimed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Entry {
    kind: Kind,
    column: usize,
    local_column: usize,
    depth: usize,
    content_column: usize,
    hosted_quote: bool,
}

impl Entry {
    fn new(
        kind: Kind,
        (column, local_column): (usize, usize),
        depth: usize,
        width: usize,
        hosted_quote: bool,
    ) -> Self {
        Self {
            kind,
            column,
            local_column,
            depth,
            content_column: column + width,
            hosted_quote,
        }
    }
}

struct Classifier {
    lines: Vec<String>,
    matching_code_closer: Vec<bool>,
    walk: Walk,
    scanned: usize,
}

/// What the classifier carries from one line to the next, and what it decided.
struct Walk {
    stack: Vec<Entry>,
    held_owner: Option<Entry>,
    pending_blank: bool,
    // The last line left paragraph text open, so a plain line below it is lazy.
    paragraph_open: bool,
    closed_quote_on_blank: bool,
    code_fence: Option<FenceOpen>,
    marker_fence: Option<(FenceOpen, usize)>,
    decisions: Vec<Option<Decision>>,
    reported: Vec<bool>,
}

thread_local! {
    static CLASSIFIER: RefCell<Option<Classifier>> = const { RefCell::new(None) };
    #[cfg(test)]
    pub(super) static LINES_CLASSIFIED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    #[cfg(test)]
    pub(super) static STACK_ENTRIES_EXAMINED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    #[cfg(test)]
    pub(super) static FOLLOWER_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub(super) struct Guard(Option<Classifier>);

impl Guard {
    pub(super) fn install(source: &str) -> Self {
        if !source.contains('+') || !may_have_standalone_plus(source) {
            return Self(CLASSIFIER.with(|slot| slot.replace(None)));
        }
        let lines: Vec<_> = source.lines().map(str::to_owned).collect();
        let mut matching_code_closer = vec![false; lines.len()];
        let mut closers: HashMap<(u8, usize), usize> = HashMap::new();
        for (index, line) in lines.iter().enumerate().rev() {
            let (_, body) = quote_body(line);
            let column = source_column(line, None);
            if let Some(open) = detect_fence_open(body) {
                matching_code_closer[index] = closers
                    .get(&(open.fence_char, column))
                    .is_some_and(|&run| run >= open.fence_len);
            }
            let bytes = body.as_bytes();
            let Some(&fence_char @ (b'`' | b'~')) = bytes.first() else {
                continue;
            };
            let run = bytes.iter().take_while(|&&byte| byte == fence_char).count();
            if run >= 3 && bytes[run..].iter().all(|byte| matches!(byte, b' ' | b'\t')) {
                closers
                    .entry((fence_char, column))
                    .and_modify(|longest| *longest = (*longest).max(run))
                    .or_insert(run);
            }
        }
        let decisions = vec![None; lines.len()];
        let reported = vec![false; lines.len()];
        let classifier = Classifier {
            lines,
            matching_code_closer,
            walk: Walk {
                stack: Vec::new(),
                held_owner: None,
                pending_blank: false,
                paragraph_open: false,
                closed_quote_on_blank: false,
                code_fence: None,
                marker_fence: None,
                decisions,
                reported,
            },
            scanned: 0,
        };
        Self(CLASSIFIER.with(|slot| slot.replace(Some(classifier))))
    }
}

/// A marker's `+` is the last non-whitespace byte of its physical line.
/// Container prefixes leave it after punctuation or whitespace, never a word byte.
fn may_have_standalone_plus(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut line_start = 0;
    while let Some(offset) = memchr::memchr(b'+', &bytes[line_start..]) {
        let plus = line_start + offset;
        let line_end =
            memchr::memchr(b'\n', &bytes[plus..]).map_or(bytes.len(), |offset| plus + offset);
        let mut end = line_end;
        if end > line_start && bytes[end - 1] == b'\r' && line_end < bytes.len() {
            end -= 1;
        }
        while end > line_start && matches!(bytes[end - 1], b' ' | b'\t') {
            end -= 1;
        }
        if end > line_start && bytes[end - 1] == b'+' {
            let at = end - 1;
            if at == line_start || !bytes[at - 1].is_ascii_alphanumeric() {
                return true;
            }
        }
        if line_end == bytes.len() {
            break;
        }
        line_start = line_end + 1;
    }
    false
}

impl Drop for Guard {
    fn drop(&mut self) {
        CLASSIFIER.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

/// Whether `decision` can answer: the document holds a `+` to classify.
pub(super) fn active() -> bool {
    CLASSIFIER.with(|slot| slot.borrow().is_some())
}

pub(super) fn decision(source_line: usize) -> Option<Decision> {
    CLASSIFIER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let classifier = slot.as_mut()?;
        let index = source_line.checked_sub(1)?;
        while classifier.scanned <= index && classifier.scanned < classifier.lines.len() {
            classifier.step();
        }
        let found = classifier.walk.decisions.get(index).copied().flatten();
        if let Some(decision) = found {
            // A document-level `+` no container claims is reported by the paragraph
            // reader, and only before a block (carve#3421).
            if let Some(reason) = decision.refusal.filter(|_| !decision.unclaimed) {
                if !classifier.walk.reported[index] {
                    classifier.walk.reported[index] = true;
                    let line = &classifier.lines[index];
                    note_lint_event(LintEvent::Marker {
                        line: index + 1,
                        column: column_at(line, line.rfind('+').unwrap_or(0)) + 1,
                        host: host(decision.owner, decision.quote_depth),
                        reason: refusal_name(reason),
                        first_block: decision.first_block,
                    });
                }
            }
        }
        found
    })
}

pub(super) fn document_column(source_line: usize) -> Option<usize> {
    CLASSIFIER.with(|slot| {
        let slot = slot.borrow();
        let line = slot.as_ref()?.lines.get(source_line.checked_sub(1)?)?;
        Some(source_column(line, None))
    })
}

pub(super) fn preceding_nested_list_marker(source_line: usize, require_blank: bool) -> bool {
    CLASSIFIER.with(|slot| {
        let slot = slot.borrow();
        let Some(classifier) = slot.as_ref() else {
            return false;
        };
        let mut found_marker = false;
        for line in classifier
            .lines
            .iter()
            .take(source_line.saturating_sub(1))
            .rev()
            .skip_while(|line| is_blank_line(line))
        {
            if is_blank_line(line) {
                return require_blank && found_marker;
            }
            if detect_fence_open(line.trim_start()).is_some() {
                return false;
            }
            if indent_columns(line) > 0
                && detect_list_marker_full(line.trim_start_matches([' ', '\t'])).is_some()
            {
                if !require_blank {
                    return true;
                }
                found_marker = true;
            }
        }
        false
    })
}

fn quote_prefix_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut at = start;
    while bytes.get(at).is_some_and(|b| *b == b' ' || *b == b'\t') {
        at += 1;
    }
    if bytes.get(at) != Some(&b'>') {
        return None;
    }
    at += 1;
    match bytes.get(at) {
        None => {}
        Some(b' ') => at += 1,
        _ => return None,
    }
    Some(at)
}

fn quote_prefix(line: &str, levels: Option<usize>) -> Option<(usize, usize)> {
    let (mut depth, mut at) = (0, 0);
    while levels.map_or(true, |levels| depth < levels) {
        let Some(end) = quote_prefix_end(line, at) else {
            return levels.is_none().then_some((depth, at));
        };
        at = end;
        depth += 1;
    }
    Some((depth, at))
}

fn quote_body(line: &str) -> (usize, &str) {
    let (depth, at) = quote_prefix(line, None).unwrap_or_default();
    (depth, &line[at + leading_ws(&line[at..])..])
}

fn column_at(line: &str, end: usize) -> usize {
    let mut col = 0;
    for ch in line[..end].chars() {
        col += if ch == '\t' { 4 - col % 4 } else { 1 };
    }
    col
}

/// A follower's column after the opener line's `levels` host quote prefixes, or `usize::MAX`
/// when its `>` markers sit elsewhere: an indented `>` is no host prefix (`> a` / `  > b`).
fn stacked_follower_column(follower: &str, line: &str, levels: usize) -> usize {
    let (mut follower_at, mut line_at) = (0, 0);
    for _ in 0..levels {
        let (Some(follower_end), Some(line_end)) = (
            quote_prefix_end(follower, follower_at),
            quote_prefix_end(line, line_at),
        ) else {
            return usize::MAX;
        };
        let marker = |text: &str, from: usize| from + text[from..].find('>').unwrap_or(0);
        if column_at(follower, marker(follower, follower_at))
            != column_at(line, marker(line, line_at))
        {
            return usize::MAX;
        }
        (follower_at, line_at) = (follower_end, line_end);
    }
    source_column(follower, Some(levels))
}

fn source_column(line: &str, levels: Option<usize>) -> usize {
    quote_prefix(line, levels).map_or(usize::MAX, |(_, at)| {
        column_at(line, at + leading_ws(&line[at..]))
    })
}

fn local_column(line: &str, depth: usize) -> (usize, bool) {
    let Some((_, at)) = quote_prefix(line, Some(depth)) else {
        return (usize::MAX, false);
    };
    let rest = &line[at..];
    (
        indent_columns(rest),
        rest[..leading_ws(rest)].contains('\t'),
    )
}

fn tight_quote_prefix(line: &str, depth: usize) -> bool {
    let Some((_, at)) = quote_prefix(line, Some(depth)) else {
        return false;
    };
    line.as_bytes()[..line.len().min(at + 1)]
        .windows(2)
        .any(|pair| pair[0] == b'>' && !matches!(pair[1], b' ' | b'\t'))
}

/// Where a classified line sits: its quote depth and the columns its marker is
/// measured at.
#[derive(Clone, Copy)]
struct Shape {
    index: usize,
    depth: usize,
    column: usize,
    local: usize,
    has_tab: bool,
    marker_column: usize,
    relative_quote_column: bool,
    owner_column: usize,
}

impl Shape {
    fn new(line: &str, index: usize, depth: usize, column: usize) -> Self {
        let (local, has_tab) = local_column(line, depth);
        let marker_column = if depth > 0 && has_tab { local } else { column };
        let relative_quote_column = depth > 0 && (has_tab || tight_quote_prefix(line, depth));
        let owner_column = if relative_quote_column {
            local
        } else {
            marker_column
        };
        Self {
            index,
            depth,
            column,
            local,
            has_tab,
            marker_column,
            relative_quote_column,
            owner_column,
        }
    }
}

/// What the open stack says about a standalone `+` at the line's depth.
#[derive(Clone, Copy, Default)]
struct StackScan {
    deepest_quote: usize,
    list_at_column: bool,
    inner_list_open: bool,
    inner_list_blocks_quote: bool,
}

/// The line after a standalone `+`, read at the marker's depth.
#[derive(Clone, Copy)]
struct Follower<'a> {
    line: Option<&'a str>,
    col: usize,
    body: &'a str,
    depth: Option<usize>,
}

impl<'a> Follower<'a> {
    fn probe(lines: &'a [String], shape: Shape) -> Self {
        let line = lines.get(shape.index + 1).map(String::as_str);
        if line.is_some() {
            #[cfg(test)]
            FOLLOWER_PROBES.with(|c| c.set(c.get() + 1));
        }
        let col = line.map_or(usize::MAX, |line| {
            if shape.relative_quote_column {
                local_column(line, shape.depth).0
            } else {
                source_column(line, Some(shape.depth))
            }
        });
        Self {
            line,
            col,
            body: line.map(|line| quote_body(line).1).unwrap_or(""),
            depth: line.map(|line| quote_body(line).0),
        }
    }
}

/// Who a standalone `+` attaches to, before its follower is checked.
#[derive(Clone, Copy)]
struct MarkerClaim {
    owner: Option<Entry>,
    first_block_quote: bool,
    first_block_attaches: bool,
    sibling: bool,
}

/// The containers one line opens, read left to right.
struct LineOpeners<'a> {
    inner: &'a str,
    entry: Entry,
    first_block_owner: Option<Entry>,
    list_opener_on_line: bool,
    // A task box counts as one.
    openers_on_line: usize,
    inner_quote_marker_column: Option<usize>,
    first_quote_marker_column: Option<usize>,
}

/// The line after a first-block `+` that ends a container opener line.
#[derive(Clone, Copy)]
struct FirstBlockFollower<'a> {
    line: Option<&'a str>,
    body: &'a str,
    column: usize,
    opener_column: usize,
}

/// A non-list block at column 0, which has left any item it sits below.
fn column_zero_block(line: &str) -> bool {
    !line.starts_with([' ', '\t'])
        && item_block_opener(line)
        && detect_list_marker_full(line).is_none()
}

impl Classifier {
    fn step(&mut self) {
        let index = self.scanned;
        self.scanned += 1;
        #[cfg(test)]
        LINES_CLASSIFIED.with(|c| c.set(c.get() + 1));
        let lines = &self.lines;
        let line = &lines[index];
        let (depth, body) = quote_body(line);
        let column = source_column(line, None);
        let walk = &mut self.walk;
        if walk.continues_code_fence(body, column) {
            return;
        }
        if let Some(mut open) = self.matching_code_closer[index]
            .then(|| detect_fence_open(body))
            .flatten()
        {
            open.content_col = column;
            walk.code_fence = Some(open);
            walk.paragraph_open = false;
            return;
        }
        let shape = Shape::new(line, index, depth, column);
        if is_plus_marker(body) {
            if !walk.deeper_quote_in_list(lines, shape) {
                walk.classify_marker(lines, shape);
            }
            return;
        }
        if is_blank_line(body) {
            if depth > 0
                && !walk
                    .stack
                    .iter()
                    .any(|entry| entry.kind == Kind::Quote && entry.depth >= depth)
            {
                walk.close_before_line(shape, body);
                walk.push_quote_prefixes(line, depth);
            }
            walk.blank(depth);
            return;
        }
        walk.content_line(lines, shape, body);
    }
}

impl Walk {
    fn root_list_open(&self) -> bool {
        self.stack
            .iter()
            .any(|entry| entry.kind == Kind::List && entry.depth == 0)
    }

    fn continues_code_fence(&mut self, body: &str, column: usize) -> bool {
        let Some(open) = self.code_fence else {
            return false;
        };
        if is_blank_line(body) || column >= open.content_col {
            if is_blank_line(body) {
                self.pending_blank = true;
            }
            if column == open.content_col && is_fence_close(body, open) {
                self.code_fence = None;
                self.marker_fence = None;
            }
            return true;
        }
        self.code_fence = None;
        if !is_plus_marker(body) {
            self.marker_fence = None;
        }
        false
    }

    /// A deeper quote opened inside a list has no marker owner at this depth.
    fn deeper_quote_in_list(&self, lines: &[String], shape: Shape) -> bool {
        let Shape {
            index,
            depth,
            local,
            ..
        } = shape;
        depth > 0
            && local == 0
            && index > 0
            && quote_body(&lines[index - 1]).0 > 0
            && quote_body(&lines[index - 1]).0 < depth
            && self.root_list_open()
    }

    fn scan_stack(&self, shape: Shape) -> StackScan {
        let Shape {
            depth,
            has_tab,
            marker_column,
            ..
        } = shape;
        let mut scan = StackScan::default();
        for entry in &self.stack {
            #[cfg(test)]
            STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
            if entry.kind == Kind::Quote {
                scan.deepest_quote = scan.deepest_quote.max(entry.depth);
            }
            if depth > 0 && entry.kind == Kind::List && entry.depth == depth {
                scan.inner_list_open = true;
                scan.inner_list_blocks_quote |= entry.column >= marker_column;
            }
            if entry.kind == Kind::List
                && entry.depth == depth
                && (if depth > 0 && has_tab {
                    entry.local_column
                } else {
                    entry.column
                }) == marker_column
            {
                scan.list_at_column = true;
            }
        }
        scan
    }

    fn find_owner(&self, shape: Shape, scan: StackScan) -> Option<(usize, Entry)> {
        let Shape {
            depth,
            relative_quote_column,
            owner_column,
            ..
        } = shape;
        for (at, entry) in self.stack.iter().enumerate().rev() {
            #[cfg(test)]
            STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
            let entry_column = if relative_quote_column {
                entry.local_column
            } else {
                entry.column
            };
            if entry_column == owner_column
                && entry.depth == depth
                && (depth == 0
                    || depth >= scan.deepest_quote
                    || matches!(entry.kind, Kind::List | Kind::Quote))
                && !(scan.list_at_column && entry.kind == Kind::Quote)
            {
                return Some((at, *entry));
            }
        }
        None
    }

    /// Whether an unowned `+` on a quote line is that quote's own first-block
    /// marker.
    fn first_block_quote(&self, lines: &[String], shape: Shape, scan: StackScan) -> bool {
        let Shape {
            index,
            depth,
            local,
            marker_column,
            ..
        } = shape;
        depth > 0
            && local == 0
            && !scan.inner_list_open
            && lines.get(index + 1).map_or(true, |line| {
                let (next_depth, next_body) = quote_body(line);
                next_depth < depth
                    || is_blank_line(next_body)
                    || next_depth == depth
                        && self.root_list_open()
                        && index > 0
                        && quote_body(&lines[index - 1]).0 >= depth
            })
            && !index.checked_sub(1).is_some_and(|prior| {
                quote_body(&lines[prior]).0 > 0
                    && source_column(&lines[prior], None) > marker_column
            })
            && !index.checked_sub(1).is_some_and(|prior| {
                quote_body(&lines[prior]).0 >= depth
                    && is_blank_line(quote_body(&lines[prior]).1)
                    && !self.root_list_open()
            })
            && !index.checked_sub(1).is_some_and(|prior| {
                is_blank_line(&lines[prior])
                    && lines[..prior]
                        .iter()
                        .rev()
                        .find(|line| !is_blank_line(line))
                        .is_some_and(|line| {
                            let (prior_depth, prior_body) = quote_body(line);
                            prior_depth > 0 && detect_list_marker_full(prior_body).is_some()
                        })
            })
    }

    fn first_block_attaches(&self, lines: &[String], shape: Shape, follower: Follower<'_>) -> bool {
        let Shape { index, depth, .. } = shape;
        (index == 0 || quote_body(&lines[index - 1]).0 < depth)
            && follower.line.is_some_and(|line| {
                let column = source_column(line, Some(0));
                // CARVE-P9-067: inside an item the follower may also sit at the quote line's column.
                // A block at column 0 below an indented quote line has left the item; only
                // paragraph text there is the quote's (carve#3421).
                (column == 0
                    && (source_column(&lines[index], Some(0)) == 0 || !item_block_opener(line))
                    || quote_body(line).0 == 0 && column == source_column(&lines[index], Some(0))
                    // The quote's own first marker, in line with it (`  > > > +` /
                    // `  > q`), carries the follower into the innermost quote.
                    || column > 0
                        && quote_body(line).0 > 0
                        && column == source_column(&lines[index], Some(0)))
                    && detect_list_marker_full(follower.body).is_none()
            })
            || follower.depth == Some(depth)
                && self.root_list_open()
                && index > 0
                && quote_body(&lines[index - 1]).0 >= depth
    }

    fn classify_marker(&mut self, lines: &[String], shape: Shape) {
        let Shape {
            index,
            depth,
            column,
            local,
            marker_column,
            owner_column,
            ..
        } = shape;
        let scan = self.scan_stack(shape);
        let found = self.find_owner(shape, scan);
        let mut owner = found.map(|(_, entry)| entry);
        let has_ancestor_list = found.is_some_and(|(at, owner)| {
            self.stack[..at].iter().any(|entry| {
                #[cfg(test)]
                STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
                matches!(entry.kind, Kind::List | Kind::Description) && entry.depth <= owner.depth
            })
        });
        let first_block_quote = owner.is_none() && self.first_block_quote(lines, shape, scan);
        if first_block_quote {
            owner = Some(Entry::new(
                Kind::Quote,
                (marker_column, local),
                depth,
                0,
                false,
            ));
        }
        let follower = Follower::probe(lines, shape);
        let sibling = owner.is_some_and(|entry| {
            entry.kind == Kind::List
                || entry.kind == Kind::Quote
                    && (has_ancestor_list || depth > 0 && scan.deepest_quote == depth)
        }) && follower.depth == Some(depth)
            && follower.col == owner_column
            && detect_list_marker_full(follower.body).is_some();
        let claim = MarkerClaim {
            owner,
            first_block_quote,
            first_block_attaches: first_block_quote
                && self.first_block_attaches(lines, shape, follower),
            sibling,
        };
        let refusal = self.marker_refusal(lines, shape, scan, claim, follower);
        if self
            .marker_fence
            .is_some_and(|(fence, _)| marker_column < fence.content_col)
        {
            self.marker_fence = None;
        }
        self.pending_blank = false;
        // A block at column 0 below an indented quote line has left its item; carve-js
        // reports nothing for the `+` it leaves as text.
        if first_block_quote
            && refusal.is_some()
            && source_column(&lines[index], Some(0)) > 0
            && follower.line.is_some_and(column_zero_block)
        {
            self.reported[index] = true;
        }
        self.decisions[index] = Some(Decision {
            owner: owner.map(|e| e.kind),
            refusal,
            first_block: first_block_quote,
            quote_depth: depth,
            after_quote_blank: self.closed_quote_on_blank,
            sibling_interrupt: column == 0
                || depth > 0 && local == 0 && index > 0 && quote_body(&lines[index - 1]).0 >= depth,
            unclaimed: owner.is_none() && depth == 0 && self.stack.is_empty(),
        });
        if refusal.is_none() {
            self.held_owner = owner;
        }
        if depth == 0 {
            self.closed_quote_on_blank = false;
        }
    }

    fn marker_refusal(
        &self,
        lines: &[String],
        shape: Shape,
        scan: StackScan,
        claim: MarkerClaim,
        follower: Follower<'_>,
    ) -> Option<Refusal> {
        let Shape {
            index,
            depth,
            owner_column,
            ..
        } = shape;
        let owner = claim.owner;
        if owner.is_none()
            || scan.inner_list_open
                && owner.is_some_and(|entry| {
                    entry.kind == Kind::Quote
                        && (scan.inner_list_blocks_quote || !entry.hosted_quote)
                })
            || self.pending_blank && owner.is_some_and(|entry| entry.kind == Kind::Footnote)
        {
            return Some(Refusal::WrongColumn);
        }
        if follower.line.is_none() {
            return Some(Refusal::Document);
        }
        if is_blank_line(follower.body)
            && !(follower.depth.is_some_and(|next_depth| next_depth > depth)
                && follower
                    .line
                    .is_some_and(|line| strip_blockquote_prefix(line).is_some()))
        {
            return Some(Refusal::Blank);
        }
        if claim.first_block_quote && !claim.first_block_attaches {
            // CARVE-P9-067: the quote's first-block form refuses a column-0 list and any other column.
            let follower_column = follower.line.map_or(0, |line| source_column(line, Some(0)));
            if follower_column == 0
                && follower
                    .line
                    .is_some_and(|line| detect_list_marker_full(line).is_some())
            {
                return Some(Refusal::List);
            }
            if follower_column > source_column(&lines[index], Some(0)) {
                return Some(Refusal::Right);
            }
            return Some(Refusal::Left);
        }
        if !claim.first_block_attaches
            && (follower.depth.is_some_and(|next_depth| next_depth < depth)
                || follower.col < owner_column)
        {
            return Some(Refusal::Left);
        }
        if !claim.first_block_attaches && follower.col > owner_column {
            return Some(Refusal::Right);
        }
        if claim.sibling && self.held_owner != owner {
            return Some(Refusal::Sibling);
        }
        if depth == 0
            && scan.deepest_quote > 0
            && owner.is_some_and(|entry| entry.kind == Kind::Description)
            && follower.col == owner_column
            && detect_list_marker_full(follower.body).is_some()
        {
            return Some(Refusal::WrongColumn);
        }
        None
    }

    fn blank(&mut self, depth: usize) {
        if depth == 0 {
            if let Some(quote) = self
                .stack
                .iter()
                .position(|entry| entry.kind == Kind::Quote)
            {
                self.closed_quote_on_blank = true;
                self.stack.truncate(quote);
                self.held_owner = None;
            }
        }
        self.pending_blank = true;
        self.paragraph_open = false;
    }

    fn content_line(&mut self, lines: &[String], shape: Shape, body: &str) {
        let Shape { depth, column, .. } = shape;
        if self.marker_fence.is_some_and(|(fence, _)| {
            column < fence.content_col
                || (column == fence.content_col && is_fence_close(body, fence))
        }) {
            self.marker_fence = None;
        }
        let text = trim_ascii_start(body);
        // A lazy paragraph line closes no container (§17 L3 keeps the item's `+` column).
        if self.paragraph_open
            && !self.pending_blank
            && (depth == 0
                || self
                    .stack
                    .iter()
                    .any(|entry| entry.kind == Kind::Quote && entry.depth >= depth))
            && line_starts_paragraph(text)
            && !item_block_opener(text)
            && detect_list_marker_full(text).is_none()
        {
            return;
        }
        self.paragraph_open = false;
        self.close_before_line(shape, body);
        self.push_quote_prefixes(&lines[shape.index], depth);
        let opened = self.open_containers(shape, body);
        self.note_opened_fence(&opened);
        if let Some(owner) = opened.first_block_owner {
            self.classify_first_block(lines, shape, owner, &opened);
        }
    }

    /// Pops what a non-lazy line at this depth and column has closed.
    fn close_before_line(&mut self, shape: Shape, body: &str) {
        let Shape { depth, column, .. } = shape;
        if self.pending_blank {
            self.pending_blank = false;
            if let Some(at) = self.stack.iter().position(|entry| {
                #[cfg(test)]
                STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
                entry.kind == Kind::List
                    && entry.depth == depth
                    && entry.column < column
                    && column < entry.content_column
            }) {
                self.stack.truncate(at);
            }
        }
        if depth == 0 {
            self.closed_quote_on_blank = false;
        }
        if self.held_owner.is_some_and(|entry| {
            depth < entry.depth
                || depth == entry.depth
                    && (column < entry.column
                        || column == entry.column && detect_list_marker_full(body).is_some())
        }) {
            self.held_owner = None;
        }
        self.stack
            .retain(|entry| entry.kind != Kind::Quote || Some(*entry) == self.held_owner);
        while self.stack.last().is_some_and(|entry| {
            Some(*entry) != self.held_owner
                && (entry.depth > depth || entry.depth == depth && entry.column >= column)
        }) {
            self.stack.pop();
        }
    }

    fn push_quote_prefixes(&mut self, line: &str, depth: usize) {
        let mut at = 0;
        for level in 1..=depth {
            let end = quote_prefix_end(line, at).unwrap_or(at);
            let marker_byte = line.as_bytes()[at..end]
                .iter()
                .position(|byte| *byte == b'>')
                .map_or(at, |offset| at + offset);
            let marker_column = column_at(line, marker_byte);
            at = end;
            let quote_column = column_at(line, at);
            self.stack.push(Entry::new(
                Kind::Quote,
                (marker_column, 0),
                level - 1,
                0,
                false,
            ));
            self.stack
                .push(Entry::new(Kind::Quote, (quote_column, 0), level, 0, false));
        }
    }

    fn open_containers<'a>(&mut self, shape: Shape, body: &'a str) -> LineOpeners<'a> {
        let Shape {
            depth,
            column,
            local,
            ..
        } = shape;
        let mut opened = LineOpeners {
            inner: body,
            entry: Entry::new(Kind::List, (column, local), depth, 0, false),
            first_block_owner: None,
            list_opener_on_line: false,
            openers_on_line: 0,
            inner_quote_marker_column: None,
            first_quote_marker_column: None,
        };
        let mut description_on_line = false;
        let entry = &mut opened.entry;
        for _ in 0..32 {
            let inner = opened.inner;
            if let Some(marker) = detect_list_marker_full(inner) {
                opened.list_opener_on_line = true;
                opened.openers_on_line += 1 + usize::from(marker.checked.is_some());
                let width = (marker.content.as_ptr() as usize) - (inner.as_ptr() as usize);
                entry.kind = Kind::List;
                entry.content_column = entry.column + width;
                entry.hosted_quote = false;
                self.stack.push(*entry);
                if is_plus_marker(marker.content) {
                    opened.first_block_owner = Some(*entry);
                }
                entry.column += width;
                entry.local_column += width;
                opened.inner = marker.content.trim_start();
                continue;
            }
            if let Some(quoted) = strip_blockquote_prefix(inner) {
                opened.openers_on_line += 1;
                opened.inner_quote_marker_column = Some(entry.column);
                opened.first_quote_marker_column.get_or_insert(entry.column);
                entry.kind = Kind::Quote;
                entry.content_column = entry.column;
                entry.hosted_quote = opened.list_opener_on_line || description_on_line;
                self.stack.push(*entry);
                entry.depth += 1;
                entry.column += inner.len() - quoted.len();
                entry.local_column = indent_columns(quoted);
                entry.content_column = entry.column;
                self.stack.push(*entry);
                if is_plus_marker(quoted) {
                    opened.first_block_owner = Some(*entry);
                }
                opened.inner = quoted.trim_start();
                continue;
            }
            let first_block = if inner.starts_with("[^") && inner.contains("]:") {
                entry.kind = Kind::Footnote;
                inner
                    .split_once("]:")
                    .is_some_and(|(_, body)| is_plus_marker(body.trim_start()))
            } else if inner.starts_with(": ") {
                entry.kind = Kind::Description;
                is_plus_marker(inner[1..].trim_start())
            } else {
                break;
            };
            entry.content_column = entry.column + 2;
            entry.hosted_quote = false;
            self.stack.push(*entry);
            if first_block {
                opened.first_block_owner = Some(*entry);
            }
            if entry.kind == Kind::Description {
                if let Some(content) = strip_description_prefix(inner)
                    .filter(|content| strip_blockquote_prefix(content).is_some())
                {
                    let width = (content.as_ptr() as usize) - (inner.as_ptr() as usize);
                    entry.column += width;
                    entry.local_column += width;
                    opened.inner = content;
                    description_on_line = true;
                    continue;
                }
            }
            break;
        }
        opened
    }

    fn note_opened_fence(&mut self, opened: &LineOpeners<'_>) {
        let fence_column = opened.entry.column;
        let rest = trim_ascii_start(opened.inner);
        self.paragraph_open = line_starts_paragraph(rest) && !item_block_opener(rest);
        let Some(fence) = opened
            .list_opener_on_line
            .then(|| detect_fence_open(opened.inner))
            .flatten()
        else {
            return;
        };
        self.code_fence = Some(FenceOpen {
            content_col: fence_column,
            ..fence
        });
        let nested = self
            .stack
            .iter()
            .filter(|entry| entry.kind == Kind::List)
            .count()
            > 1;
        if let Some(entry) = self.stack.last().filter(|entry| entry.kind == Kind::List) {
            if nested {
                self.marker_fence = Some((
                    FenceOpen {
                        content_col: fence_column,
                        ..fence
                    },
                    entry.column,
                ));
            }
        }
    }

    /// Decides a `+` that ends a line of container openers (`- +`, `> > +`).
    fn classify_first_block(
        &mut self,
        lines: &[String],
        shape: Shape,
        owner: Entry,
        opened: &LineOpeners<'_>,
    ) {
        let Shape { index, depth, .. } = shape;
        let line = lines.get(index + 1).map(String::as_str);
        if line.is_some() {
            #[cfg(test)]
            FOLLOWER_PROBES.with(|c| c.set(c.get() + 1));
        }
        let follower = FirstBlockFollower {
            line,
            body: line.map(|line| quote_body(line).1).unwrap_or(""),
            column: line.map_or(usize::MAX, |line| source_column(line, Some(owner.depth))),
            opener_column: line.map_or(usize::MAX, |line| source_column(line, Some(depth))),
        };
        // A stacked quote opener takes a non-list follower at column 0, its line's column,
        // or the column of the innermost `>`. A list at that last column attaches only when
        // it carries the owning quote marker (carve#3421, carve#3544).
        let stacked_quote = owner.kind == Kind::Quote && opened.openers_on_line >= 2;
        let list_at_owning_quote = stacked_quote
            && follower.line.is_some_and(|line| {
                let raw = stacked_follower_column(line, &lines[index], depth);
                Some(raw) == opened.inner_quote_marker_column
                    && quote_body(line).0 >= owner.depth
                    && detect_list_marker_full(follower.body).is_some()
            });
        let at_column = if stacked_quote {
            stacked_at_column(lines, shape, opened, follower, list_at_owning_quote)
        } else {
            self.unstacked_at_column(shape, owner, follower)
        };
        let at_column = at_column && !outside_quote(lines, shape, owner, opened, follower);
        let refusal = first_block_refusal(shape, owner, follower, at_column, list_at_owning_quote);
        // As below an indented quote line: carve-js reports nothing for this `+`.
        if stacked_quote
            && refusal.is_some()
            && leading_ws(&lines[index]) > 0
            && follower.line.is_some_and(column_zero_block)
        {
            self.reported[index] = true;
        }
        self.decisions[index] = Some(Decision {
            owner: Some(owner.kind),
            refusal,
            first_block: true,
            quote_depth: depth,
            after_quote_blank: false,
            sibling_interrupt: false,
            unclaimed: false,
        });
        if refusal.is_none() {
            self.held_owner = Some(owner);
        }
    }

    fn unstacked_at_column(
        &self,
        shape: Shape,
        owner: Entry,
        follower: FirstBlockFollower<'_>,
    ) -> bool {
        follower.column == owner.column
            || follower.opener_column == shape.column
            || self.stack.iter().any(|entry| {
                #[cfg(test)]
                STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
                entry.kind == Kind::List
                    && entry.depth < owner.depth
                    && entry.content_column == follower.opener_column
                    && follower
                        .line
                        .is_some_and(|line| !item_block_opener(line.trim_start()))
            })
            // Column 0 inside the host quotes, not the column of a host `>` (carve#3421).
            || follower.opener_column == 0 && detect_list_marker_full(follower.body).is_none()
    }
}

fn stacked_at_column(
    lines: &[String],
    shape: Shape,
    opened: &LineOpeners<'_>,
    follower: FirstBlockFollower<'_>,
    list_at_owning_quote: bool,
) -> bool {
    let Shape {
        index,
        depth,
        column,
        ..
    } = shape;
    follower.line.is_some_and(|line| {
        let raw = stacked_follower_column(line, &lines[index], depth);
        (detect_list_marker_full(follower.body).is_none() || list_at_owning_quote)
            && (raw == 0
                || raw == column
                || Some(raw) == opened.inner_quote_marker_column
                // No host prefix in line with the opener's: read at column 0.
                || raw == usize::MAX
                    && source_column(line, Some(0)) == 0
                    // A misaligned `>` is no host prefix (`> > - > +` / `>  > q`).
                    && quote_body(line).0 < depth.max(1)
                    // Below an indented host quote, a column-0 block has left
                    // the item that holds it (carve#3421).
                    && (leading_ws(&lines[index]) == 0 || !item_block_opener(line)))
    })
}

// A list's, note's or description's `+` inside a quote takes nothing from a line the
// quote does not reach: the quote has ended (carve#3421).
fn outside_quote(
    lines: &[String],
    shape: Shape,
    owner: Entry,
    opened: &LineOpeners<'_>,
    follower: FirstBlockFollower<'_>,
) -> bool {
    let Shape {
        index,
        depth,
        column,
        ..
    } = shape;
    owner.kind != Kind::Quote
        && owner.depth > 0
        && follower.line.is_some_and(|line| {
            let marker = source_column(line, Some(depth));
            quote_body(line).0 < owner.depth
                // An indented `>` is no host prefix (`> - - +` / ` > q`).
                || depth > 0
                    && stacked_follower_column(line, &lines[index], depth) == usize::MAX
                || owner.depth > depth
                    && quote_body(line).0 > depth
                    && marker != column
                    && Some(marker) != opened.first_quote_marker_column
        })
}

fn first_block_refusal(
    shape: Shape,
    owner: Entry,
    follower: FirstBlockFollower<'_>,
    at_column: bool,
    list_at_owning_quote: bool,
) -> Option<Refusal> {
    let sibling = matches!(owner.kind, Kind::List | Kind::Quote)
        && (follower.column == owner.column || follower.opener_column == shape.column)
        && detect_list_marker_full(follower.body).is_some()
        && !list_at_owning_quote;
    // CARVE-P9-067: a note's or a description's first-block form pulls no column-0 list.
    let column_zero_list = matches!(owner.kind, Kind::Footnote | Kind::Description)
        && follower.line.is_some_and(|line| {
            source_column(line, Some(0)) == 0 && detect_list_marker_full(line).is_some()
        });
    if follower.line.is_none() {
        Some(Refusal::Document)
    } else if follower.line.is_some_and(is_blank_line) {
        Some(Refusal::Blank)
    } else if column_zero_list {
        Some(Refusal::List)
    } else if sibling {
        Some(Refusal::Sibling)
    } else if !at_column && follower.column < owner.column {
        Some(Refusal::Left)
    } else if !at_column {
        Some(Refusal::Right)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_plus_probe_matches_line_iterator() {
        fn reference(source: &str) -> bool {
            source.lines().any(|line| {
                let trimmed = line.trim_end_matches([' ', '\t']);
                trimmed.strip_suffix('+').is_some_and(|prefix| {
                    prefix
                        .as_bytes()
                        .last()
                        .map_or(true, |byte| !byte.is_ascii_alphanumeric())
                })
            })
        }
        let fragments = [
            "", "+", "a+", "a +", "> +", "é+", "é +", "a+b+", "x\r+", "+x",
        ];
        let endings = ["", "\n", "\r\n", " \n", " \t\r\n", "\r"];
        for fragment in fragments {
            for ending in endings {
                for prefix in ["", "plain line\n", "plain+text\r\n"] {
                    let source = format!("{prefix}{fragment}{ending}");
                    assert_eq!(
                        may_have_standalone_plus(&source),
                        reference(&source),
                        "{source:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn prose_plus_signs_do_not_allocate_a_classifier() {
        for source in [
            "Block+\nNext line\n",
            "C++ and A+B\n",
            "A + B\n",
            "> quoted+\n",
        ] {
            let _guard = Guard::install(source);
            assert!(
                CLASSIFIER.with(|slot| slot.borrow().is_none()),
                "{source:?}"
            );
        }
    }

    #[test]
    fn standalone_marker_gate_keeps_container_prefixes() {
        for source in [
            "+\n",
            "  + \t\n",
            "- +\n",
            "> +\n",
            "> > +\n",
            "[^n]: +\n",
            "[^n]:+\n",
            ": +\n",
        ] {
            assert!(may_have_standalone_plus(source), "{source:?}");
            let _guard = Guard::install(source);
            assert!(
                CLASSIFIER.with(|slot| slot.borrow().is_some()),
                "{source:?}"
            );
        }
    }

    #[test]
    fn prose_plus_before_a_refused_marker_keeps_its_decision() {
        assert!(may_have_standalone_plus("+x +\n"));
        assert!(!may_have_standalone_plus("a+ \t\n"));
        let _guard = Guard::install("C++\n> - a\n>   > q\n>   +\n>   para\n");
        assert_eq!(decision(4).unwrap().refusal, Some(Refusal::WrongColumn));
    }

    #[test]
    fn owners_and_refusals_use_physical_columns() {
        for (source, marker_line, owner, refusal) in [
            ("- a\n  - b\n  +\n  > q", 3, Kind::List, None),
            ("- x\n  - y\n    - a\n\t+\n\t> q", 4, Kind::List, None),
            (
                "- a\n  - ```\n    code\n    ```\n\n  +\n  > q",
                6,
                Kind::List,
                None,
            ),
            ("  - +\n  > q", 1, Kind::List, None),
            ("- [ ] +\n> q", 1, Kind::List, None),
            ("- +\n- b", 1, Kind::List, Some(Refusal::Sibling)),
            ("- +\nfirst\n+\n- inner", 3, Kind::List, None),
            ("- a\n+\n- inner", 2, Kind::List, Some(Refusal::Sibling)),
            ("- a\n+\n", 2, Kind::List, Some(Refusal::Document)),
            ("> - a\n> +\n> > q", 2, Kind::List, None),
            ("> > q\n> +\n> para", 2, Kind::Quote, None),
            ("> - a\n> \n> +\n> tail", 3, Kind::List, None),
            (
                "[^a]: a\n\n+\npara",
                3,
                Kind::Footnote,
                Some(Refusal::WrongColumn),
            ),
            ("> :: t\n> : a\n> +\n> > q", 3, Kind::Description, None),
            (
                ":: term\n:  detail\n  - a\n    1. ordered\n      * [ ] task\n+\n- item",
                6,
                Kind::Description,
                None,
            ),
            (
                "- a\n  1. ordered\n    [^n]: note\n  +\n  > quote",
                4,
                Kind::List,
                None,
            ),
            (
                "> - foo\n>   - +\n>     > q",
                2,
                Kind::List,
                Some(Refusal::Right),
            ),
            ("- > a  \n  +  \n  b  ", 2, Kind::Quote, None),
            ("- a\n\t- > +\n  b", 2, Kind::Quote, Some(Refusal::Right)),
            ("- a\n\t- > +\n  # h", 2, Kind::Quote, Some(Refusal::Right)),
            ("> - > a\n>   +\n>   b", 2, Kind::Quote, None),
            (
                "> - a\n>   > q\n>   +\n>   para",
                3,
                Kind::Quote,
                Some(Refusal::WrongColumn),
            ),
            (
                "> quote\n> +\n```\ncode\n```",
                2,
                Kind::Quote,
                Some(Refusal::Left),
            ),
        ] {
            let _guard = Guard::install(source);
            let actual = decision(marker_line).expect(source);
            assert_eq!(actual.owner, Some(owner), "{source:?} {actual:?}");
            assert_eq!(actual.refusal, refusal, "{source:?}");
        }
    }

    #[test]
    fn glued_quote_plus_has_no_continuation_decision() {
        for source in [
            "> - a\n>+\n> tail",
            "> > - a\n> >+\n> > tail",
            "> > - a\n>> +\n> > tail",
        ] {
            let _guard = Guard::install(source);
            assert!(decision(2).is_none(), "{source:?}");
        }
    }

    #[test]
    fn accepted_owner_closes_before_a_dedented_sibling() {
        let _guard = Guard::install("- a\n  - b\n  +\n  > q\n- c\n  +\n  > r\n");
        assert_eq!(decision(3).unwrap().refusal, None);
        assert_eq!(decision(6).unwrap().refusal, Some(Refusal::WrongColumn));
    }

    #[test]
    fn a_fence_payload_does_not_open_a_marker_owner() {
        let _guard = Guard::install("- a\n  ```\n  - b\n  +\n  x\n  ```\n  +\n  > q\n");
        assert_eq!(decision(7).unwrap().refusal, Some(Refusal::WrongColumn));
    }

    #[test]
    fn repeated_shallow_markers_have_linear_classification_work() {
        let pattern = ["- a", "  - b", "  +", "  > q", "- c", "+", "text"];
        let mut previous = None;
        for count in [2_000, 8_000, 32_000] {
            let source = pattern
                .iter()
                .cycle()
                .take(count)
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            LINES_CLASSIFIED.with(|c| c.set(0));
            STACK_ENTRIES_EXAMINED.with(|c| c.set(0));
            FOLLOWER_PROBES.with(|c| c.set(0));
            let _document =
                super::super::parse_with_options(&source, &super::super::Options::default());
            let counters = (
                LINES_CLASSIFIED.with(std::cell::Cell::get),
                STACK_ENTRIES_EXAMINED.with(std::cell::Cell::get),
                FOLLOWER_PROBES.with(std::cell::Cell::get),
            );
            assert!(
                counters.0 > 0 && counters.0 <= count as u64,
                "{count} lines: {counters:?}"
            );
            if let Some((lines, entries, probes)) = previous {
                assert!(counters.0 <= lines * 5, "classified lines: {counters:?}");
                assert!(counters.1 <= entries * 5, "stack entries: {counters:?}");
                assert!(counters.2 <= probes * 5, "follower probes: {counters:?}");
            }
            previous = Some(counters);
        }
    }

    #[test]
    fn repeated_tight_quote_markers_have_linear_classification_work() {
        let pattern = ["> - a", ">+", "> +", "> tail", "> > - b", "> >+", "> > end"];
        let mut previous = None;
        for count in [2_000, 8_000, 32_000] {
            let source = pattern
                .iter()
                .cycle()
                .take(count)
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            LINES_CLASSIFIED.with(|c| c.set(0));
            STACK_ENTRIES_EXAMINED.with(|c| c.set(0));
            FOLLOWER_PROBES.with(|c| c.set(0));
            let _document =
                super::super::parse_with_options(&source, &super::super::Options::default());
            let counters = (
                LINES_CLASSIFIED.with(std::cell::Cell::get),
                STACK_ENTRIES_EXAMINED.with(std::cell::Cell::get),
                FOLLOWER_PROBES.with(std::cell::Cell::get),
            );
            assert!(counters.0 > 0 && counters.0 <= count as u64, "{counters:?}");
            if let Some((lines, entries, probes)) = previous {
                assert!(counters.0 <= lines * 5, "classified lines: {counters:?}");
                assert!(counters.1 <= entries * 5, "stack entries: {counters:?}");
                assert!(counters.2 <= probes * 5, "follower probes: {counters:?}");
            }
            previous = Some(counters);
        }
    }
}
