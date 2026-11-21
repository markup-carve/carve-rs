//! Source and tree records consumed by the pinned 0.2 lint model, borrowed from the parse and the line model.
#![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

use super::data_interpreter::{F, Val};
use super::{Holder, Range, lines::Line};
use crate::source::Source;
use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::HashMap;

fn text_at(source: &str, from: usize, to: usize) -> &str {
    source
        .get(from.min(source.len())..to.min(source.len()))
        .unwrap_or("")
}

fn trim_end(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

fn quoted_start(text: &str, marks: usize) -> usize {
    let bytes = text.as_bytes();
    let mut at = 0;
    while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
        at += 1;
    }
    for _ in 0..marks {
        if bytes.get(at) != Some(&b'>') || bytes.get(at + 1).is_some_and(|b| *b != b' ') {
            break;
        }
        at += 1;
        if bytes.get(at) == Some(&b' ') {
            at += 1;
        }
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
    }
    at
}

fn quote_marks(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut marks = 0;
    loop {
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'>')
            || !bytes.get(at + 1).is_none_or(|b| *b == b' ' || *b == b'\t')
        {
            break;
        }
        marks += 1;
        at += 1;
    }
    marks
}

fn quote_marks_space(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut marks = 0;
    loop {
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'>') || bytes.get(at + 1) != Some(&b' ') {
            break;
        }
        marks += 1;
        at += 1;
    }
    marks
}

fn shape_heading(body: &str) -> bool {
    let bytes = body.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    let run = bytes.iter().take_while(|b| **b == first).count();
    matches!(first, b'#' | b'=') && run <= 6 && run > 0 && bytes.get(run) == Some(&b' ')
}

fn shape_attribute(body: &str) -> bool {
    body.starts_with('{')
        && crate::attr::parse_glued(body, 0)
            .is_some_and(|(block, end)| !block.empty && end == body.len())
}

fn shape_quote(body: &str) -> bool {
    body.starts_with('>') && (body.len() == 1 || body.as_bytes().get(1) == Some(&b' '))
}

fn shape_div_bare_run(body: &str) -> bool {
    body.starts_with(":::") && trim_end(body).bytes().all(|b| b == b':')
}

fn shape_bullet(body: &str) -> bool {
    body.starts_with(['-', '*']) && body.get(1..2) == Some(" ")
}

fn shape_ordered(body: &str) -> bool {
    body.as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && body
            .find(['.', ')'])
            .is_some_and(|at| body.get(at + 1..at + 2) == Some(" "))
}

/// The line shapes in the order the first that fits names the line; a line no shape fits is `text`.
type ShapeTest = fn(&str) -> bool;

const SHAPES: [(&str, ShapeTest); 20] = [
    ("blank", str::is_empty),
    ("heading", shape_heading),
    ("quote", shape_quote),
    ("comment_block_opener", |b| b.starts_with("%%%")),
    ("comment_only", |b| b.starts_with("%%")),
    ("caption", |b| b.starts_with("^ ")),
    ("include", |b| b.starts_with("{{") && b.ends_with("}}")),
    ("attribute_line", shape_attribute),
    ("div_bare_run", shape_div_bare_run),
    ("div_opener", |b| b.starts_with(":::")),
    ("term", |b| b.starts_with(":: ")),
    ("description", |b| b.starts_with(": ")),
    ("table_row", |b| b.starts_with('|') && b.ends_with('|')),
    ("code_fence_opener", |b| b.starts_with("```")),
    ("thematic_break", |b| {
        b.starts_with("---") || b.starts_with("***") || b.starts_with("___")
    }),
    ("footnote_definition", |b| {
        b.starts_with("[^") && b.contains("]:")
    }),
    ("link_definition", |b| {
        b.starts_with('[') && b.contains("]:")
    }),
    ("bullet", shape_bullet),
    ("ordered", shape_ordered),
    ("text", |_| true),
];

fn shape(body: &str) -> &'static str {
    SHAPES
        .iter()
        .find(|(_, fits)| fits(body))
        .map_or("text", |(name, _)| *name)
}

/// Whether `shape(body) == name`, testing only the shapes up to `name`.
pub(super) fn shape_is(body: &str, name: &str) -> bool {
    for (known, fits) in &SHAPES {
        if fits(body) {
            return *known == name;
        }
        if *known == name {
            return false;
        }
    }
    false
}

