//! Physical-line ownership for standalone continuation markers.

use std::cell::RefCell;
use std::collections::HashMap;

use super::{
    detect_fence_open, detect_list_marker_full, indent_columns, is_blank_line, is_fence_close,
    is_plus_marker, item_block_opener, leading_ws, note_lint_event, strip_blockquote_prefix,
    strip_description_prefix, FenceOpen, LintEvent,
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
    stack: Vec<Entry>,
    held_owner: Option<Entry>,
    pending_blank: bool,
    closed_quote_on_blank: bool,
    code_fence: Option<FenceOpen>,
    marker_fence: Option<(FenceOpen, usize)>,
    matching_code_closer: Vec<bool>,
    decisions: Vec<Option<Decision>>,
    reported: Vec<bool>,
    scanned: usize,
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
        if !source.contains('+') {
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
            stack: Vec::new(),
            held_owner: None,
            pending_blank: false,
            closed_quote_on_blank: false,
            code_fence: None,
            marker_fence: None,
            matching_code_closer,
            decisions,
            reported,
            scanned: 0,
        };
        Self(CLASSIFIER.with(|slot| slot.replace(Some(classifier))))
    }
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
        let found = classifier.decisions.get(index).copied().flatten();
        if let Some(decision) = found {
            // A document-level `+` no container claims is reported by the paragraph
            // reader, and only before a block (carve#3421).
            if let Some(reason) = decision.refusal.filter(|_| !decision.unclaimed) {
                if !classifier.reported[index] {
                    classifier.reported[index] = true;
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

pub(super) fn preceding_nested_list_marker(source_line: usize) -> bool {
    CLASSIFIER.with(|slot| {
        let slot = slot.borrow();
        let Some(classifier) = slot.as_ref() else {
            return false;
        };
        for line in classifier
            .lines
            .iter()
            .take(source_line.saturating_sub(1))
            .rev()
            .skip_while(|line| is_blank_line(line))
        {
            if is_blank_line(line) || detect_fence_open(line.trim_start()).is_some() {
                break;
            }
            if indent_columns(line) > 0
                && detect_list_marker_full(line.trim_start_matches([' ', '\t'])).is_some()
            {
                return true;
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

impl Classifier {
    fn step(&mut self) {
        let index = self.scanned;
        self.scanned += 1;
        #[cfg(test)]
        LINES_CLASSIFIED.with(|c| c.set(c.get() + 1));
        let line = &self.lines[index];
        let (depth, body) = quote_body(line);
        let column = source_column(line, None);
        if let Some(open) = self.code_fence {
            if is_blank_line(body) || column >= open.content_col {
                if is_blank_line(body) {
                    self.pending_blank = true;
                }
                if column == open.content_col && is_fence_close(body, open) {
                    self.code_fence = None;
                    self.marker_fence = None;
                }
                return;
            }
            self.code_fence = None;
            if !is_plus_marker(body) {
                self.marker_fence = None;
            }
        }
        if let Some(mut open) = self.matching_code_closer[index]
            .then(|| detect_fence_open(body))
            .flatten()
        {
            open.content_col = column;
            self.code_fence = Some(open);
            return;
        }
        let (local, has_tab) = local_column(line, depth);
        let marker_column = if depth > 0 && has_tab { local } else { column };
        let relative_quote_column = depth > 0 && (has_tab || tight_quote_prefix(line, depth));
        let owner_column = if relative_quote_column {
            local
        } else {
            marker_column
        };
        // A deeper quote opened inside a list has no marker owner at this depth.
        if depth > 0
            && local == 0
            && index > 0
            && quote_body(&self.lines[index - 1]).0 > 0
            && quote_body(&self.lines[index - 1]).0 < depth
            && self
                .stack
                .iter()
                .any(|entry| entry.kind == Kind::List && entry.depth == 0)
            && is_plus_marker(body)
        {
            return;
        }
        if is_plus_marker(body) {
            let mut deepest_quote = 0;
            let mut list_at_column = false;
            let mut inner_list_open = false;
            let mut inner_list_blocks_quote = false;
            for entry in &self.stack {
                #[cfg(test)]
                STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
                if entry.kind == Kind::Quote {
                    deepest_quote = deepest_quote.max(entry.depth);
                }
                if depth > 0 && entry.kind == Kind::List && entry.depth == depth {
                    inner_list_open = true;
                    inner_list_blocks_quote |= entry.column >= marker_column;
                }
                if entry.kind == Kind::List
                    && entry.depth == depth
                    && (if depth > 0 && has_tab {
                        entry.local_column
                    } else {
                        entry.column
                    }) == marker_column
                {
                    list_at_column = true;
                }
            }
            let mut owner = None;
            let mut owner_index = None;
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
                        || depth >= deepest_quote
                        || matches!(entry.kind, Kind::List | Kind::Quote))
                    && !(list_at_column && entry.kind == Kind::Quote)
                {
                    owner = Some(*entry);
                    owner_index = Some(at);
                    break;
                }
            }
            let has_ancestor_list = owner_index.is_some_and(|at| {
                self.stack[..at].iter().any(|entry| {
                    #[cfg(test)]
                    STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
                    matches!(entry.kind, Kind::List | Kind::Description)
                        && owner.is_some_and(|owner| entry.depth <= owner.depth)
                })
            });
            let first_block_quote = owner.is_none()
                && depth > 0
                && local == 0
                && !inner_list_open
                && self.lines.get(index + 1).map_or(true, |line| {
                    let (next_depth, next_body) = quote_body(line);
                    next_depth < depth
                        || is_blank_line(next_body)
                        || next_depth == depth
                            && self
                                .stack
                                .iter()
                                .any(|entry| entry.kind == Kind::List && entry.depth == 0)
                            && index > 0
                            && quote_body(&self.lines[index - 1]).0 >= depth
                })
                && !index.checked_sub(1).is_some_and(|prior| {
                    quote_body(&self.lines[prior]).0 > 0
                        && source_column(&self.lines[prior], None) > marker_column
                })
                && !index.checked_sub(1).is_some_and(|prior| {
                    quote_body(&self.lines[prior]).0 >= depth
                        && is_blank_line(quote_body(&self.lines[prior]).1)
                        && !self
                            .stack
                            .iter()
                            .any(|entry| entry.kind == Kind::List && entry.depth == 0)
                })
                && !index.checked_sub(1).is_some_and(|prior| {
                    is_blank_line(&self.lines[prior])
                        && self.lines[..prior]
                            .iter()
                            .rev()
                            .find(|line| !is_blank_line(line))
                            .is_some_and(|line| {
                                let (prior_depth, prior_body) = quote_body(line);
                                prior_depth > 0 && detect_list_marker_full(prior_body).is_some()
                            })
                });
            if first_block_quote {
                owner = Some(Entry::new(
                    Kind::Quote,
                    (marker_column, local),
                    depth,
                    0,
                    false,
                ));
            }
            let follower = self.lines.get(index + 1).map(String::as_str);
            if follower.is_some() {
                #[cfg(test)]
                FOLLOWER_PROBES.with(|c| c.set(c.get() + 1));
            }
            let follower_col = follower.map_or(usize::MAX, |line| {
                if relative_quote_column {
                    local_column(line, depth).0
                } else {
                    source_column(line, Some(depth))
                }
            });
            let follower_body = follower.map(|line| quote_body(line).1).unwrap_or("");
            let follower_depth = follower.map(|line| quote_body(line).0);
            let sibling = owner.is_some_and(|entry| {
                entry.kind == Kind::List
                    || entry.kind == Kind::Quote
                        && (has_ancestor_list || depth > 0 && deepest_quote == depth)
            }) && follower_depth == Some(depth)
                && follower_col == owner_column
                && detect_list_marker_full(follower_body).is_some();
            let first_block_attaches = first_block_quote
                && ((index == 0 || quote_body(&self.lines[index - 1]).0 < depth)
                    && follower.is_some_and(|line| {
                        let column = source_column(line, Some(0));
                        // CARVE-P9-067: inside an item the follower may also sit at the quote line's column.
                        // A block at column 0 below an indented quote line has left the item; only
                        // paragraph text there is the quote's (carve#3421).
                        (column == 0
                            && (source_column(&self.lines[index], Some(0)) == 0
                                || !item_block_opener(line))
                            || quote_body(line).0 == 0
                                && column == source_column(&self.lines[index], Some(0))
                            // The quote's own first marker, in line with it (`  > > > +` /
                            // `  > q`), carries the follower into the innermost quote.
                            || column > 0
                                && quote_body(line).0 > 0
                                && column == source_column(&self.lines[index], Some(0)))
                            && detect_list_marker_full(follower_body).is_none()
                    })
                    || follower_depth == Some(depth)
                        && self
                            .stack
                            .iter()
                            .any(|entry| entry.kind == Kind::List && entry.depth == 0)
                        && index > 0
                        && quote_body(&self.lines[index - 1]).0 >= depth);
            let refusal = if owner.is_none()
                || inner_list_open
                    && owner.is_some_and(|entry| {
                        entry.kind == Kind::Quote
                            && (inner_list_blocks_quote || !entry.hosted_quote)
                    })
                || self.pending_blank && owner.is_some_and(|entry| entry.kind == Kind::Footnote)
            {
                Some(Refusal::WrongColumn)
            } else if follower.is_none() {
                Some(Refusal::Document)
            } else if is_blank_line(follower_body) {
                Some(Refusal::Blank)
            } else if first_block_quote && !first_block_attaches {
                // CARVE-P9-067: the quote's first-block form refuses a column-0 list and any other column.
                let follower_column = follower.map_or(0, |line| source_column(line, Some(0)));
                if follower_column == 0
                    && follower.is_some_and(|line| detect_list_marker_full(line).is_some())
                {
                    Some(Refusal::List)
                } else if follower_column > source_column(&self.lines[index], Some(0)) {
                    Some(Refusal::Right)
                } else {
                    Some(Refusal::Left)
                }
            } else if !first_block_attaches
                && (follower_depth.is_some_and(|next_depth| next_depth < depth)
                    || follower_col < owner_column)
            {
                Some(Refusal::Left)
            } else if !first_block_attaches && follower_col > owner_column {
                Some(Refusal::Right)
            } else if sibling && self.held_owner != owner {
                Some(Refusal::Sibling)
            } else if depth == 0
                && deepest_quote > 0
                && owner.is_some_and(|entry| entry.kind == Kind::Description)
                && follower_col == owner_column
                && detect_list_marker_full(follower_body).is_some()
            {
                Some(Refusal::WrongColumn)
            } else {
                None
            };
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
                && source_column(&self.lines[index], Some(0)) > 0
                && follower.is_some_and(|line| {
                    !line.starts_with([' ', '\t'])
                        && item_block_opener(line)
                        && detect_list_marker_full(line).is_none()
                })
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
                    || depth > 0
                        && local == 0
                        && index > 0
                        && quote_body(&self.lines[index - 1]).0 >= depth,
                unclaimed: owner.is_none() && depth == 0 && self.stack.is_empty(),
            });
            if refusal.is_none() {
                self.held_owner = owner;
            }
            if depth == 0 {
                self.closed_quote_on_blank = false;
            }
            return;
        }
        if is_blank_line(body) {
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
            return;
        }
        if self.marker_fence.is_some_and(|(fence, _)| {
            column < fence.content_col
                || (column == fence.content_col && is_fence_close(body, fence))
        }) {
            self.marker_fence = None;
        }
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
        let mut inner = body;
        let mut entry = Entry::new(Kind::List, (column, local), depth, 0, false);
        let mut first_block_owner = None;
        let mut list_opener_on_line = false;
        let mut description_on_line = false;
        // Openers on this line; a task box counts as one.
        let mut openers_on_line = 0usize;
        let mut inner_quote_marker_column = None;
        let mut first_quote_marker_column = None;
        for _ in 0..32 {
            if let Some(marker) = detect_list_marker_full(inner) {
                list_opener_on_line = true;
                openers_on_line += 1 + usize::from(marker.checked.is_some());
                let width = (marker.content.as_ptr() as usize) - (inner.as_ptr() as usize);
                entry.kind = Kind::List;
                entry.content_column = entry.column + width;
                entry.hosted_quote = false;
                self.stack.push(entry);
                if is_plus_marker(marker.content) {
                    first_block_owner = Some(entry);
                }
                entry.column += width;
                entry.local_column += width;
                inner = marker.content.trim_start();
                continue;
            }
            if let Some(quoted) = strip_blockquote_prefix(inner) {
                openers_on_line += 1;
                inner_quote_marker_column = Some(entry.column);
                first_quote_marker_column.get_or_insert(entry.column);
                entry.kind = Kind::Quote;
                entry.content_column = entry.column;
                entry.hosted_quote = list_opener_on_line || description_on_line;
                self.stack.push(entry);
                entry.depth += 1;
                entry.column += inner.len() - quoted.len();
                entry.local_column = indent_columns(quoted);
                entry.content_column = entry.column;
                self.stack.push(entry);
                if is_plus_marker(quoted) {
                    first_block_owner = Some(entry);
                }
                inner = quoted.trim_start();
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
            self.stack.push(entry);
            if first_block {
                first_block_owner = Some(entry);
            }
            if entry.kind == Kind::Description {
                if let Some(content) = strip_description_prefix(inner)
                    .filter(|content| strip_blockquote_prefix(content).is_some())
                {
                    let width = (content.as_ptr() as usize) - (inner.as_ptr() as usize);
                    entry.column += width;
                    entry.local_column += width;
                    inner = content;
                    description_on_line = true;
                    continue;
                }
            }
            break;
        }
        let fence_column = entry.column;
        if let Some(fence) = list_opener_on_line
            .then(|| detect_fence_open(inner))
            .flatten()
        {
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
        if let Some(owner) = first_block_owner {
            let follower = self.lines.get(index + 1).map(String::as_str);
            if follower.is_some() {
                #[cfg(test)]
                FOLLOWER_PROBES.with(|c| c.set(c.get() + 1));
            }
            let follower_body = follower.map(|line| quote_body(line).1).unwrap_or("");
            let follower_column =
                follower.map_or(usize::MAX, |line| source_column(line, Some(owner.depth)));
            let opener_column =
                follower.map_or(usize::MAX, |line| source_column(line, Some(depth)));
            // A stacked quote opener (`- > +`, `- > > +`, `- - > +`) takes a non-list follower at
            // column 0, its line's column, or the column of the innermost `>`, which owns the `+`;
            // an intervening opener's column is refused (carve#3421).
            let stacked_quote = owner.kind == Kind::Quote && openers_on_line >= 2;
            let at_column = if stacked_quote {
                follower.is_some_and(|line| {
                    let raw = stacked_follower_column(line, &self.lines[index], depth);
                    detect_list_marker_full(follower_body).is_none()
                        && (raw == 0
                            || raw == column
                            || Some(raw) == inner_quote_marker_column
                            // No host prefix in line with the opener's: read at column 0.
                            || raw == usize::MAX
                                && source_column(line, Some(0)) == 0
                                // A misaligned `>` is no host prefix (`> > - > +` / `>  > q`).
                                && quote_body(line).0 < depth.max(1)
                                // Below an indented host quote, a column-0 block has left
                                // the item that holds it (carve#3421).
                                && (leading_ws(&self.lines[index]) == 0
                                    || !item_block_opener(line)))
                })
            } else {
                follower_column == owner.column
                    || opener_column == column
                    || self.stack.iter().any(|entry| {
                        #[cfg(test)]
                        STACK_ENTRIES_EXAMINED.with(|c| c.set(c.get() + 1));
                        entry.kind == Kind::List
                            && entry.depth < owner.depth
                            && entry.content_column == opener_column
                            && follower.is_some_and(|line| !item_block_opener(line.trim_start()))
                    })
                    // Column 0 inside the host quotes, not the column of a host `>` (carve#3421).
                    || opener_column == 0 && detect_list_marker_full(follower_body).is_none()
            };
            // A list's, note's or description's `+` inside a quote takes nothing from a line the
            // quote does not reach: the quote has ended (carve#3421).
            let outside_quote = owner.kind != Kind::Quote
                && owner.depth > 0
                && follower.is_some_and(|line| {
                    let marker = source_column(line, Some(depth));
                    quote_body(line).0 < owner.depth
                        // An indented `>` is no host prefix (`> - - +` / ` > q`).
                        || depth > 0
                            && stacked_follower_column(line, &self.lines[index], depth)
                                == usize::MAX
                        || owner.depth > depth
                            && quote_body(line).0 > depth
                            && marker != column
                            && Some(marker) != first_quote_marker_column
                });
            let at_column = at_column && !outside_quote;
            let sibling = matches!(owner.kind, Kind::List | Kind::Quote)
                && (follower_column == owner.column || opener_column == column)
                && detect_list_marker_full(follower_body).is_some();
            // CARVE-P9-067: a note's or a description's first-block form pulls no column-0 list.
            let column_zero_list = matches!(owner.kind, Kind::Footnote | Kind::Description)
                && follower.is_some_and(|line| {
                    source_column(line, Some(0)) == 0 && detect_list_marker_full(line).is_some()
                });
            let refusal = if follower.is_none() {
                Some(Refusal::Document)
            } else if is_blank_line(follower_body) {
                Some(Refusal::Blank)
            } else if column_zero_list {
                Some(Refusal::List)
            } else if sibling {
                Some(Refusal::Sibling)
            } else if !at_column && follower_column < owner.column {
                Some(Refusal::Left)
            } else if !at_column {
                Some(Refusal::Right)
            } else {
                None
            };
            // As below an indented quote line: carve-js reports nothing for this `+`.
            if stacked_quote
                && refusal.is_some()
                && leading_ws(&self.lines[index]) > 0
                && follower.is_some_and(|line| {
                    !line.starts_with([' ', '\t'])
                        && item_block_opener(line)
                        && detect_list_marker_full(line).is_none()
                })
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let _document = super::super::parse(&source);
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
        let pattern = ["> - a", ">+", "> tail", "> > - b", "> >+", "> > end"];
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
            let _document = super::super::parse(&source);
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
