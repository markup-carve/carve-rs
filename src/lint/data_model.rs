//! Source and tree records consumed by the pinned 0.2 lint model, borrowed from the parse and the line model.
#![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

use super::data_interpreter::{F, LookupIndex, LookupKey, LookupOver, Seg, Val};
use super::{Holder, Range, lines::Line};
use crate::source::Source;
use std::borrow::Cow;
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;

fn text_at(source: &str, from: usize, to: usize) -> &str {
    source
        .get(from.min(source.len())..to.min(source.len()))
        .unwrap_or("")
}

fn trim_end(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

pub(super) fn quoted_start_pub(text: &str, marks: usize) -> usize {
    quoted_start(text, marks)
}

/// The end of up to `marks` quote marks, each with the blanks before it and one blank after it: the
/// prefix that C2-B-027 leaves outside a span that crosses into the line.
pub(super) fn quote_prefix_end(text: &str, marks: usize) -> usize {
    let bytes = text.as_bytes();
    let blank = |at: usize| bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t'));
    let mut at = 0;
    for _ in 0..marks {
        while blank(at) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'>') {
            break;
        }
        at += 1;
        if blank(at) {
            at += 1;
        }
    }
    at
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

/// Whether a holder of this type is a block quote, and whether it holds inline nodes.
fn holder_flags(kind: &str) -> (bool, bool) {
    match kind {
        "block_quote" => (true, false),
        "paragraph" | "caption" | "definition_term" | "heading" | "line" | "cell_paragraph"
        | "code_block" | "raw_block" | "frontmatter" | "comment" | "include" => (false, true),
        _ => (false, false),
    }
}
fn text_leaf(kind: &str) -> bool {
    matches!(
        kind,
        "paragraph" | "caption" | "definition_term" | "heading" | "line" | "table_cell"
    )
}
fn line_leaf(kind: &str) -> bool {
    matches!(
        kind,
        "paragraph"
            | "caption"
            | "definition_term"
            | "heading"
            | "line"
            | "table_cell"
            | "table_row"
            | "code_block"
            | "raw_block"
            | "frontmatter"
            | "comment"
    )
}
fn inline_kind(kind: &str) -> bool {
    matches!(
        kind,
        "text"
            | "image"
            | "emphasis"
            | "strong"
            | "highlight"
            | "strike"
            | "link"
            | "raw_inline"
            | "code"
            | "span"
            | "subscript"
            | "superscript"
            | "autolink"
            | "reference"
            | "footnote_ref"
            | "citation"
            | "wikilink"
            | "xref"
    )
}
fn opener_rule_kind(kind: &str) -> bool {
    matches!(
        kind,
        "admonition"
            | "block_quote"
            | "code_block"
            | "definition_term"
            | "directive"
            | "div"
            | "figure_group"
            | "heading"
            | "line_block"
            | "raw_block"
            | "table"
            | "thematic_break"
    )
}

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

impl InlineRec<'_> {
    pub fn first_byte(&self) -> Option<u8> {
        self.text.as_bytes().first().copied()
    }
}

/// The record a rule runs over.
pub(super) enum Subject<'r, 'a> {
    Line(usize),
    Block(&'r BlockRec<'a>),
    Inline(&'r InlineRec<'a>),
    /// An attribute record, by index into the attribute table.
    Attribute(u32),
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
    /// The node of each range a `node` field can read, cut at the depth its rules read.
    nodes: &'a [Option<serde_json::Value>],
    /// The lookups of this call, by the address of their compiled expression.
    lookups: RefCell<HashMap<usize, LookupIndex>>,
    /// The attribute records of this call (C2-W-148).
    pub attributes: super::data_attrs::Table,
    tree: TreeFacts<'a>,
    extensions: Vec<String>,
    layout: OnceCell<Layout>,
    tables: OnceCell<TableLines>,
}

/// An id record (C2-W-150).
struct IdRec<'a> {
    id: Cow<'a, str>,
    owner: &'a str,
    start: usize,
}

/// What the records read from the tree beyond the ranges: the id records, the links and images with a key,
/// and the document for the source layout.
pub(super) struct TreeFacts<'a> {
    ids: Vec<IdRec<'a>>,
    keyed: std::collections::HashSet<u32>,
    document: Option<Box<dyn Fn() -> serde_json::Value + 'a>>,
}