fn column_of(source: &Source<'_>, at: usize, quotes: usize) -> usize {
    let (start, _) = source.line_range(source.line_of(at));
    let prefix = source.slice(start, at);
    let mut chars = prefix.chars().peekable();
    let mut column: usize = 0;
    let mut origin: usize = 0;
    for _ in 0..quotes {
        while let Some(ch) = chars.peek().copied() {
            if ch != ' ' && ch != '\t' {
                break;
            }
            column = if ch == '\t' {
                column + 4 - column % 4
            } else {
                column + 1
            };
            chars.next();
        }
        if chars.peek() != Some(&'>') {
            break;
        }
        chars.next();
        column += 1;
        if chars.peek() == Some(&' ') {
            chars.next();
            column += 1;
        }
        origin = column;
    }
    for ch in chars {
        column = if ch == '\t' {
            column + 4 - column % 4
        } else if ch == '\u{feff}' && column == 0 {
            column
        } else {
            column + 1
        };
    }
    column.saturating_sub(origin)
}

fn opener_start(source: &str, mut at: usize, marks: usize) -> usize {
    while source.get(at..).is_some_and(|s| s.starts_with('{')) {
        let Some(end) = source.get(at..).and_then(|s| s.find('\n')) else {
            break;
        };
        at += end + 1;
        while source
            .as_bytes()
            .get(at)
            .is_some_and(|b| matches!(b, b' ' | b'\t'))
        {
            at += 1;
        }
        for _ in 0..marks {
            if source.as_bytes().get(at) != Some(&b'>') {
                break;
            }
            at += 1;
            while source
                .as_bytes()
                .get(at)
                .is_some_and(|b| matches!(b, b' ' | b'\t'))
            {
                at += 1;
            }
        }
    }
    at
}

fn block_opener_text<'a>(source: &'a str, opener: usize, kind: &str, opener_rule: bool) -> &'a str {
    if !opener_rule && !matches!(kind, "directive" | "list") {
        return "";
    }
    let end = source
        .get(opener..)
        .and_then(|s| s.find('\n'))
        .map_or(source.len(), |n| opener + n);
    trim_end(text_at(source, opener, end))
}

const INLINE_HOLDERS: [&str; 11] = [
    "paragraph",
    "caption",
    "definition_term",
    "heading",
    "line",
    "cell_paragraph",
    "code_block",
    "raw_block",
    "frontmatter",
    "comment",
    "include",
];
const TEXT_LEAVES: [&str; 6] = [
    "paragraph",
    "caption",
    "definition_term",
    "heading",
    "line",
    "table_cell",
];
const LINE_LEAVES: [&str; 11] = [
    "paragraph",
    "caption",
    "definition_term",
    "heading",
    "line",
    "table_cell",
    "table_row",
    "code_block",
    "raw_block",
    "frontmatter",
    "comment",
];
const INLINE_KINDS: [&str; 17] = [
    "text",
    "image",
    "emphasis",
    "strong",
    "highlight",
    "strike",
    "link",
    "raw_inline",
    "code",
    "span",
    "subscript",
    "superscript",
    "autolink",
    "reference",
    "footnote_ref",
    "citation",
    "wikilink",
];
const OPENER_RULE_KINDS: [&str; 12] = [
    "admonition",
    "block_quote",
    "code_block",
    "definition_term",
    "directive",
    "div",
    "figure_group",
    "heading",
    "line_block",
    "raw_block",
    "table",
    "thematic_break",
];

fn verbatim(kind: &str) -> bool {
    matches!(
        kind,
        "code_block" | "raw_block" | "frontmatter" | "comment_block"
    )
}

/// An open colon container on a line: the values of an `open_colon` item.
pub(super) struct Colon {
    from: usize,
    end: usize,
    closed: bool,
    owner: Option<u32>,
    width: usize,
    line: usize,
    column: usize,
    kind: &'static str,
}

/// One source line of the line model (C2-W-139).
pub(super) struct LineRec<'a> {
    pub start: usize,
    end: usize,
    pub text: &'a str,
    body: &'a str,
    body_start: usize,
    leaf: Option<u32>,
    pub leaf_kind: &'a str,
    quote_depth: usize,
    quote_marks: usize,
    attached: bool,
    closes: bool,
    colons: (u32, u32),
    pub path: &'a str,
}

impl<'a> LineRec<'a> {
    pub fn body(&self) -> &'a str {
        self.body
    }
}

/// A block record for a node a block rule can select; the fields that scan the source are read on demand.
pub(super) struct BlockRec<'a> {
    pub range: u32,
    pub kind: &'a str,
    pub line_index: usize,
    last_line_index: usize,
    quotes: usize,
    item_child: bool,
    opener_rule: bool,
    opener_start: usize,
    column: OnceCell<usize>,
    mark_column: OnceCell<usize>,
    parent: Option<u32>,
    owner: Option<u32>,
    item_owner: Option<u32>,
}