/// The generated id of each heading of a tree, by the start of the heading.
pub(super) fn heading_ids(node: crate::view::V<'_>, out: &mut HashMap<usize, String>) {
    if node.is_array() {
        if let Some(items) = node.as_array() {
            for item in items.iter() {
                heading_ids(item, out);
            }
        }
        return;
    }
    if !node.is_object() {
        return;
    }
    if node.ty().as_str() == Some("heading")
        && let (Some((start, _)), Some(id)) = (node.offsets(), node.at("generatedId").as_str())
    {
        out.insert(start, id.to_owned());
    }
    node.each(&mut |key, value| {
        if key != "pos" && value.is_container() {
            heading_ids(value, out);
        }
    });
}

impl<'a> TreeFacts<'a> {
    /// `generated`: the heading ids of a parse with other heading id options, by the start of the heading.
    pub fn read<'v: 'a>(
        ranges: &[Range<'a>],
        values: &[crate::view::V<'v>],
        document: Option<crate::view::V<'v>>,
        generated: Option<&HashMap<usize, String>>,
    ) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut ids = Vec::new();
        let mut keyed = std::collections::HashSet::new();
        for (at, (range, value)) in ranges.iter().zip(values).enumerate() {
            if matches!(range.kind, "link" | "image") && value.get("key").is_some() {
                keyed.insert(at as u32);
            }
            let authored = value
                .get("attrs")
                .and_then(|attrs| attrs.get("id"))
                .and_then(crate::view::V::as_str)
                .map(Cow::Borrowed);
            let heading = (range.kind == "heading").then(|| match generated {
                Some(generated) => generated.get(&range.start).cloned().map(Cow::Owned),
                None => value
                    .get("generatedId")
                    .and_then(crate::view::V::as_str)
                    .map(Cow::Borrowed),
            });
            for id in [authored, heading.flatten()].into_iter().flatten() {
                if seen.insert(id.clone()) {
                    ids.push(IdRec {
                        id,
                        owner: range.kind,
                        start: range.start,
                    });
                }
            }
        }
        let document = document.map(|document| {
            Box::new(move || document.to_value()) as Box<dyn Fn() -> serde_json::Value + 'a>
        });
        Self {
            ids,
            keyed,
            document,
        }
    }
}

/// The source layout entries of the document (C2-L-009), with the entry of each path.
struct Layout {
    entries: Vec<serde_json::Value>,
    by_path: HashMap<String, u32>,
}

/// For each line, the nearest line above and below it in the source of its pipe table whose raw body does not
/// start with `%%`, and the range of that table (C2-W-149).
struct TableLines {
    above: Vec<Option<u32>>,
    below: Vec<Option<u32>>,
    table: Vec<Option<u32>>,
}

impl<'a> Records<'a> {
    pub fn new(
        source: &'a str,
        positions: &'a Source<'a>,
        ranges: &'a [Range<'a>],
        holders: &'a [Holder],
        nodes: &'a [Option<serde_json::Value>],
        model: &'a [Line],
        tree: TreeFacts<'a>,
        extensions: Vec<String>,
    ) -> Self {
        let mut holder_quotes = Vec::with_capacity(holders.len());
        let mut holder_inline = Vec::with_capacity(holders.len());
        for holder in holders {
            let (quote, inline) = holder
                .range
                .map_or((false, false), |at| holder_flags(ranges[at as usize].kind));
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
            nodes,
            lookups: RefCell::new(HashMap::new()),
            attributes: super::data_attrs::Table::default(),
            tree,
            extensions,
            layout: OnceCell::new(),
            tables: OnceCell::new(),
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
        // A caption holds its own text, so the caption is the first block of the line.
        let caption = leaf
            .filter(|&at| self.ranges[at as usize].kind == "caption")
            .map(|_| "caption");
        caption.into_iter().chain(std::iter::from_fn(move || {
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
        }))
    }

    /// The ranges of the blocks that hold a range, innermost first, the document left out.
    fn holder_chain(&self, at: u32) -> impl Iterator<Item = u32> + '_ {
        let mut holder = self.ranges[at as usize].holder;
        std::iter::from_fn(move || {
            while let Some(up) = holder {
                let node = &self.holders[up as usize];
                holder = node.up;
                if node.range.is_some() {
                    return node.range;
                }
            }
            None
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
        let (start, first) = if kind == "footnote" {
            footnote_marker(source, range.start, range.start_line.saturating_sub(1))
        } else {
            (range.start, range.start_line.saturating_sub(1))
        };
        let marker_at = start - source.line_range(first).0;
        let suffix = source.line(first).get(marker_at..).unwrap_or("");
        let marker_len = match kind {
            "definition_description" => 1,
            "footnote" => suffix.find(':').map_or(0, |i| i + 1),
            _ if suffix.starts_with(['-', '*', '.']) => 1,
            _ => suffix.find(['.', ')']).map_or(0, |i| i + 1),
        };
        let bytes = self.source.as_bytes();
        let mut at = start + marker_len;
        // C2-B-101: only brace blocks glued to an item marker are its attributes.
        if kind == "list_item" {
            while self.source.get(at..).is_some_and(|s| s.starts_with('{')) {
                let Some(close) = self.source[at..].find(['}', '\n']) else {
                    break;
                };
                if bytes[at + close] != b'}' {
                    break;
                }
                at += close + 1;
            }
        }
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        (column_of(source, at, quotes), marker)
    }

    fn lazy(&self, index: usize) -> bool {
        let line = &self.lines[index];
        let Some(leaf) = line.leaf else {
            return false;
        };
        let range = &self.ranges[leaf as usize];
        let can_be_lazy = text_leaf(line.leaf_kind) && range.start_line != index + 1;
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

    /// `leaf_first` of a line.
    pub fn leaf_first(&self, index: usize) -> bool {
        self.lines[index]
            .leaf
            .is_some_and(|leaf| self.ranges[leaf as usize].start_line == index + 1)
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
            F::AncestorIds => Val::LineAncestorIds(line.leaf),
            F::TableAbove => self.table_lines().above[index].map_or(Val::Null, Val::Line),
            F::TableBelow => self.table_lines().below[index].map_or(Val::Null, Val::Line),
            _ => Val::Null,
        }
    }

    fn table_lines(&self) -> &TableLines {
        self.tables.get_or_init(|| {
            let count = self.lines.len();
            let mut lines = TableLines {
                above: vec![None; count],
                below: vec![None; count],
                table: vec![None; count],
            };
            let open = |index: usize| {
                let line = &self.lines[index];
                !trim_end(text_at(self.source, line.body_start, line.end)).starts_with("%%")
            };
            for (at, range) in self.ranges.iter().enumerate() {
                if range.kind != "table"
                    || range.start_line == 0
                    || self.source_format(range) != "pipe"
                {
                    continue;
                }
                let first = range.start_line - 1;
                let last = range.end_line.max(range.start_line).min(count);
                let mut seen = None;
                for index in first..last {
                    lines.above[index] = seen;
                    if open(index) {
                        seen = Some(index as u32);
                    }
                    lines.table[index].get_or_insert(at as u32);
                }
                seen = None;
                for index in (first..last).rev() {
                    lines.below[index] = seen;
                    if open(index) {
                        seen = Some(index as u32);
                    }
                }
            }
            lines
        })
    }

    /// The pipe table whose source holds a line, which names the node of a finding on it (C2-W-138).
    pub fn line_table(&self, index: usize) -> Option<u32> {
        self.table_lines().table.get(index).copied().flatten()
    }

    fn source_format(&self, range: &Range<'_>) -> &'static str {
        crate::api::layout::table_source_format(self.source, range.start, range.end)
    }

    /// The source format of a table, or of the table that holds a row or cell.
    fn table_format(&self, at: u32) -> Val<'a> {
        let table = match self.kind(at) {
            "table" => Some(at),
            "table_row" | "table_cell" => {
                self.holder_chain(at).find(|&up| self.kind(up) == "table")
            }
            _ => None,
        };
        table.map_or(Val::Null, |table| {
            Val::Str(Cow::Borrowed(
                self.source_format(&self.ranges[table as usize]),
            ))
        })
    }

    /// The row of a row or cell record.
    fn row_of(&self, at: u32) -> Option<u32> {
        match self.kind(at) {
            "table_row" => Some(at),
            "table_cell" => self.holder_range(self.ranges[at as usize].holder),
            _ => None,
        }
    }

    /// The group that holds the row of a row or cell record.
    fn group_of(&self, at: u32) -> Val<'a> {
        self.row_of(at)
            .map(|row| self.holder_kind(self.ranges[row as usize].holder))
            .filter(|kind| matches!(*kind, "table_head" | "table_body" | "table_foot"))
            .map_or(Val::Null, |kind| Val::Str(Cow::Borrowed(kind)))
    }

    /// The position of the row of a row or cell record among the rows of its group: a comment is no row.
    fn row_index(&self, at: u32) -> Val<'a> {
        let Some(row) = self.row_of(at) else {
            return Val::Null;
        };
        let mut count = 0;
        let mut prev = self.ranges[row as usize].prev;
        while let Some(sibling) = prev {
            count += usize::from(self.kind(sibling) == "table_row");
            prev = self.ranges[sibling as usize].prev;
        }
        Val::num(count)
    }