/// An inline record for a node an inline rule can select.
pub(super) struct InlineRec<'a> {
    pub range: u32,
    pub kind: &'a str,
    pub line_index: usize,
    start: usize,
    end: usize,
    text: &'a str,
    leaf: &'a str,
    parent: &'a str,
}

/// The record a rule runs over.
pub(super) enum Subject<'r, 'a> {
    Line(usize),
    Block(&'r BlockRec<'a>),
    Inline(&'r InlineRec<'a>),
}

/// What a block or inline node is to the records: the node classes of C2-W-139.
pub(super) enum Node<'a> {
    Block(BlockRec<'a>),
    Inline(InlineRec<'a>),
    Skip,
}

pub(super) struct Records<'a> {
    source: &'a str,
    positions: &'a Source<'a>,
    pub ranges: &'a [Range<'a>],
    holders: &'a [Holder],
    root: Option<u32>,
    pub lines: Vec<LineRec<'a>>,
    colons: Vec<Colon>,
    line_colons: Vec<u32>,
    holder_quotes: Vec<usize>,
    holder_inline: Vec<bool>,
    owners: Vec<OnceCell<(usize, Option<usize>)>>,
}

impl<'a> Records<'a> {
    pub fn new(
        source: &'a str,
        positions: &'a Source<'a>,
        ranges: &'a [Range<'a>],
        holders: &'a [Holder],
        model: &'a [Line],
    ) -> Self {
        let mut holder_quotes = Vec::with_capacity(holders.len());
        let mut holder_inline = Vec::with_capacity(holders.len());
        // A type name is one `&'static str` per type, so its flags are looked up by address.
        let mut flags: [Option<(&str, bool, bool)>; 64] = [None; 64];
        for holder in holders {
            let (quote, inline) = holder.range.map_or((false, false), |at| {
                let kind = ranges[at as usize].kind;
                let slot = &mut flags[(kind.as_ptr() as usize >> 3) & 63];
                if let Some((known, quote, inline)) = *slot {
                    if std::ptr::eq(known, kind) {
                        return (quote, inline);
                    }
                }
                let found = (kind == "block_quote", INLINE_HOLDERS.contains(&kind));
                *slot = Some((kind, found.0, found.1));
                found
            });
            let up = holder.up.map(|up| up as usize);
            holder_quotes.push(up.map_or(0, |up| holder_quotes[up]) + usize::from(quote));
            holder_inline.push(up.is_some_and(|up| holder_inline[up]) || inline);
        }
        let mut records = Self {
            source,
            positions,
            ranges,
            holders,
            // The walk records a node before its children, so the document range, if any, comes first.
            root: ranges
                .first()
                .filter(|range| range.path.is_empty())
                .map(|_| 0),
            lines: Vec::new(),
            colons: Vec::new(),
            line_colons: Vec::new(),
            holder_quotes,
            holder_inline,
            owners: (0..=holders.len()).map(|_| OnceCell::new()).collect(),
        };
        records.build_lines(model);
        records
    }

    pub fn positions(&self) -> &'a Source<'a> {
        self.positions
    }

    fn kind(&self, range: u32) -> &'a str {
        self.ranges[range as usize].kind
    }

    /// Block quotes at or above a holder.
    fn quotes_at(&self, holder: Option<u32>) -> usize {
        holder.map_or(0, |at| self.holder_quotes[at as usize])
    }

    fn inline_held(&self, range: &Range) -> bool {
        range
            .holder
            .is_some_and(|at| self.holder_inline[at as usize])
    }

    /// The range of a holder; the document's is the root range.
    fn holder_range(&self, holder: Option<u32>) -> Option<u32> {
        holder.map_or(self.root, |at| self.holders[at as usize].range)
    }

    fn holder_kind(&self, holder: Option<u32>) -> &'a str {
        self.holder_range(holder)
            .map_or("document", |at| self.kind(at))
    }

    fn up(&self, holder: Option<u32>) -> Option<u32> {
        holder.and_then(|at| self.holders[at as usize].up)
    }

    pub fn id_text(&self, id: Option<u32>) -> &'a str {
        id.map_or("", |at| self.ranges[at as usize].path.as_str())
    }

    pub fn path_of(&self, range: u32) -> String {
        self.ranges[range as usize].path.as_str().to_owned()
    }

    /// The types of the blocks that hold a leaf, innermost first, then the document.
    pub fn ancestors(&self, leaf: Option<u32>) -> impl Iterator<Item = &'a str> + '_ {
        let mut holder = leaf.and_then(|at| self.ranges[at as usize].holder);
        let mut done = leaf.is_none();
        std::iter::from_fn(move || {
            while let Some(at) = holder {
                let node = &self.holders[at as usize];
                holder = node.up;
                if let Some(range) = node.range {
                    return Some(self.kind(range));
                }
            }
            if done {
                return None;
            }
            done = true;
            Some("document")
        })
    }

    /// The content and marker columns of the node at a holder's path.
    fn owner(&self, holder: Option<u32>) -> (usize, Option<usize>) {
        let slot = holder.map_or(0, |at| at as usize + 1);
        *self.owners[slot].get_or_init(|| self.owner_columns(holder))
    }

    fn owner_columns(&self, holder: Option<u32>) -> (usize, Option<usize>) {
        let kind = self.holder_kind(holder);
        let Some(range) = self
            .holder_range(holder)
            .map(|at| &self.ranges[at as usize])
        else {
            return (0, None);
        };
        let quotes = self.quotes_at(self.up(holder));
        let source = self.positions;
        let marker = Some(column_of(source, range.start, quotes));
        if matches!(kind, "block_quote" | "document") {
            return (0, marker);
        }
        if !matches!(kind, "list_item" | "footnote" | "definition_description") {
            return (column_of(source, range.start, quotes), marker);
        }
        let first = source.line(range.start_line.saturating_sub(1));
        let marker_at = range
            .start
            .saturating_sub(source.line_range(range.start_line.saturating_sub(1)).0);
        let suffix = first.get(marker_at..).unwrap_or("");
        let marker_len = match kind {
            "definition_description" => 1,
            "footnote" => suffix.find(':').map_or(0, |i| i + 1),
            _ if suffix.starts_with(['-', '*', '.']) => 1,
            _ => suffix.find(['.', ')']).map_or(0, |i| i + 1),
        };
        let bytes = self.source.as_bytes();
        let mut at = range.start + marker_len;
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        if kind == "list_item" && self.source.get(at..).is_some_and(|s| s.starts_with("{.")) {
            if let Some(close) = self.source.get(at..).and_then(|s| s.find('}')) {
                at += close + 1;
            }
            while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                at += 1;
            }
        }
        (column_of(source, at, quotes), marker)
    }

    fn lazy(&self, index: usize) -> bool {
        let line = &self.lines[index];
        let Some(leaf) = line.leaf else {
            return false;
        };
        let range = &self.ranges[leaf as usize];
        let can_be_lazy = TEXT_LEAVES.contains(&line.leaf_kind) && range.start_line != index + 1;
        if !can_be_lazy {
            return false;
        }
        let carried = quote_marks_space(line.text);
        if carried < line.quote_depth {
            return true;
        }
        let mut holder = range.holder;
        while let Some(at) = holder {
            let node = &self.holders[at as usize];
            if node.range.is_some_and(|range| {
                matches!(
                    self.kind(range),
                    "list_item" | "footnote" | "definition_description"
                )
            }) {
                return column_of(self.positions, line.body_start, carried) < self.owner(holder).0;
            }
            holder = node.up;
        }
        false
    }

    /// `prev.leaf_id == leaf_id` of a line.
    pub fn prev_same_leaf(&self, index: usize) -> bool {
        index
            .checked_sub(1)
            .is_some_and(|prev| self.lines[prev].leaf == self.lines[index].leaf)
    }

    pub fn line_shape_is(&self, index: usize, name: &str) -> bool {
        shape_is(self.lines[index].body, name)
    }

    fn neighbor(&self, index: usize, name: F) -> Option<usize> {
        let line = &self.lines[index];
        let other = match name {
            F::Prev => index.checked_sub(1)?,
            F::Prev2 => index.checked_sub(2)?,
            F::Next => index + 1,
            F::Next2 => index + 2,
            F::LeafHead => {
                let leaf = &self.ranges[line.leaf? as usize];
                leaf.start_line.checked_sub(1)?
            }
            F::LeafNext => match line.leaf.map(|at| self.ranges[at as usize].end_line) {
                Some(last) if last > 0 => last,
                _ => index + 1,
            },
            _ => return None,
        };
        (other < self.lines.len()).then_some(other)
    }

    fn line_field(&self, index: usize, name: F) -> Val<'a> {
        let line = &self.lines[index];
        match name {
            F::Prev | F::Prev2 | F::Next | F::Next2 | F::LeafHead | F::LeafNext => self
                .neighbor(index, name)
                .map_or(Val::Null, |at| Val::Line(at as u32)),
            F::Number => Val::num(index + 1),
            F::Start => Val::num(line.start),
            F::End => Val::num(line.end),
            F::Text => Val::Str(Cow::Borrowed(line.text)),
            F::Body => Val::Str(Cow::Borrowed(line.body)),
            F::BodyStart => Val::num(line.body_start),
            F::BodyEnd => Val::num(line.body_start + line.body.len()),
            F::Shape => Val::Str(Cow::Borrowed(shape(line.body))),
            F::RawBody => Val::Str(Cow::Borrowed(trim_end(text_at(
                self.source,
                line.body_start,
                line.end,
            )))),
            _ => self.line_context(index, name),
        }
    }

    /// The fields of a line that read its leaf, its quotes or its containers.
    fn line_context(&self, index: usize, name: F) -> Val<'a> {
        let line = &self.lines[index];
        let leaf = line.leaf.map(|at| &self.ranges[at as usize]);
        match name {
            F::Blank => Val::Bool(leaf.is_none() && line.body.is_empty()),
            F::Leaf => Val::Str(Cow::Borrowed(line.leaf_kind)),
            F::LeafId => Val::Id(line.leaf),
            F::LeafFirst => Val::Bool(leaf.is_some_and(|range| range.start_line == index + 1)),
            F::LeafStart => Val::num(leaf.map_or(0, |range| range.start)),
            F::Ancestors => Val::Ancestors(line.leaf),
            F::QuotePrefix => {
                Val::Bool(text_at(self.source, line.start, line.body_start).contains('>'))
            }
            F::QuoteDepth => Val::num(line.quote_depth),
            F::QuoteMarks => Val::num(line.quote_marks),
            F::Lazy => Val::Bool(self.lazy(index)),
            F::AttachedHere => Val::Bool(line.attached),
            F::ClosesContainer => Val::Bool(line.closes),
            F::OpenColon => Val::Colons(index as u32),
            _ => Val::Null,
        }
    }

    fn owner_field(&self, holder: Option<u32>, name: F) -> Val<'a> {
        match name {
            F::Type => Val::Str(Cow::Borrowed(self.holder_kind(holder))),
            F::ContentColumn => Val::num(self.owner(holder).0),
            F::MarkerColumn => self.owner(holder).1.map_or(Val::Null, Val::num),
            _ => Val::Null,
        }
    }

    fn colon_field(&self, at: u32, name: F) -> Val<'a> {
        let colon = &self.colons[at as usize];
        match name {
            F::Width => Val::num(colon.width),
            F::Line => Val::num(colon.line),
            F::Column => Val::num(colon.column),
            F::Kind => Val::Str(Cow::Borrowed(colon.kind)),
            F::Start => Val::num(colon.from),
            _ => Val::Null,
        }
    }

    pub fn field(&self, value: &Val<'a>, name: F) -> Val<'a> {
        match value {
            Val::Line(at) => self.line_field(*at as usize, name),
            Val::Owner(holder) => self.owner_field(*holder, name),
            Val::Colon(at) => self.colon_field(*at, name),
            _ => Val::Null,
        }
    }

    pub fn subject_field(&self, subject: &Subject<'_, 'a>, name: F) -> Val<'a> {
        match subject {
            Subject::Line(at) => self.line_field(*at, name),
            Subject::Block(block) => self.block_field(block, name),
            Subject::Inline(inline) => match name {
                F::Type => Val::Str(Cow::Borrowed(inline.kind)),
                F::Start => Val::num(inline.start),
                F::End => Val::num(inline.end),
                F::Text => Val::Str(Cow::Borrowed(inline.text)),
                F::Leaf => Val::Str(Cow::Borrowed(inline.leaf)),
                F::Parent => Val::Str(Cow::Borrowed(inline.parent)),
                _ => Val::Null,
            },
        }
    }

    fn colon_ids(&self, line: u32) -> &[u32] {
        let (from, to) = self.lines[line as usize].colons;
        &self.line_colons[from as usize..to as usize]
    }

    pub fn list_len(&self, list: &Val<'a>) -> usize {
        match list {
            Val::Colons(line) => self.colon_ids(*line).len(),
            Val::Ancestors(leaf) => self.ancestors(*leaf).count(),
            _ => 0,
        }
    }

    pub fn list_item(&self, list: &Val<'a>, index: usize) -> Val<'a> {
        match list {
            Val::Colons(line) => self
                .colon_ids(*line)
                .get(index)
                .map_or(Val::Null, |at| Val::Colon(*at)),
            Val::Ancestors(leaf) => self
                .ancestors(*leaf)
                .nth(index)
                .map_or(Val::Null, |kind| Val::Str(Cow::Borrowed(kind))),
            _ => Val::Null,
        }
    }
}

impl<'a> Records<'a> {
    fn block_column(&self, block: &BlockRec<'a>) -> usize {
        *block.column.get_or_init(|| {
            if block.item_child {
                column_of(
                    self.positions,
                    self.ranges[block.range as usize].start,
                    block.quotes,
                )
            } else {
                0
            }
        })
    }

    fn block_mark_column(&self, block: &BlockRec<'a>) -> usize {
        *block.mark_column.get_or_init(|| {
            if block.opener_rule || block.item_child {
                column_of(self.positions, block.opener_start, block.quotes)
            } else {
                0
            }
        })
    }

    fn prev_type(&self, block: &BlockRec<'a>) -> Option<&'a str> {
        let range = &self.ranges[block.range as usize];
        if block.kind != "paragraph" || range.index == 0 {
            return None;
        }
        range.prev.map(|sibling| self.kind(sibling))
    }

    fn block_field(&self, block: &BlockRec<'a>, name: F) -> Val<'a> {
        let range = &self.ranges[block.range as usize];
        let source = self.source;
        match name {
            F::Type => Val::Str(Cow::Borrowed(block.kind)),
            F::Start => Val::num(range.start),
            F::End => Val::num(range.end),
            F::Text => Val::Str(Cow::Borrowed(
                if matches!(block.kind, "line" | "list" | "frontmatter") {
                    text_at(source, range.start, range.end)
                } else {
                    ""
                },
            )),
            F::Column => Val::num(self.block_column(block)),
            F::OpenerColumn => Val::num(if range.has_attrs {
                self.block_column(block)
            } else {
                self.block_mark_column(block)
            }),
            F::MarkColumn => Val::num(self.block_mark_column(block)),
            _ => self.block_place(block, name),
        }
    }

    /// The fields of a block that place it among its lines, siblings and containers.
    fn block_place(&self, block: &BlockRec<'a>, name: F) -> Val<'a> {
        let range = &self.ranges[block.range as usize];
        let source = self.source;
        match name {
            F::OpenerStart => Val::num(block.opener_start),
            F::OpenerText => Val::Str(Cow::Borrowed(block_opener_text(
                source,
                block.opener_start,
                block.kind,
                block.opener_rule,
            ))),
            F::AfterQuoteMark => Val::Bool(
                text_at(source, self.lines[block.line_index].start, range.start).ends_with("> "),
            ),
            F::Attributed => Val::Bool(range.has_attrs),
            F::ImageLine => Val::Bool(range.image_line),
            F::Index => Val::num(range.index as usize),
            F::Line => Val::Line(block.line_index as u32),
            F::LastLine => Val::Line(block.last_line_index as u32),
            F::PrevType => self
                .prev_type(block)
                .map_or(Val::Null, |kind| Val::Str(Cow::Borrowed(kind))),
            F::Owner => Val::Owner(block.owner),
            F::ItemOwner => Val::Owner(block.item_owner),
            F::Parent => Val::Owner(block.parent),
            _ => Val::Null,
        }
    }
}

struct LeafIndex {
    leaves: Vec<Option<u32>>,
    attached: Vec<bool>,
}

impl<'a> Records<'a> {
    fn leaf_index(&self, count: usize) -> LeafIndex {
        let mut leaves: Vec<Option<u32>> = vec![None; count];
        let mut attached = vec![false; count];
        for (at, range) in self.ranges.iter().enumerate() {
            if self.inline_held(range) {
                continue;
            }
            if range.has_attrs {
                if let Some(slot) = attached.get_mut(range.start_line.saturating_sub(1)) {
                    *slot = true;
                }
            }
            if !LINE_LEAVES.contains(&range.kind) || (range.kind == "comment" && !range.form_block)
            {
                continue;
            }
            let first = range.start_line.saturating_sub(1);
            for place in leaves
                .get_mut(first..range.end_line.min(count))
                .into_iter()
                .flatten()
            {
                if place
                    .is_none_or(|known| self.ranges[known as usize].path.len() <= range.path.len())
                {
                    *place = Some(at as u32);
                }
            }
        }
        LeafIndex { leaves, attached }
    }

    /// The containers that hold each line, for the owner of a colon fence.
    fn boundaries(&self, count: usize) -> Vec<Vec<u32>> {
        let mut boundaries = vec![Vec::new(); count];
        for (at, range) in self.ranges.iter().enumerate() {
            if self.inline_held(range)
                || !matches!(
                    range.kind,
                    "block_quote" | "list_item" | "footnote" | "definition_description"
                )
            {
                continue;
            }
            let first = range.start_line.saturating_sub(1);
            for owners in boundaries
                .get_mut(first..range.end_line.min(count))
                .into_iter()
                .flatten()
            {
                owners.push(at as u32);
            }
        }
        boundaries
    }

    fn boundary_at(&self, at: usize, boundaries: &[Vec<u32>]) -> Option<u32> {
        boundaries
            .get(self.positions.line_of(at))
            .into_iter()
            .flatten()
            .copied()
            .filter(|range| {
                let range = &self.ranges[*range as usize];
                range.start <= at && at < range.end
            })
            .max_by_key(|range| self.ranges[*range as usize].path.len())
    }

    fn colon_layouts(&self, boundaries: &[Vec<u32>]) -> Vec<Colon> {
        let source = self.source;
        let positions = self.positions;
        let colon_kind = |kind: &str| {
            matches!(
                kind,
                "div" | "directive" | "admonition" | "figure_group" | "line_block"
            )
        };
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (at, range) in self.ranges.iter().enumerate() {
            if let Some(parent) = range
                .holder
                .and_then(|holder| self.holders[holder as usize].range)
                .filter(|parent| colon_kind(self.kind(*parent)))
            {
                children.entry(parent).or_default().push(at as u32);
            }
        }
        let mut colons = Vec::new();
        for (at, range) in self.ranges.iter().enumerate() {
            if !colon_kind(range.kind) {
                continue;
            }
            let from = opener_start(source, range.start, usize::MAX);
            let width = source
                .as_bytes()
                .get(from..)
                .unwrap_or(&[])
                .iter()
                .take_while(|b| **b == b':')
                .count();
            if width < 3 {
                continue;
            }
            let kind = match range.kind {
                "admonition" => "admonition",
                "figure_group" => "figure group",
                "line_block" => "line block",
                _ if source
                    .get(from + width..)
                    .is_some_and(|s| s.starts_with(" \\")) =>
                {
                    "hard-break block"
                }
                _ => "div",
            };
            let (line, column) = positions.line_col(from);
            let final_index = positions.line_of(range.end.saturating_sub(1));
            let final_line = positions.line(final_index);
            let closer = trim_end(final_line.trim_start_matches([' ', '\t', '>']));
            let closer_start = positions.line_range(final_index).0
                + final_line
                    .len()
                    .saturating_sub(final_line.trim_start_matches([' ', '\t', '>']).len());
            let closer_owned_by_child = children.get(&(at as u32)).is_some_and(|direct| {
                direct.iter().any(|child| {
                    let child = &self.ranges[*child as usize];
                    child.start <= closer_start && child.end >= closer_start + closer.len()
                })
            });
            colons.push(Colon {
                from,
                end: range.end,
                closed: !closer_owned_by_child
                    && closer.len() >= width
                    && closer.bytes().all(|byte| byte == b':'),
                owner: self.boundary_at(from, boundaries),
                width,
                line,
                column,
                kind,
            });
        }
        colons.sort_by_key(|colon| colon.from);
        colons
    }

    fn build_lines(&mut self, model: &'a [Line]) {
        let source = self.source;
        let model: Vec<&'a Line> = model
            .iter()
            .filter(|line| line.start < source.len() || !source.ends_with('\n'))
            .collect();
        let count = model.len();
        let LeafIndex { leaves, attached } = self.leaf_index(count);
        let has_colons = self.ranges.iter().any(|range| {
            matches!(
                range.kind,
                "div" | "directive" | "admonition" | "figure_group" | "line_block"
            )
        });
        let boundaries = if has_colons {
            self.boundaries(count)
        } else {
            Vec::new()
        };
        self.colons = if has_colons {
            self.colon_layouts(&boundaries)
        } else {
            Vec::new()
        };
        let mut next_colon = 0;
        let mut active: Vec<u32> = Vec::new();
        let mut lines = Vec::with_capacity(count);
        for (index, line) in model.into_iter().enumerate() {
            let leaf = leaves[index];
            let leaf_range = leaf.map(|at| &self.ranges[at as usize]);
            let leaf_kind = leaf_range.map_or("none", |range| {
                if range.kind == "comment" && range.form_block {
                    "comment_block"
                } else {
                    range.kind
                }
            });
            let raw = text_at(source, line.start, line.start + line.text.len());
            let line_text = if verbatim(leaf_kind) {
                raw
            } else {
                line.escaped.as_deref().unwrap_or(&line.text)
            };
            let quote_depth = leaf_range.map_or(0, |range| self.quotes_at(range.holder));
            let body_start = line.content.map_or_else(
                || {
                    line.start
                        + quoted_start(
                            line_text,
                            if verbatim(leaf_kind) {
                                quote_depth
                            } else {
                                usize::MAX
                            },
                        )
                },
                |at| line.start + at,
            );
            let from = body_start.saturating_sub(line.start).min(line_text.len());
            let body = trim_end(line_text.get(from..).unwrap_or(""));
            let first_colon = self.line_colons.len() as u32;
            if has_colons {
                self.open_colons(
                    &boundaries,
                    &mut active,
                    &mut next_colon,
                    body_start,
                    line.start,
                );
            }
            let colons = (first_colon, self.line_colons.len() as u32);
            let closes = body.len() >= 3
                && body.bytes().all(|byte| byte == b':')
                && self.line_colons[colons.0 as usize..colons.1 as usize]
                    .first()
                    .is_some_and(|at| body.len() >= self.colons[*at as usize].width);
            lines.push(LineRec {
                start: line.start,
                end: line.start + raw.len(),
                text: line_text,
                body,
                body_start,
                leaf,
                leaf_kind,
                quote_depth,
                quote_marks: quote_marks(line_text),
                attached: attached[index],
                closes,
                colons,
                path: line.path.as_str(),
            });
        }
        self.lines = lines;
    }

    fn open_colons(
        &mut self,
        boundaries: &[Vec<u32>],
        active: &mut Vec<u32>,
        next: &mut usize,
        body_start: usize,
        line_start: usize,
    ) {
        while self
            .colons
            .get(*next)
            .is_some_and(|colon| colon.from < body_start)
        {
            active.push(*next as u32);
            *next += 1;
        }
        let source = self.source;
        let colons = &self.colons;
        active.retain(|at| {
            let colon = &colons[*at as usize];
            if colon.closed && colon.end < line_start {
                return false;
            }
            if colon.end >= line_start.saturating_sub(1) {
                return true;
            }
            source.get(colon.end..line_start).is_some_and(|gap| {
                gap.bytes()
                    .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'>'))
            })
        });
        if active.is_empty() {
            return;
        }
        let owner = self.boundary_at(body_start, boundaries);
        for at in active.iter().rev() {
            if self.colons[*at as usize].owner == owner {
                self.line_colons.push(*at);
            }
        }
    }

    /// The record of a node, or `Skip` when no rule can select it; `block` and `inline` say whether a
    /// rule can select that node type, given its parent type for a block.
    pub fn node(
        &self,
        at: usize,
        block: &mut dyn FnMut(&'a str, &'a str) -> bool,
        inline: &mut dyn FnMut(&'a str, &'a str) -> bool,
    ) -> Node<'a> {
        let range = &self.ranges[at];
        let line_index = range.start_line.saturating_sub(1);
        if line_index >= self.lines.len() {
            return Node::Skip;
        }
        let kind = range.kind;
        if self.inline_held(range) {
            let text = text_at(self.source, range.start, range.end);
            if !inline(kind, text)
                || !((kind == "comment" && !range.form_block) || INLINE_KINDS.contains(&kind))
            {
                return Node::Skip;
            }
            let mut holder = range.holder;
            let mut leaf = None;
            while let Some(up) = holder {
                let node = &self.holders[up as usize];
                if let Some(kind) = node
                    .range
                    .map(|range| self.kind(range))
                    .filter(|kind| TEXT_LEAVES.contains(kind))
                {
                    leaf = Some(kind);
                    break;
                }
                holder = node.up;
            }
            let Some(leaf) = leaf else {
                return Node::Skip;
            };
            return Node::Inline(InlineRec {
                range: at as u32,
                kind,
                line_index,
                start: range.start,
                end: range.end,
                text,
                leaf,
                parent: self.holder_kind(range.holder),
            });
        }
        if matches!(kind, "soft_break" | "hard_break" | "line_break") {
            return Node::Skip;
        }
        let parent_kind = self.holder_kind(range.holder);
        if !block(kind, parent_kind) {
            return Node::Skip;
        }
        Node::Block(self.block_record(at, line_index, parent_kind))
    }