    fn id_records(&self) -> &[IdRec<'a>] {
        &self.tree.ids
    }

    fn id_field(&self, at: u32, name: F) -> Val<'a> {
        let record = &self.id_records()[at as usize];
        match name {
            F::Id => Val::Str(record.id.clone()),
            F::Owner => Val::Str(Cow::Borrowed(record.owner)),
            F::Index => Val::num(at as usize),
            F::Start => Val::num(record.start),
            F::Type => Val::Str(Cow::Borrowed("id")),
            _ => Val::Null,
        }
    }

    fn layout(&self) -> &Layout {
        self.layout.get_or_init(|| {
            let entries = self
                .tree
                .document
                .as_ref()
                .map(|document| crate::api::layout::entries(self.source, &document()))
                .unwrap_or_default();
            let mut by_path = HashMap::with_capacity(entries.len());
            for (at, entry) in entries.iter().enumerate() {
                if let Some(path) = entry.get("path").and_then(serde_json::Value::as_str) {
                    by_path.entry(path.to_owned()).or_insert(at as u32);
                }
            }
            Layout { entries, by_path }
        })
    }

    fn layout_entry(&self, range: u32) -> Val<'a> {
        let path = self.ranges[range as usize].path.as_str();
        self.layout()
            .by_path
            .get(path)
            .map_or(Val::Null, |at| Val::Layout(*at))
    }

    /// A fact of a layout entry by name; absent where the entry does not have it (C2-L-014).
    fn layout_key(&self, at: u32, key: &str) -> Val<'a> {
        match self.layout().entries[at as usize].get(key) {
            Some(serde_json::Value::Number(n)) => Val::Num(n.as_i64().unwrap_or(0)),
            Some(serde_json::Value::String(text)) => Val::Str(Cow::Owned(text.clone())),
            Some(serde_json::Value::Bool(b)) => Val::Bool(*b),
            _ => Val::Null,
        }
    }

    /// `raw_ref` of a link or an image with a key, otherwise its text (C2-W-150).
    fn raw_ref(&self, at: u32) -> Val<'a> {
        let range = &self.ranges[at as usize];
        let keyed = self.tree.keyed.contains(&at);
        if !keyed || range.start >= range.end || range.end > self.source.len() {
            return Val::Str(Cow::Borrowed(text_at(self.source, range.start, range.end)));
        }
        Val::Str(Cow::Owned(crate::api::layout::raw_ref_text(
            self.source,
            range.kind,
            range.start,
            range.end,
            self.quotes_at(range.holder),
        )))
    }

    pub fn column_at(&self, at: usize, quotes: usize) -> usize {
        column_of(self.positions, at, quotes)
    }

    pub fn extensions(&self) -> Val<'a> {
        Val::List(
            self.extensions
                .iter()
                .map(|name| Val::Str(Cow::Owned(name.clone())))
                .collect(),
        )
    }

    /// One step of a dotted path from a value.
    pub fn step(&self, found: &Val<'a>, seg: &Seg) -> Val<'a> {
        match seg {
            Seg::Field(name) => self.field(found, *name),
            Seg::At(index) => self.list_item(found, *index),
            Seg::AtEnd(back) => self
                .list_len(found)
                .checked_sub(*back)
                .map_or(Val::Null, |at| self.list_item(found, at)),
            Seg::Key(key) => self.node_key(found, key),
            Seg::Item => Val::Null,
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

    /// Builds the attribute records of the nodes that carry `attrs`.
    pub fn read_attributes(&mut self, values: &[crate::view::V<'_>]) {
        let nodes = self
            .ranges
            .iter()
            .zip(values)
            .enumerate()
            // Consumed keys leave no `attrs` in the tree, so a block is read where its source opens
            // with a brace, an item or cell at its marker, and an inline node where it ends in one.
            .filter(|(_, (range, _))| {
                let bytes = self.source.as_bytes();
                if range.kind == "document" || range.end <= range.start {
                    false
                } else if self.inline_held(range) {
                    bytes.get(range.end - 1) == Some(&b'}')
                } else {
                    matches!(range.kind, "list_item" | "table_cell")
                        || bytes.get(range.start) == Some(&b'{')
                }
            })
            .map(|(at, (range, value))| super::data_attrs::Node {
                range: at as u32,
                kind: range.kind,
                value: *value,
                start: range.start,
                end: range.end,
                inline: self.inline_held(range),
                lines: (range.start_line.saturating_sub(1)..range.end_line.max(range.start_line))
                    .map(|line| self.positions.line_range(line))
                    .collect(),
                quotes: self.quotes_at(range.holder),
            });
        self.attributes = super::data_attrs::build(self.source, nodes);
    }

    fn attribute_field(&self, at: u32, name: F) -> Val<'a> {
        let part = &self.attributes.parts[at as usize];
        let text = |value: &str| Val::Str(Cow::Owned(value.to_owned()));
        match name {
            F::Kind => Val::Str(Cow::Borrowed(part.kind)),
            F::Key => text(&part.key),
            F::Value => text(&part.value),
            F::Start => Val::num(part.start),
            F::End => Val::num(part.end),
            F::KeyEnd => Val::num(part.key_end),
            F::Text => Val::Str(Cow::Borrowed(text_at(self.source, part.start, part.end))),
            F::Index => Val::num(part.index),
            F::Owner => Val::AttrOwner(part.owner),
            _ => attribute_fact(part, name),
        }
    }

    fn owner_record_field(&self, at: u32, name: F) -> Val<'a> {
        let owner = &self.attributes.owners[at as usize];
        let range = &self.ranges[owner.range as usize];
        match name {
            F::Type => Val::Str(Cow::Borrowed(
                if range.kind == "comment" && range.form_block {
                    "comment_block"
                } else {
                    range.kind
                },
            )),
            F::Start => Val::num(owner.start),
            F::End => Val::num(owner.end),
            F::OpenerStart => Val::num(owner.opener_start),
            F::Node => self.tree_node(owner.range),
            F::Format => Val::Str(Cow::Borrowed(owner.format)),
            F::Columns => Val::num(owner.columns),
            F::HeadRows => Val::num(owner.rows[0]),
            F::BodyRows => Val::num(owner.rows[1]),
            F::FootRows => Val::num(owner.rows[2]),
            F::AlignOverlap => Val::Str(Cow::Owned(owner.align_overlap.clone())),
            F::ValignOverlap => Val::Str(Cow::Owned(owner.valign_overlap.clone())),
            _ => Val::Null,
        }
    }

    pub fn has_lookup(&self, id: usize) -> bool {
        self.lookups.borrow().contains_key(&id)
    }

    pub fn store_lookup(&self, id: usize, index: LookupIndex) {
        self.lookups.borrow_mut().insert(id, index);
    }

    /// Every record of a key, in document order (C2-W-150).
    pub fn lookup_all(&self, id: usize, key: &LookupKey) -> Vec<u32> {
        self.lookups.borrow()[&id]
            .keys
            .get(key)
            .map(|found| found.iter().map(|(_, record)| *record).collect())
            .unwrap_or_default()
    }

    /// The first record of a key; with `side` -1 the last of them that starts before `start`, with 1 the
    /// first that starts after it (C2-W-141).
    pub fn lookup_at(&self, id: usize, key: &LookupKey, start: usize, side: i8) -> Option<u32> {
        let lookups = self.lookups.borrow();
        let found = lookups[&id].keys.get(key)?;
        let at = match side {
            0 => 0,
            -1 => found
                .partition_point(|(from, _)| *from < start)
                .checked_sub(1)?,
            _ => found.partition_point(|(from, _)| *from <= start),
        };
        found.get(at).map(|(_, record)| *record)
    }

    /// The records a lookup reads in document order, with their starts; a block or inline lookup that
    /// names no types reads every block or every inline node (C2-W-150).
    pub fn lookup_records(&self, over: LookupOver, types: &[&str]) -> Vec<(u32, usize)> {
        match over {
            LookupOver::Line => {
                return (0..self.lines.len())
                    .map(|at| (at as u32, self.lines[at].start))
                    .collect();
            }
            LookupOver::Id => {
                return self
                    .id_records()
                    .iter()
                    .enumerate()
                    .map(|(at, record)| (at as u32, record.start))
                    .collect();
            }
            LookupOver::Block | LookupOver::Inline => {}
        }
        let inline = over == LookupOver::Inline;
        let mut found: Vec<(u32, usize, usize)> = self
            .ranges
            .iter()
            .enumerate()
            .filter(|(_, range)| {
                (if types.is_empty() {
                    range.kind != "document"
                } else {
                    types.contains(&range.kind)
                }) && range.start_line.saturating_sub(1) < self.lines.len()
                    && self.inline_held(range) == inline
            })
            .map(|(at, range)| (at as u32, range.start, range.end))
            .collect();
        found.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)));
        found
            .into_iter()
            .map(|(at, start, _)| (at, start))
            .collect()
    }

    fn record_field(&self, at: u32, name: F) -> Val<'a> {
        let range = &self.ranges[at as usize];
        if self.inline_held(range) {
            return match name {
                F::Type => Val::Str(Cow::Borrowed(range.kind)),
                F::Start => Val::num(range.start),
                F::End => Val::num(range.end),
                F::Text => Val::Str(Cow::Borrowed(text_at(self.source, range.start, range.end))),
                F::Node => self.tree_node(at),
                F::RawRef => self.raw_ref(at),
                _ => Val::Null,
            };
        }
        let line_index = range.start_line.saturating_sub(1);
        let block = self.block_record(at as usize, line_index);
        self.block_field(&block, name)
    }

    pub fn field(&self, value: &Val<'a>, name: F) -> Val<'a> {
        match value {
            Val::Rec(at) => self.record_field(*at, name),
            Val::AttrOwner(at) => self.owner_record_field(*at, name),
            Val::Node(_) => self.node_key(value, super::data_interpreter::field_name(name)),
            Val::Line(at) => self.line_field(*at as usize, name),
            Val::Owner(holder) => self.owner_field(*holder, name),
            Val::Colon(at) => self.colon_field(*at, name),
            Val::IdRec(at) => self.id_field(*at, name),
            Val::Layout(at) => self.layout_key(*at, super::data_interpreter::field_name(name)),
            _ => Val::Null,
        }
    }

    pub fn subject_field(&self, subject: &Subject<'_, 'a>, name: F) -> Val<'a> {
        match subject {
            Subject::Line(at) => self.line_field(*at, name),
            Subject::Block(block) => self.block_field(block, name),
            Subject::Attribute(at) => self.attribute_field(*at, name),
            Subject::Inline(inline) => match name {
                F::Node if inline.kind != "text_run" => self.tree_node(inline.range),
                F::Type => Val::Str(Cow::Borrowed(inline.kind)),
                F::Start => Val::num(inline.start),
                F::End => Val::num(inline.end),
                F::Text => Val::Str(Cow::Borrowed(inline.text)),
                F::Leaf => Val::Str(Cow::Borrowed(inline.leaf)),
                F::Parent => Val::Str(Cow::Borrowed(inline.parent)),
                F::RawRef if inline.kind != "text_run" => self.raw_ref(inline.range),
                _ => Val::Null,
            },
        }
    }

    /// A key of an AST node: a scalar as a value, anything else as a node; absent is null.
    pub fn node_key(&self, value: &Val<'a>, key: &str) -> Val<'a> {
        if let Val::Layout(at) = value {
            return self.layout_key(*at, key);
        }
        let Val::Node(node) = value else {
            return Val::Null;
        };
        node.get(key).map_or(Val::Null, node_value)
    }

    fn tree_node(&self, range: u32) -> Val<'a> {
        self.nodes
            .get(range as usize)
            .and_then(Option::as_ref)
            .map_or(Val::Null, Val::Node)
    }

    fn colon_ids(&self, line: u32) -> &[u32] {
        let (from, to) = self.lines[line as usize].colons;
        &self.line_colons[from as usize..to as usize]
    }

    pub fn list_len(&self, list: &Val<'a>) -> usize {
        match list {
            Val::Colons(line) => self.colon_ids(*line).len(),
            Val::Ancestors(leaf) => self.ancestors(*leaf).count(),
            Val::Node(node) => node.as_array().map_or(0, Vec::len),
            Val::BlockAncestors(at) | Val::AncestorIds(at) => self.holder_chain(*at).count() + 1,
            Val::LineAncestorIds(leaf) => self.ancestors(*leaf).count(),
            Val::List(items) => items.len(),
            _ => 0,
        }
    }

    pub fn is_list(&self, value: &Val<'a>) -> bool {
        match value {
            Val::Node(node) => node.is_array(),
            Val::Colons(_)
            | Val::Ancestors(_)
            | Val::BlockAncestors(_)
            | Val::AncestorIds(_)
            | Val::LineAncestorIds(_)
            | Val::List(_) => true,
            _ => false,
        }
    }

    /// The identities of the blocks of a line's `ancestors`, item by item (C2-W-150).
    fn line_ancestor_ids(&self, leaf: Option<u32>) -> impl Iterator<Item = i64> + '_ {
        let caption = leaf.filter(|&at| self.ranges[at as usize].kind == "caption");
        let chain = leaf.into_iter().flat_map(|at| self.holder_chain(at));
        caption
            .into_iter()
            .chain(chain)
            .map(i64::from)
            .chain(leaf.map(|_| -1))
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
            Val::BlockAncestors(at) => self
                .holder_chain(*at)
                .map(|range| self.kind(range))
                .chain(["document"])
                .nth(index)
                .map_or(Val::Null, |kind| Val::Str(Cow::Borrowed(kind))),
            Val::AncestorIds(at) => self
                .holder_chain(*at)
                .map(i64::from)
                .chain([-1])
                .nth(index)
                .map_or(Val::Null, Val::Num),
            Val::LineAncestorIds(leaf) => self
                .line_ancestor_ids(*leaf)
                .nth(index)
                .map_or(Val::Null, Val::Num),
            Val::List(items) => items.get(index).cloned().unwrap_or(Val::Null),
            Val::Node(node) => node
                .as_array()
                .and_then(|items| items.get(index))
                .map_or(Val::Null, node_value),
            _ => Val::Null,
        }
    }
}

impl<'a> Records<'a> {
    fn block_column(&self, block: &BlockRec<'a>) -> usize {
        *block.column.get_or_init(|| {
            column_of(
                self.positions,
                self.ranges[block.range as usize].start,
                block.quotes,
            )
        })
    }

    fn block_mark_column(&self, block: &BlockRec<'a>) -> usize {
        *block
            .mark_column
            .get_or_init(|| column_of(self.positions, block.opener_start, block.quotes))
    }

    fn prev_type(&self, block: &BlockRec<'a>) -> Option<&'a str> {
        let range = &self.ranges[block.range as usize];
        if range.index == 0 {
            return None;
        }
        range.prev.map(|sibling| self.kind(sibling))
    }

    fn block_field(&self, block: &BlockRec<'a>, name: F) -> Val<'a> {
        let range = &self.ranges[block.range as usize];
        let source = self.source;
        match name {
            F::Node => self.tree_node(block.range),
            F::Ancestors => Val::BlockAncestors(block.range),
            F::AncestorIds => Val::AncestorIds(block.range),
            F::Type => Val::Str(Cow::Borrowed(block.kind)),
            F::Start => Val::num(range.start),
            F::End => Val::num(range.end),
            F::Text => Val::Str(Cow::Borrowed(text_at(source, range.start, range.end))),
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
            // C2-B-031: read before opener_start, which for an attributed block is past its attribute lines.
            F::AfterQuoteMark => {
                let at = block.opener_start.min(source.len());
                let line = source[..at].rfind('\n').map_or(0, |n| n + 1);
                Val::Bool(text_at(source, line, at).ends_with("> "))
            }
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
            F::Identity => Val::num(block.range as usize),
            F::Layout => self.layout_entry(block.range),
            F::SourceFormat => self.table_format(block.range),
            F::Group => self.group_of(block.range),
            F::RowIndex => self.row_index(block.range),
            _ => Val::Null,
        }
    }
}

struct LeafIndex {
    leaves: Vec<Option<u32>>,
    attached: Vec<bool>,
    /// Whether any block can be a colon fence.
    has_colons: bool,
}

impl<'a> Records<'a> {
    fn leaf_index(&self, count: usize) -> LeafIndex {
        let mut leaves: Vec<Option<u32>> = vec![None; count];
        let mut attached = vec![false; count];
        let mut has_colons = false;
        for (at, range) in self.ranges.iter().enumerate() {
            has_colons |= matches!(
                range.kind,
                "div" | "directive" | "admonition" | "figure_group" | "line_block"
            );
            if self.inline_held(range) {
                continue;
            }
            if range.has_attrs {
                if let Some(slot) = attached.get_mut(range.start_line.saturating_sub(1)) {
                    *slot = true;
                }
            }
            // A joined row has no pos, so it is the leaf of no line.
            if range.joined
                || !line_leaf(range.kind)
                || (range.kind == "comment" && !range.form_block)
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
        LeafIndex {
            leaves,
            attached,
            has_colons,
        }
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
        let LeafIndex {
            leaves,
            attached,
            has_colons,
        } = self.leaf_index(count);
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
                || !((kind == "comment" && !range.form_block) || inline_kind(kind))
            {
                return Node::Skip;
            }
            let Some(leaf) = self.inline_leaf(range) else {
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
        Node::Block(self.block_record(at, line_index))
    }

    /// The text leaf that holds an inline node.
    fn inline_leaf(&self, range: &Range) -> Option<&'a str> {
        let mut holder = range.holder;
        while let Some(up) = holder {
            let node = &self.holders[up as usize];
            if let Some(kind) = node
                .range
                .map(|range| self.kind(range))
                .filter(|kind| text_leaf(kind))
            {
                return Some(kind);
            }
            holder = node.up;
        }
        None
    }

    /// The text or soft break sibling right before (`back`) or after a node of those two types. They
    /// have no children, so it is a neighbouring range, whichever order the walk gave siblings.
    fn run_sibling(&self, at: usize, back: bool) -> Option<usize> {
        let range = &self.ranges[at];
        let index = if back {
            range.index.checked_sub(1)?
        } else {
            range.index + 1
        };
        [at.checked_sub(1), Some(at + 1)]
            .into_iter()
            .flatten()
            .find(|&near| {
                self.ranges.get(near).is_some_and(|other| {
                    other.holder == range.holder
                        && other.index == index
                        && matches!(other.kind, "text" | "soft_break")
                })
            })
    }

    /// C2-W-140: the text run that starts at a text or soft break node, the siblings of those two
    /// types next to it read as one record; none where the node continues a run.
    pub fn text_run(&self, at: usize) -> Option<InlineRec<'a>> {
        let range = &self.ranges[at];
        if !matches!(range.kind, "text" | "soft_break")
            || !self.inline_held(range)
            || self.run_sibling(at, true).is_some()
        {
            return None;
        }
        let line_index = range.start_line.saturating_sub(1);
        if line_index >= self.lines.len() {
            return None;
        }
        let mut last = at;
        while let Some(next) = self.run_sibling(last, false) {
            last = next;
        }
        let end = self.ranges[last].end;
        Some(InlineRec {
            range: at as u32,
            kind: "text_run",
            line_index,
            start: range.start,
            end,
            text: text_at(self.source, range.start, end),
            leaf: self.inline_leaf(range)?,
            parent: self.holder_kind(range.holder),
        })
    }

    fn block_record(&self, at: usize, line_index: usize) -> BlockRec<'a> {
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
            opener_rule: opener_rule_kind(kind),
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

/// The start of a footnote's marker and its line: its range starts at its attribute lines
/// (C2-B-090), so the marker stands on the first line below them.
fn footnote_marker(source: &Source<'_>, start: usize, line: usize) -> (usize, usize) {
    let mut at = (start, line);
    while source
        .text()
        .get(at.0..)
        .is_some_and(|rest| rest.starts_with('{'))
        && at.1 + 1 < source.line_count()
    {
        let (from, to) = source.line_range(at.1 + 1);
        let Some(mark) = source.line(at.1 + 1).find(['[', '{']) else {
            break;
        };
        at = ((from + mark).min(to), at.1 + 1);
    }
    at
}

fn node_value(value: &serde_json::Value) -> Val<'_> {
    match value {
        serde_json::Value::Null => Val::Null,
        serde_json::Value::String(text) => Val::Str(Cow::Borrowed(text)),
        serde_json::Value::Bool(b) => Val::Bool(*b),
        serde_json::Value::Number(n) => Val::Num(n.as_i64().unwrap_or(0)),
        other => Val::Node(other),
    }
}

/// The facts of an attribute part that its value and the later parts of its owner give.
fn attribute_fact(part: &super::data_attrs::Part, name: F) -> Val<'static> {
    let key = part.kind == "key";
    match name {
        F::LastKey => Val::Bool(part.last_key),
        F::Kept => Val::Bool(part.kept),
        F::Semantic => Val::Bool(key && matches!(part.key.as_str(), "abbr" | "time" | "kbd")),
        F::SemanticValue => Val::Bool(key && matches!(part.key.as_str(), "abbr" | "time")),
        F::Entries => Val::num(if key {
            super::data_attrs::entries(&part.value).0
        } else {
            0
        }),
        F::EntryTotal => Val::Num(if key {
            super::data_attrs::entries(&part.value).1
        } else {
            0
        }),
        _ => Val::Null,
    }
}