    fn block_record(&self, at: usize, line_index: usize, parent_kind: &'a str) -> BlockRec<'a> {
        let range = &self.ranges[at];
        let kind = range.kind;
        let mut owner = range.holder;
        while self
            .holder_range(owner)
            .is_some_and(|r| self.kind(r) == "definition_list")
        {
            owner = self.up(owner);
        }
        let mut item_owner = owner;
        while self.holder_range(item_owner).is_some_and(|r| {
            matches!(
                self.kind(r),
                "admonition" | "directive" | "div" | "figure_group"
            )
        }) {
            item_owner = self.up(item_owner);
        }
        let item_owner = item_owner.filter(|_| {
            self.holder_range(item_owner)
                .is_some_and(|r| self.kind(r) == "list_item")
        });
        let quotes = self.quotes_at(range.holder);
        BlockRec {
            range: at as u32,
            kind,
            line_index,
            last_line_index: range
                .end_line
                .saturating_sub(1)
                .min(self.lines.len().saturating_sub(1)),
            quotes,
            item_child: parent_kind == "list_item" || item_owner.is_some(),
            opener_rule: OPENER_RULE_KINDS.contains(&kind),
            opener_start: if range.has_attrs {
                opener_start(self.source, range.start, quotes)
            } else {
                range.start
            },
            column: OnceCell::new(),
            mark_column: OnceCell::new(),
            parent: range.holder,
            owner,
            item_owner,
        }
    }
}
