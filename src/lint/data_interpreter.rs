//! Compiles the expressions of the pinned 0.2 lint model once and evaluates them over typed records.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use super::data_model::{Records, Subject};
use regex::{Regex, RegexBuilder};
use serde_json::Value;
use std::borrow::Cow;

/// A field name of a record, resolved when the model is compiled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum F {
    Prev,
    Prev2,
    Next,
    Next2,
    LeafHead,
    LeafNext,
    Ancestors,
    AttachedHere,
    Blank,
    Body,
    BodyEnd,
    BodyStart,
    ClosesContainer,
    End,
    Lazy,
    Leaf,
    LeafFirst,
    LeafId,
    LeafStart,
    Number,
    OpenColon,
    QuoteDepth,
    QuoteMarks,
    QuotePrefix,
    RawBody,
    Shape,
    Start,
    Text,
    AfterQuoteMark,
    Attributed,
    Column,
    ImageLine,
    Index,
    ItemOwner,
    LastLine,
    Line,
    MarkColumn,
    OpenerColumn,
    OpenerStart,
    OpenerText,
    Owner,
    Parent,
    PrevType,
    Type,
    ContentColumn,
    MarkerColumn,
    Width,
    Kind,
    Node,
    AncestorIds,
    Key,
    Value,
    KeyEnd,
    Kept,
    LastKey,
    Semantic,
    SemanticValue,
    Entries,
    EntryTotal,
    Format,
    Columns,
    HeadRows,
    BodyRows,
    FootRows,
    AlignOverlap,
    ValignOverlap,
    Id,
    Identity,
    Layout,
    SourceFormat,
    Group,
    RowIndex,
    TableAbove,
    TableBelow,
    RawRef,
    Unknown,
    /// Not a field: the fact that the previous line has the same leaf (`prev.leaf_id == leaf_id`).
    PrevSameLeaf,
}

const FIELDS: &[(&str, F)] = &[
    ("prev", F::Prev),
    ("prev2", F::Prev2),
    ("next", F::Next),
    ("next2", F::Next2),
    ("leaf_head", F::LeafHead),
    ("leaf_next", F::LeafNext),
    ("ancestors", F::Ancestors),
    ("ancestor_ids", F::AncestorIds),
    ("key", F::Key),
    ("value", F::Value),
    ("key_end", F::KeyEnd),
    ("kept", F::Kept),
    ("last_key", F::LastKey),
    ("semantic", F::Semantic),
    ("semantic_value", F::SemanticValue),
    ("entries", F::Entries),
    ("entry_total", F::EntryTotal),
    ("format", F::Format),
    ("columns", F::Columns),
    ("head_rows", F::HeadRows),
    ("body_rows", F::BodyRows),
    ("foot_rows", F::FootRows),
    ("align_overlap", F::AlignOverlap),
    ("valign_overlap", F::ValignOverlap),
    ("attached_here", F::AttachedHere),
    ("blank", F::Blank),
    ("body", F::Body),
    ("body_end", F::BodyEnd),
    ("body_start", F::BodyStart),
    ("closes_container", F::ClosesContainer),
    ("end", F::End),
    ("lazy", F::Lazy),
    ("leaf", F::Leaf),
    ("leaf_first", F::LeafFirst),
    ("leaf_id", F::LeafId),
    ("leaf_start", F::LeafStart),
    ("number", F::Number),
    ("open_colon", F::OpenColon),
    ("quote_depth", F::QuoteDepth),
    ("quote_marks", F::QuoteMarks),
    ("quote_prefix", F::QuotePrefix),
    ("raw_body", F::RawBody),
    ("shape", F::Shape),
    ("start", F::Start),
    ("text", F::Text),
    ("after_quote_mark", F::AfterQuoteMark),
    ("attributed", F::Attributed),
    ("column", F::Column),
    ("image_line", F::ImageLine),
    ("index", F::Index),
    ("item_owner", F::ItemOwner),
    ("last_line", F::LastLine),
    ("line", F::Line),
    ("mark_column", F::MarkColumn),
    ("opener_column", F::OpenerColumn),
    ("opener_start", F::OpenerStart),
    ("opener_text", F::OpenerText),
    ("owner", F::Owner),
    ("parent", F::Parent),
    ("prev_type", F::PrevType),
    ("type", F::Type),
    ("node", F::Node),
    ("content_column", F::ContentColumn),
    ("marker_column", F::MarkerColumn),
    ("width", F::Width),
    ("kind", F::Kind),
    ("id", F::Id),
    ("identity", F::Identity),
    ("layout", F::Layout),
    ("source_format", F::SourceFormat),
    ("group", F::Group),
    ("row_index", F::RowIndex),
    ("table_above", F::TableAbove),
    ("table_below", F::TableBelow),
    ("raw_ref", F::RawRef),
];

fn field_id(name: &str) -> F {
    FIELDS
        .iter()
        .find(|(known, _)| *known == name)
        .map_or(F::Unknown, |(_, id)| *id)
}

/// The name of a field, as an AST key when the field reads a node of the tree.
pub(super) fn field_name(id: F) -> &'static str {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    let names = NAMES.get_or_init(|| {
        let mut names = vec![""; F::PrevSameLeaf as usize + 1];
        for (name, known) in FIELDS {
            names[*known as usize] = name;
        }
        names
    });
    names.get(id as usize).copied().unwrap_or("")
}

/// One step of a dotted field path.
#[derive(Clone, Copy, Debug)]
pub(super) enum Seg {
    Item,
    Field(F),
    At(usize),
    /// An index counted from the end of a list: `-1` is the last item (C2-W-141).
    AtEnd(usize),
    /// A name no record field has: a key of an AST node (C2-W-140).
    Key(&'static str),
}

pub(super) type Path = Box<[Seg]>;

fn path(text: &str) -> Path {
    text.split('.')
        .enumerate()
        .map(|(at, part)| {
            if at == 0 && part == "item" {
                Seg::Item
            } else if let Ok(index) = part.parse() {
                Seg::At(index)
            } else if let Some(index) = part.strip_prefix('-').and_then(|n| n.parse().ok()) {
                Seg::AtEnd(index)
            } else {
                match field_id(part) {
                    // The names live as long as the compiled model, which is never dropped.
                    F::Unknown => Seg::Key(Box::leak(part.to_owned().into_boxed_str())),
                    id => Seg::Field(id),
                }
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A compiled pattern of C2-W-142 with the names of its groups.
#[derive(Debug)]
pub(super) struct Pattern {
    pub re: Regex,
    /// The names and capture indices of its named groups.
    pub groups: Vec<(&'static str, usize)>,
    /// For a pattern anchored at the start that cannot match the empty text: the bytes a match starts with.
    pub first: Option<Box<[bool; 256]>>,
    /// For a pattern anchored at the end of the text that cannot match the empty text: the bytes a match ends with.
    last: Option<Box<[bool; 256]>>,
    /// For an alternation whose every branch is anchored at the start or at the end: the bytes a match
    /// starts with at the start and those it ends with at the end.
    edges: Option<Box<Edges>>,
    /// Bytes of which every match holds at least one.
    needs: Option<Box<[bool; 256]>>,
    /// The pattern is `STRIP_QUOTES`, read without the regex engine.
    strip_quotes: bool,
    /// For a top-level alternation without look-around in which some branch has no group: each branch
    /// anchored at the start, with its groups. A span the whole pattern finds is the match of the first
    /// branch that matches at its start, so only that branch's groups need the capture engine.
    pub branches: Option<Vec<(Regex, Vec<(&'static str, usize)>)>>,
}

/// The branches of a top-level alternation without look-around, each anchored at the start, with the
/// names and indices of their groups; none unless some branch has no group.
fn branch_patterns(
    hir: &regex_syntax::hir::Hir,
) -> Option<Vec<(Regex, Vec<(&'static str, usize)>)>> {
    let regex_syntax::hir::HirKind::Alternation(items) = hir.kind() else {
        return None;
    };
    if !hir.properties().look_set().is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let re = Regex::new(&format!("\\A(?:{item})")).ok()?;
        let groups = re
            .capture_names()
            .enumerate()
            .filter_map(|(index, name)| {
                name.map(|name| (&*Box::leak(name.to_owned().into_boxed_str()), index))
            })
            .collect();
        out.push((re, groups));
    }
    out.iter()
        .any(|(_, groups): &(Regex, Vec<_>)| groups.is_empty())
        .then_some(out)
}

/// The rows' pattern for the content of a line behind its blanks and quote marks: it matches every line
/// text, and its group is what the greedy prefix and the trailing blanks leave.
const STRIP_QUOTES: &str = "^[ \\t]*(?:> [ \\t]*)*(?<c>.*?)[ \\t]*$";

/// The group of `STRIP_QUOTES` in a text without a line break.
fn strip_quotes(text: &str) -> (usize, usize) {
    let bytes = text.as_bytes();
    let blanks = |mut at: usize| {
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        at
    };
    let mut start = blanks(0);
    while bytes.get(start) == Some(&b'>') && bytes.get(start + 1) == Some(&b' ') {
        start = blanks(start + 2);
    }
    let end = start + text[start..].trim_end_matches([' ', '\t']).len();
    (start, end)
}

/// What an alternation of start- and end-anchored branches needs at the edges of the text: a branch
/// with a literal next to its anchor gives that literal, any other one its start or end bytes.
#[derive(Debug, Default)]
struct Edges {
    prefixes: Vec<Vec<u8>>,
    suffixes: Vec<Vec<u8>>,
    first: Option<Box<[bool; 256]>>,
    last: Option<Box<[bool; 256]>>,
}

impl Edges {
    fn admit(&self, text: &[u8]) -> bool {
        self.prefixes.iter().any(|prefix| text.starts_with(prefix))
            || self.suffixes.iter().any(|suffix| text.ends_with(suffix))
            || self
                .first
                .as_ref()
                .is_some_and(|set| text.first().is_some_and(|byte| set[usize::from(*byte)]))
            || self
                .last
                .as_ref()
                .is_some_and(|set| text.last().is_some_and(|byte| set[usize::from(*byte)]))
    }

    fn branch(&mut self, hir: &regex_syntax::hir::Hir) -> bool {
        if let Some(prefix) = edge_literal(hir, false) {
            self.prefixes.push(prefix);
            return true;
        }
        if let Some(suffix) = edge_literal(hir, true) {
            self.suffixes.push(suffix);
            return true;
        }
        let mut set = Box::new([false; 256]);
        if anchored_first(hir, &mut set) {
            let first = self.first.get_or_insert_with(|| Box::new([false; 256]));
            first.iter_mut().zip(set.iter()).for_each(|(a, b)| *a |= b);
            return true;
        }
        let mut set = Box::new([false; 256]);
        if anchored_last(hir, &mut set) {
            let last = self.last.get_or_insert_with(|| Box::new([false; 256]));
            last.iter_mut().zip(set.iter()).for_each(|(a, b)| *a |= b);
            return true;
        }
        false
    }
}

/// The literal bytes right after a leading `^` (or right before a trailing `$` when `end`).
fn edge_literal(hir: &regex_syntax::hir::Hir, end: bool) -> Option<Vec<u8>> {
    use regex_syntax::hir::{HirKind, Look};
    let HirKind::Concat(items) = hir.kind() else {
        return match hir.kind() {
            HirKind::Capture(capture) => edge_literal(&capture.sub, end),
            _ => None,
        };
    };
    let anchor = if end { Look::End } else { Look::Start };
    let mut order: Vec<&regex_syntax::hir::Hir> = items.iter().collect();
    if end {
        order.reverse();
    }
    if !matches!(order.first().map(|item| item.kind()), Some(HirKind::Look(look)) if *look == anchor)
    {
        return None;
    }
    let mut literal: Vec<u8> = Vec::new();
    for item in order.iter().skip(1) {
        let HirKind::Literal(bytes) = item.kind() else {
            break;
        };
        if end {
            let mut joined = bytes.0.to_vec();
            joined.extend_from_slice(&literal);
            literal = joined;
        } else {
            literal.extend_from_slice(&bytes.0);
        }
    }
    (!literal.is_empty()).then_some(literal)
}

impl Pattern {
    /// Whether the text can match at all, from its first byte; a quick test before the regex runs.
    pub fn filters(&self) -> bool {
        self.first.is_some() || self.last.is_some() || self.edges.is_some() || self.needs.is_some()
    }

    pub fn may_match(&self, text: &str) -> bool {
        self.first.as_ref().is_none_or(|first| {
            text.as_bytes()
                .first()
                .is_some_and(|byte| first[usize::from(*byte)])
        }) && self.last.as_ref().is_none_or(|last| {
            text.as_bytes()
                .last()
                .is_some_and(|byte| last[usize::from(*byte)])
        }) && self
            .edges
            .as_ref()
            .is_none_or(|edges| edges.admit(text.as_bytes()))
            && self
                .needs
                .as_ref()
                .is_none_or(|needs| text.as_bytes().iter().any(|byte| needs[usize::from(*byte)]))
    }
}

/// Adds the bytes `hir` can start with to `set`; returns whether it can match the empty text.
fn first_bytes(hir: &regex_syntax::hir::Hir, set: &mut [bool; 256]) -> bool {
    use regex_syntax::hir::{Class, HirKind};
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => true,
        HirKind::Literal(literal) => {
            if let Some(byte) = literal.0.first() {
                set[usize::from(*byte)] = true;
            }
            literal.0.is_empty()
        }
        HirKind::Class(Class::Unicode(class)) => {
            for range in class.iter() {
                let (from, to) = (u32::from(range.start()), u32::from(range.end()));
                for byte in from.min(0x80)..=to.min(0x7f) {
                    set[byte as usize] = true;
                }
                if to >= 0x80 {
                    set[0x80..].fill(true);
                }
            }
            false
        }
        HirKind::Class(Class::Bytes(class)) => {
            for range in class.iter() {
                set[usize::from(range.start())..=usize::from(range.end())].fill(true);
            }
            false
        }
        HirKind::Repetition(repetition) => first_bytes(&repetition.sub, set) || repetition.min == 0,
        HirKind::Capture(capture) => first_bytes(&capture.sub, set),
        HirKind::Concat(items) => items.iter().all(|item| first_bytes(item, set)),
        HirKind::Alternation(items) => items
            .iter()
            .fold(false, |nullable, item| first_bytes(item, set) || nullable),
    }
}

/// Adds the bytes `hir` can end with to `set`; returns whether it can match the empty text.
fn last_bytes(hir: &regex_syntax::hir::Hir, set: &mut [bool; 256]) -> bool {
    use regex_syntax::hir::HirKind;
    match hir.kind() {
        HirKind::Literal(literal) => {
            if let Some(byte) = literal.0.last() {
                set[usize::from(*byte)] = true;
            }
            literal.0.is_empty()
        }
        HirKind::Repetition(repetition) => last_bytes(&repetition.sub, set) || repetition.min == 0,
        HirKind::Capture(capture) => last_bytes(&capture.sub, set),
        HirKind::Concat(items) => items.iter().rev().all(|item| last_bytes(item, set)),
        HirKind::Alternation(items) => items
            .iter()
            .fold(false, |nullable, item| last_bytes(item, set) || nullable),
        // A class or look ends a match the same way it starts one.
        _ => first_bytes(hir, set),
    }
}

/// The end bytes of a pattern whose every match ends at the end of the text, if it has any.
fn anchored_last(hir: &regex_syntax::hir::Hir, set: &mut [bool; 256]) -> bool {
    use regex_syntax::hir::{HirKind, Look};
    match hir.kind() {
        HirKind::Concat(items)
            if matches!(
                items.last().map(regex_syntax::hir::Hir::kind),
                Some(HirKind::Look(Look::End))
            ) =>
        {
            !items.iter().rev().skip(1).all(|item| last_bytes(item, set))
        }
        HirKind::Capture(capture) => anchored_last(&capture.sub, set),
        HirKind::Alternation(items) => items.iter().all(|item| anchored_last(item, set)),
        _ => false,
    }
}

/// A set of bytes every match of `hir` holds one of, keeping the smallest a concatenation offers.
/// The bytes of a small ASCII class, or `None` for a class too wide to help.
fn class_bytes(class: &regex_syntax::hir::Class) -> Option<Vec<u8>> {
    use regex_syntax::hir::Class;
    let bytes: Vec<u8> = match class {
        Class::Unicode(class) => {
            let mut bytes = Vec::new();
            for range in class.iter() {
                if u32::from(range.end()) >= 0x80 || bytes.len() > 8 {
                    return None;
                }
                bytes.extend(u32::from(range.start()) as u8..=u32::from(range.end()) as u8);
            }
            bytes
        }
        Class::Bytes(class) => class
            .iter()
            .flat_map(|range| range.start()..=range.end())
            .collect(),
    };
    (!bytes.is_empty() && bytes.len() <= 8).then_some(bytes)
}

fn needed_bytes(hir: &regex_syntax::hir::Hir) -> Option<Vec<u8>> {
    use regex_syntax::hir::HirKind;
    match hir.kind() {
        HirKind::Literal(literal) => literal.0.first().map(|byte| vec![*byte]),
        HirKind::Class(class) => class_bytes(class),
        HirKind::Repetition(repetition) if repetition.min > 0 => needed_bytes(&repetition.sub),
        HirKind::Capture(capture) => needed_bytes(&capture.sub),
        HirKind::Concat(items) => items.iter().filter_map(needed_bytes).min_by_key(Vec::len),
        HirKind::Alternation(items) => {
            let mut all = Vec::new();
            for item in items {
                all.extend(needed_bytes(item)?);
            }
            all.sort_unstable();
            all.dedup();
            Some(all)
        }
        _ => None,
    }
}

/// The start bytes of a pattern whose every match begins at the start of the text, if it has any.
fn anchored_first(hir: &regex_syntax::hir::Hir, set: &mut [bool; 256]) -> bool {
    use regex_syntax::hir::{HirKind, Look};
    match hir.kind() {
        HirKind::Concat(items)
            if matches!(
                items.first().map(regex_syntax::hir::Hir::kind),
                Some(HirKind::Look(Look::Start))
            ) =>
        {
            !items.iter().skip(1).all(|item| first_bytes(item, set))
        }
        HirKind::Capture(capture) => anchored_first(&capture.sub, set),
        HirKind::Alternation(items) => items.iter().all(|item| anchored_first(item, set)),
        _ => false,
    }
}

pub(super) fn pattern(text: &str, flags: &str) -> Option<Pattern> {
    let original = text;
    let text = &text.replace("(?<", "(?P<");
    let re = RegexBuilder::new(text)
        .case_insensitive(flags.contains('i'))
        .build()
        .ok()?;
    // The names live as long as the compiled model, which is never dropped.
    let groups: Vec<(&'static str, usize)> = re
        .capture_names()
        .enumerate()
        .filter_map(|(index, name)| {
            name.map(|name| (&*Box::leak(name.to_owned().into_boxed_str()), index))
        })
        .collect();
    let hir = regex_syntax::ParserBuilder::new()
        .case_insensitive(flags.contains('i'))
        .build()
        .parse(text)
        .ok();
    let mut set = Box::new([false; 256]);
    let first = hir
        .as_ref()
        .filter(|hir| anchored_first(hir, &mut set))
        .map(|_| set);
    let mut end_set = Box::new([false; 256]);
    let last = hir
        .as_ref()
        .filter(|hir| anchored_last(hir, &mut end_set))
        .map(|_| end_set);
    let edges = hir.as_ref().and_then(|hir| {
        let mut edges = Box::<Edges>::default();
        let all = match hir.kind() {
            regex_syntax::hir::HirKind::Alternation(items) => {
                items.iter().all(|item| edges.branch(item))
            }
            // One branch: worth it only for a literal longer than the first or last byte.
            _ => {
                edges.branch(hir)
                    && edges
                        .prefixes
                        .iter()
                        .chain(&edges.suffixes)
                        .any(|lit| lit.len() > 1)
            }
        };
        all.then_some(edges)
    });
    let needs = hir.as_ref().and_then(needed_bytes).map(|bytes| {
        let mut set = Box::new([false; 256]);
        for byte in bytes {
            set[usize::from(byte)] = true;
        }
        set
    });
    let branches = hir
        .as_ref()
        .filter(|_| !groups.is_empty())
        .and_then(branch_patterns);
    Some(Pattern {
        strip_quotes: original == STRIP_QUOTES && flags.is_empty(),
        branches,
        re,
        groups,
        first,
        last,
        edges,
        needs,
    })
}

#[derive(Debug)]
pub(super) enum Expr {
    Lit(Val<'static>),
    Field(Path),
    Var(usize),
    Has(Path),
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
    If(Box<(Self, Self, Self)>),
    Cmp(Cmp, Box<(Self, Self)>),
    In(Box<Self>, Vec<Val<'static>>),
    Contains(Box<(Self, Self)>),
    Starts(Box<(Self, Self)>),
    /// `eq` of a line's own `shape` with a literal, decided without naming the shape.
    ShapeIs(&'static str),
    Matches {
        subject: Box<Self>,
        pattern: Option<Pattern>,
        base: Option<Path>,
    },
    Match {
        group: &'static str,
        end: bool,
    },
    Some {
        list: Box<Self>,
        from: usize,
        test: Box<Self>,
    },
    /// The number of list items for which `test` holds (C2-W-141).
    Count {
        list: Box<Self>,
        test: Box<Self>,
    },
    /// The first list item for which `test` holds (C2-W-150).
    Find {
        list: Box<Self>,
        test: Box<Self>,
    },
    Nth(Box<(Self, Self)>),
    Join(Box<(Self, Self)>),
    Lower(Box<Self>),
    PercentDecode(Box<Self>),
    /// The column of an offset after a number of quote marks (C2-B-004).
    Column(Box<(Self, Self)>),
    /// The extensions registered for the run.
    RunExtensions,
    /// Whether a text is a valid attribute line (C2-B-090).
    AttributeLine(Box<Self>),
    /// A `matches` on a `let` that is a slice of `field` or empty: the match starts with one of the
    /// pattern's first bytes, so `field` must hold one before the `let` is computed.
    Within {
        field: Path,
        test: Box<Self>,
    },
    /// A field of a record of the document found by key, or the last one before (C2-W-141).
    Lookup(Box<Lookup>),
    IndexOf(Box<(Self, Self)>),
    JsonEscape(Box<Self>),
    Len(Box<Self>),
    Add(Vec<Self>),
    Sub(Box<(Self, Self)>),
    Slice(Box<(Self, Self, Option<Self>)>),
    Concat(Vec<Self>),
    TrimEnd(Box<Self>),
    Words(Box<Self>),
    TextFrom(Box<Self>),
    Shift {
        left: bool,
        args: Box<(Self, Self)>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum LookupOver {
    Block,
    Inline,
    Id,
    Line,
}

#[derive(Debug)]
pub(super) struct Lookup {
    pub over: LookupOver,
    pub types: Vec<&'static str>,
    pub key: Option<Expr>,
    pub value: Option<Expr>,
    pub before: bool,
    pub after: bool,
    pub all: bool,
    pub from: Option<Expr>,
    pub field: Path,
    /// Lookups over the same records with the same key share one index per call.
    pub site: usize,
}

impl Lookup {
    fn record<'a>(&self, at: u32) -> Val<'a> {
        match self.over {
            LookupOver::Block | LookupOver::Inline => Val::Rec(at),
            LookupOver::Id => Val::IdRec(at),
            LookupOver::Line => Val::Line(at),
        }
    }
}

const COMPARISONS: &[(&str, Cmp)] = &[
    ("eq", Cmp::Eq),
    ("ne", Cmp::Ne),
    ("lt", Cmp::Lt),
    ("le", Cmp::Le),
    ("gt", Cmp::Gt),
    ("ge", Cmp::Ge),
];

/// Resolves set names and `let` names while compiling.
pub(super) struct Scope<'m> {
    pub model: &'static Value,
    pub lets: &'m [&'static str],
    /// The rule being compiled, whose `let` values the compiler reads.
    pub rule: &'static Value,
}

fn literal(value: &'static Value) -> Val<'static> {
    match value {
        Value::Bool(b) => Val::Bool(*b),
        Value::Number(n) => Val::Num(n.as_i64().unwrap_or(0)),
        Value::String(s) => Val::Str(Cow::Borrowed(s.as_str())),
        _ => Val::Null,
    }
}

impl Scope<'_> {
    pub fn compile(&self, expr: &'static Value) -> Expr {
        let Some(object) = expr.as_object() else {
            return Expr::Lit(literal(expr));
        };
        let Some((op, arg)) = object
            .get_key_value("match")
            .or_else(|| object.iter().find(|(key, _)| key.as_str() != "at"))
        else {
            return Expr::Lit(Val::Null);
        };
        let null = &Value::Null;
        let args: &'static [Value] = arg.as_array().map_or(&[], Vec::as_slice);
        let at = |i: usize| args.get(i).unwrap_or(null);
        let pair = |a: usize, b: usize| Box::new((self.compile(at(a)), self.compile(at(b))));
        let list = |items: &'static [Value]| items.iter().map(|item| self.compile(item)).collect();
        if let Some((_, cmp)) = COMPARISONS.iter().find(|(name, _)| name == op) {
            return self.comparison(*cmp, at(0), at(1));
        }
        match op.as_str() {
            "f" => Expr::Field(path(arg.as_str().unwrap_or(""))),
            "v" => self.variable(arg.as_str().unwrap_or("")),
            "all" if args.iter().all(|item| self.order_free(item)) => {
                let mut items: Vec<Expr> = list(args);
                items.sort_by_key(scans);
                Expr::All(items)
            }
            "all" => Expr::All(list(args)),
            "any" => Expr::Any(list(args)),
            "not" => Expr::Not(Box::new(self.compile(arg))),
            "in" => self.member(at(0), at(1)),
            "contains" => Expr::Contains(pair(0, 1)),
            "starts" => Expr::Starts(pair(0, 1)),
            "matches" => self.matches(args),
            "match" => Expr::Match {
                group: arg.as_str().unwrap_or(""),
                end: object.get("at").and_then(Value::as_str) == Some("end"),
            },
            _ => self.compile_extra(op, arg, args),
        }
    }

    /// A comparison; `eq` and `ne` of a line's own `shape` with a literal test that shape alone.
    fn comparison(&self, cmp: Cmp, left: &'static Value, right: &'static Value) -> Expr {
        if let (Cmp::Eq | Cmp::Ne, Some("shape"), Some(name)) =
            (cmp, left.get("f").and_then(Value::as_str), right.as_str())
        {
            let test = Expr::ShapeIs(name);
            return if matches!(cmp, Cmp::Eq) {
                test
            } else {
                Expr::Not(Box::new(test))
            };
        }
        Expr::Cmp(cmp, Box::new((self.compile(left), self.compile(right))))
    }

    /// Whether a test neither sets nor reads match groups, so the items of an `all` of such tests may
    /// run in any order; a `let` may read the groups of its own `matches`.
    fn order_free(&self, value: &'static Value) -> bool {
        fn groups(value: &Value, out: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    if let Some(pattern) = map
                        .get("matches")
                        .and_then(|args| args.get(1))
                        .and_then(Value::as_str)
                    {
                        let mut rest = pattern;
                        while let Some(at) = rest.find("(?<") {
                            rest = &rest[at + 3..];
                            out.push(rest.split('>').next().unwrap_or("").to_owned());
                        }
                    }
                    map.values().for_each(|item| groups(item, out));
                }
                Value::Array(items) => items.iter().for_each(|item| groups(item, out)),
                _ => {}
            }
        }
        fn reads(value: &Value, own: &[String], scope: &Scope<'_>, depth: usize) -> bool {
            match value {
                Value::Object(map) => {
                    if let Some(group) = map.get("match").and_then(Value::as_str) {
                        return own.iter().any(|known| known == group);
                    }
                    if let Some(name) = map.get("v").and_then(Value::as_str) {
                        let body = &scope.rule["let"][name];
                        let mut mine = Vec::new();
                        groups(body, &mut mine);
                        return depth < 8 && reads(body, &mine, scope, depth + 1);
                    }
                    map.values().all(|item| reads(item, own, scope, depth))
                }
                Value::Array(items) => items.iter().all(|item| reads(item, own, scope, depth)),
                _ => true,
            }
        }
        let mut set = Vec::new();
        groups(value, &mut set);
        set.is_empty() && reads(value, &[], self, 0)
    }

    /// The `in` of `some`, `count` and `find`: a field path, or a value that gives a list (C2-W-150).
    fn list(&self, value: &'static Value) -> Expr {
        value
            .as_str()
            .map_or_else(|| self.compile(value), |text| Expr::Field(path(text)))
    }

    fn variable(&self, name: &str) -> Expr {
        self.lets
            .iter()
            .position(|known| *known == name)
            .map_or(Expr::Lit(Val::Null), Expr::Var)
    }

    /// An `in` test; its set is a list or the name of a list of the model.
    fn member(&self, subject: &'static Value, set: &'static Value) -> Expr {
        let members = set.as_str().map_or_else(
            || set.as_array(),
            |name| self.model.get(name).and_then(Value::as_array),
        );
        Expr::In(
            Box::new(self.compile(subject)),
            members.into_iter().flatten().map(literal).collect(),
        )
    }

    fn matches(&self, args: &'static [Value]) -> Expr {
        let subject = args.first().unwrap_or(&Value::Null);
        let compiled = args
            .get(1)
            .and_then(Value::as_str)
            .and_then(|text| pattern(text, args.get(2).and_then(Value::as_str).unwrap_or("")));
        let within = subject
            .get("v")
            .and_then(Value::as_str)
            .and_then(|name| sliced_field(&self.rule["let"][name]))
            .filter(|_| compiled.as_ref().is_some_and(|p| p.first.is_some()));
        let test = Expr::Matches {
            subject: Box::new(self.compile(subject)),
            pattern: compiled,
            base: subject.get("f").and_then(Value::as_str).and_then(base_path),
        };
        match within {
            Some(field) => Expr::Within {
                field: path(field),
                test: Box::new(test),
            },
            None => test,
        }
    }

    fn compile_extra(&self, op: &str, arg: &'static Value, args: &'static [Value]) -> Expr {
        let null = &Value::Null;
        let at = |i: usize| args.get(i).unwrap_or(null);
        let pair = |a: usize, b: usize| Box::new((self.compile(at(a)), self.compile(at(b))));
        let list = |items: &'static [Value]| items.iter().map(|item| self.compile(item)).collect();
        match op {
            "has" => Expr::Has(path(arg.as_str().unwrap_or(""))),
            "if" => Expr::If(Box::new((
                self.compile(at(0)),
                self.compile(at(1)),
                self.compile(at(2)),
            ))),
            "some" => Expr::Some {
                list: Box::new(self.list(arg.get("in").unwrap_or(null))),
                from: arg.get("from").and_then(Value::as_u64).unwrap_or(0) as usize,
                test: Box::new(self.compile(arg.get("where").unwrap_or(null))),
            },
            "len" => Expr::Len(Box::new(self.compile(arg))),
            "add" => Expr::Add(list(args)),
            "sub" => Expr::Sub(pair(0, 1)),
            "slice" => Expr::Slice(Box::new((
                self.compile(at(0)),
                self.compile(at(1)),
                args.get(2).map(|end| self.compile(end)),
            ))),
            "concat" => Expr::Concat(list(args)),
            "trim_end" => Expr::TrimEnd(Box::new(self.compile(arg))),
            "words" => Expr::Words(Box::new(self.compile(arg))),
            "text_from" => Expr::TextFrom(Box::new(self.compile(arg))),
            "extend_left" => Expr::Shift {
                left: true,
                args: pair(0, 1),
            },
            "skip_right" => Expr::Shift {
                left: false,
                args: pair(0, 1),
            },
            _ => self.compile_scan(op, arg, args),
        }
    }

    /// The values that read lists and records of the document (C2-W-141).
    fn compile_scan(&self, op: &str, arg: &'static Value, args: &'static [Value]) -> Expr {
        let null = &Value::Null;
        let at = |i: usize| args.get(i).unwrap_or(null);
        let pair = |a: usize, b: usize| Box::new((self.compile(at(a)), self.compile(at(b))));
        match op {
            "lookup" => Expr::Lookup(Box::new(Lookup {
                over: match arg.get("over").and_then(Value::as_str) {
                    Some("inline") => LookupOver::Inline,
                    Some("id") => LookupOver::Id,
                    Some("line") => LookupOver::Line,
                    _ => LookupOver::Block,
                },
                types: arg
                    .get("types")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect(),
                key: arg.get("key").map(|key| self.compile(key)),
                value: arg.get("value").map(|value| self.compile(value)),
                before: arg.get("before").and_then(Value::as_bool) == Some(true),
                after: arg.get("after").and_then(Value::as_bool) == Some(true),
                all: arg.get("all").and_then(Value::as_bool) == Some(true),
                from: arg.get("from").map(|from| self.compile(from)),
                field: arg
                    .get("field")
                    .and_then(Value::as_str)
                    .map_or_else(|| Box::default(), path),
                site: lookup_site(arg),
            })),
            "index_of" => Expr::IndexOf(pair(0, 1)),
            "json_escape" => Expr::JsonEscape(Box::new(self.compile(arg))),
            "count" => Expr::Count {
                list: Box::new(self.list(arg.get("in").unwrap_or(null))),
                test: Box::new(self.compile(arg.get("where").unwrap_or(null))),
            },
            "find" => Expr::Find {
                list: Box::new(self.list(arg.get("in").unwrap_or(null))),
                test: Box::new(self.compile(arg.get("where").unwrap_or(null))),
            },
            "nth" => Expr::Nth(pair(0, 1)),
            "join" => Expr::Join(pair(0, 1)),
            "column" => Expr::Column(pair(0, 1)),
            "lower" => Expr::Lower(Box::new(self.compile(arg))),
            "attribute_line" => Expr::AttributeLine(Box::new(self.compile(arg))),
            "percent_decode" => Expr::PercentDecode(Box::new(self.compile(arg))),
            "run" if arg.as_str() == Some("extensions") => Expr::RunExtensions,
            _ => Expr::Lit(Val::Null),
        }
    }
}

/// What a bucket of records shares, for deciding tests before any record is read.
pub(super) struct Facts<'k> {
    /// The record's own `type` (block or inline) or `leaf` (line).
    pub kind: &'k str,
    /// The type of the parent of a block.
    pub parent: Option<&'k str>,
    /// The first byte of the record's own `text`, and of a line's `body`; `None` inside is an empty value.
    pub text_first: Option<Option<u8>>,
    pub body_first: Option<Option<u8>>,
    /// Flags of the record, and `PrevSameLeaf` for a line.
    pub flags: &'k [(F, bool)],
    /// Whether a block is the first child of its parent (`index` 0).
    pub first_child: Option<bool>,
    /// Literals the record's own `text` or `body` is known to hold or not.
    pub holds: &'k [(bool, &'static str, bool)],
}

fn own(expr: &Expr) -> Option<F> {
    match expr {
        Expr::Field(path) => match path.as_ref() {
            [Seg::Field(name)] => Some(*name),
            _ => None,
        },
        _ => None,
    }
}

fn parent_type(expr: &Expr) -> bool {
    matches!(expr, Expr::Field(path) if matches!(path.as_ref(), [Seg::Field(F::Parent), Seg::Field(F::Type)]))
}

/// A string literal operand.
fn literal_text(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Lit(Val::Str(text)) => Some(text),
        _ => None,
    }
}

/// `prev.leaf_id` against `leaf_id`, in either order.
fn prev_same_leaf(args: &(Expr, Expr)) -> bool {
    let is = |expr: &Expr, want: &[F]| match expr {
        Expr::Field(path) => {
            path.len() == want.len()
                && path
                    .iter()
                    .zip(want)
                    .all(|(seg, f)| matches!(seg, Seg::Field(name) if name == f))
        }
        _ => false,
    };
    let prev = [F::Prev, F::LeafId];
    let own = [F::LeafId];
    (is(&args.0, &prev) && is(&args.1, &own)) || (is(&args.0, &own) && is(&args.1, &prev))
}

impl Facts<'_> {
    /// The name the facts give an operand: the record's kind or its parent's type.
    fn name(&self, expr: &Expr, over_line: bool) -> Option<&str> {
        match own(expr) {
            Some(F::Leaf) if over_line => Some(self.kind),
            Some(F::Type) if !over_line => Some(self.kind),
            _ if parent_type(expr) => self.parent,
            _ => None,
        }
    }

    fn first(&self, expr: &Expr) -> Option<(bool, Option<u8>)> {
        match own(expr)? {
            F::Text => self.text_first.map(|first| (false, first)),
            F::Body => self.body_first.map(|first| (true, first)),
            _ => None,
        }
    }

    /// The truth of a primitive test when the facts fix it.
    fn known(&self, expr: &Expr, over_line: bool) -> Option<bool> {
        match expr {
            Expr::Cmp(op, args) => self.compared(*op, args, over_line),
            Expr::In(subject, members) => {
                let name = self.name(subject, over_line)?;
                Some(
                    members
                        .iter()
                        .any(|member| matches!(member, Val::Str(m) if m == name)),
                )
            }
            Expr::Starts(args) => {
                let prefix = literal_text(&args.1)?.as_bytes().first().copied()?;
                let (_, first) = self.first(&args.0)?;
                (first != Some(prefix)).then_some(false)
            }
            Expr::Contains(args) => self.held(&args.0, literal_text(&args.1)?),
            Expr::Matches {
                subject,
                pattern: Some(pattern),
                ..
            } => self.may_start(subject, pattern),
            Expr::Field(_) => self.flag(expr),
            Expr::ShapeIs(name) => self.shape_known(name),
            _ => None,
        }
    }

    fn compared(&self, op: Cmp, args: &(Expr, Expr), over_line: bool) -> Option<bool> {
        let eq = match op {
            Cmp::Eq => true,
            Cmp::Ne => false,
            _ if !over_line && own(&args.0) == Some(F::Index) => {
                return self.index_known(op, &args.1);
            }
            _ => return None,
        };
        if over_line && prev_same_leaf(args) {
            return self
                .flags
                .iter()
                .find(|(flag, _)| *flag == F::PrevSameLeaf)
                .map(|(_, same)| *same == eq);
        }
        if !over_line && own(&args.0) == Some(F::Index) {
            return self.index_known(op, &args.1);
        }
        self.equal(&args.0, literal_text(&args.1)?, over_line)
            .map(|same| same == eq)
    }

    /// A comparison of a block's `index` with a number: index 0 decides every comparison, a later
    /// child only `ge 1` and `gt 0`.
    fn index_known(&self, op: Cmp, limit: &Expr) -> Option<bool> {
        let Expr::Lit(Val::Num(limit)) = *limit else {
            return None;
        };
        if !self.first_child? {
            return match op {
                Cmp::Ge => (limit <= 1).then_some(true),
                Cmp::Gt => (limit <= 0).then_some(true),
                _ => None,
            };
        }
        Some(match op {
            Cmp::Eq => limit == 0,
            Cmp::Ne => limit != 0,
            Cmp::Lt => 0 < limit,
            Cmp::Le => 0 <= limit,
            Cmp::Gt => 0 > limit,
            Cmp::Ge => 0 >= limit,
        })
    }

    /// The shapes a line can only have with a known first byte of its body.
    fn shape_known(&self, name: &str) -> Option<bool> {
        let first = self.body_first?;
        let needed = match name {
            "attribute_line" => b'{',
            "div_bare_run" => b':',
            _ => return None,
        };
        (first != Some(needed)).then_some(false)
    }

    /// Whether an operand equals a literal, when the facts fix it.
    fn equal(&self, subject: &Expr, value: &str, over_line: bool) -> Option<bool> {
        if let Some(name) = self.name(subject, over_line) {
            return Some(name == value);
        }
        let (_, first) = self.first(subject)?;
        (first != value.as_bytes().first().copied()).then_some(false)
    }

    /// False when the first byte of the operand cannot start an anchored pattern.
    fn may_start(&self, subject: &Expr, pattern: &Pattern) -> Option<bool> {
        let (_, first) = self.first(subject)?;
        let set = pattern.first.as_ref()?;
        (!first.is_some_and(|byte| set[usize::from(byte)])).then_some(false)
    }

    fn flag(&self, expr: &Expr) -> Option<bool> {
        let name = own(expr)?;
        self.flags
            .iter()
            .find(|(flag, _)| *flag == name)
            .map(|(_, value)| *value)
    }

    fn held(&self, subject: &Expr, needle: &str) -> Option<bool> {
        let body = match own(subject)? {
            F::Text => false,
            F::Body => true,
            _ => return None,
        };
        self.holds
            .iter()
            .find(|(on_body, text, _)| *on_body == body && *text == needle)
            .map(|(_, _, held)| *held)
    }

    /// Whether the facts make a test false before it reaches anything that leaves state behind (a
    /// match that sets groups, a `let`); such a report entry can be skipped without changing what a
    /// later entry reads.
    pub fn skips(&self, expr: &Expr, over_line: bool) -> bool {
        match expr {
            Expr::All(items) => {
                for item in items {
                    if self.skips(item, over_line) {
                        return true;
                    }
                    if !self.sure(item, over_line) {
                        return false;
                    }
                }
                false
            }
            Expr::Any(items) => items.iter().all(|item| self.skips(item, over_line)),
            Expr::Not(item) => self.sure(item, over_line),
            _ => self.known(expr, over_line) == Some(false),
        }
    }

    /// Whether the facts make a test true without it leaving state behind.
    fn sure(&self, expr: &Expr, over_line: bool) -> bool {
        match expr {
            Expr::All(items) => items.iter().all(|item| self.sure(item, over_line)),
            Expr::Any(items) => {
                for item in items {
                    if self.sure(item, over_line) {
                        return true;
                    }
                    if !self.skips(item, over_line) {
                        return false;
                    }
                }
                false
            }
            Expr::Not(item) => self.skips(item, over_line),
            _ => self.known(expr, over_line) == Some(true),
        }
    }

    /// The truth of a test under the facts, when they decide it.
    pub fn decide(&self, expr: &Expr, over_line: bool) -> Option<bool> {
        match expr {
            Expr::All(items) => {
                let mut all = Some(true);
                for item in items {
                    match self.decide(item, over_line) {
                        Some(false) => return Some(false),
                        None => all = None,
                        Some(true) => {}
                    }
                }
                all
            }
            Expr::Any(items) => {
                let mut any = Some(false);
                for item in items {
                    match self.decide(item, over_line) {
                        Some(true) => return Some(true),
                        None => any = None,
                        Some(false) => {}
                    }
                }
                any
            }
            Expr::Not(item) => self.decide(item, over_line).map(|value| !value),
            _ => self.known(expr, over_line),
        }
    }
}

/// The literals tests read with `contains` on the record's own `text` (false) or `body` (true).
pub(super) fn held_literals(expr: &Expr, out: &mut Vec<(bool, &'static str)>) {
    match expr {
        Expr::All(items) | Expr::Any(items) => {
            items.iter().for_each(|item| held_literals(item, out));
        }
        Expr::Not(item) => held_literals(item, out),
        Expr::Contains(args) => {
            if let (Some(name @ (F::Text | F::Body)), Expr::Lit(Val::Str(Cow::Borrowed(text)))) =
                (own(&args.0), &args.1)
            {
                let entry = (name == F::Body, *text);
                if !out.contains(&entry) {
                    out.push(entry);
                }
            }
        }
        _ => {}
    }
}

/// `haystack.contains(needle)` for the short literals of the model: the first byte of an ASCII-led
/// needle is found with the `memchr` path of a `char` search, then the rest is compared.
pub(super) fn holds(haystack: &str, needle: &str) -> bool {
    let Some(&lead) = needle.as_bytes().first().filter(|byte| byte.is_ascii()) else {
        return haystack.contains(needle);
    };
    let mut rest = haystack;
    while let Some(at) = rest.find(char::from(lead)) {
        if rest[at..].starts_with(needle) {
            return true;
        }
        rest = &rest[at + 1..];
    }
    false
}

/// The conjuncts of a top-level `all` that are a filtered pattern over a field, by index: tests a
/// record can fail before anything is evaluated (a field reads no `let` and no match).
pub(super) fn quick_tests(when: &Expr) -> Vec<usize> {
    let Expr::All(items) = when else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(at, item)| match item {
            Expr::Matches {
                subject,
                pattern: Some(pattern),
                ..
            } if pattern.filters() && matches!(**subject, Expr::Field(_)) => Some(at),
            _ => None,
        })
        .collect()
}

/// Whether an expression reads records other than its own: a lookup, or a test over a list.
fn scans(expr: &Expr) -> bool {
    match expr {
        Expr::Lookup(_) | Expr::Some { .. } | Expr::Count { .. } | Expr::Find { .. } => true,
        Expr::All(items) | Expr::Any(items) | Expr::Add(items) | Expr::Concat(items) => {
            items.iter().any(scans)
        }
        Expr::Within { test: item, .. }
        | Expr::Not(item)
        | Expr::Lower(item)
        | Expr::PercentDecode(item)
        | Expr::AttributeLine(item)
        | Expr::Len(item)
        | Expr::TrimEnd(item)
        | Expr::Words(item)
        | Expr::TextFrom(item)
        | Expr::JsonEscape(item)
        | Expr::In(item, _) => scans(item),
        Expr::Matches { subject, .. } => scans(subject),
        Expr::Cmp(_, args)
        | Expr::Contains(args)
        | Expr::Starts(args)
        | Expr::Sub(args)
        | Expr::IndexOf(args)
        | Expr::Nth(args)
        | Expr::Join(args)
        | Expr::Column(args)
        | Expr::Shift { args, .. } => scans(&args.0) || scans(&args.1),
        Expr::If(args) => scans(&args.0) || scans(&args.1) || scans(&args.2),
        Expr::Slice(args) => scans(&args.0) || scans(&args.1) || args.2.as_ref().is_some_and(scans),
        Expr::Var(_) => true,
        Expr::Lit(_)
        | Expr::Field(_)
        | Expr::Has(_)
        | Expr::Match { .. }
        | Expr::ShapeIs(_)
        | Expr::RunExtensions => false,
    }
}

/// Whether a test can neither set groups nor fill a `let`, so skipping it changes nothing later.
fn stateless(expr: &Expr) -> bool {
    match expr {
        Expr::Var(_) => false,
        Expr::Matches {
            subject, pattern, ..
        } => pattern.as_ref().is_none_or(|p| p.groups.is_empty()) && stateless(subject),
        Expr::All(items) | Expr::Any(items) | Expr::Add(items) | Expr::Concat(items) => {
            items.iter().all(stateless)
        }
        Expr::Within { test: item, .. }
        | Expr::Not(item)
        | Expr::Lower(item)
        | Expr::PercentDecode(item)
        | Expr::AttributeLine(item)
        | Expr::Len(item)
        | Expr::TrimEnd(item)
        | Expr::Words(item)
        | Expr::TextFrom(item)
        | Expr::JsonEscape(item)
        | Expr::In(item, _) => stateless(item),
        Expr::Cmp(_, args)
        | Expr::Contains(args)
        | Expr::Starts(args)
        | Expr::Sub(args)
        | Expr::IndexOf(args)
        | Expr::Nth(args)
        | Expr::Join(args)
        | Expr::Column(args)
        | Expr::Shift { args, .. } => stateless(&args.0) && stateless(&args.1),
        Expr::If(args) => stateless(&args.0) && stateless(&args.1) && stateless(&args.2),
        Expr::Slice(args) => {
            stateless(&args.0) && stateless(&args.1) && args.2.as_ref().is_none_or(stateless)
        }
        Expr::Some { .. } | Expr::Count { .. } | Expr::Find { .. } | Expr::Lookup(_) => {
            stateless_scan(expr)
        }
        Expr::Lit(_)
        | Expr::Field(_)
        | Expr::Has(_)
        | Expr::Match { .. }
        | Expr::ShapeIs(_)
        | Expr::RunExtensions => true,
    }
}

fn stateless_scan(expr: &Expr) -> bool {
    match expr {
        Expr::Some { list, test, .. } | Expr::Count { list, test } | Expr::Find { list, test } => {
            stateless(list) && stateless(test)
        }
        Expr::Lookup(lookup) => {
            lookup.value.as_ref().is_none_or(stateless)
                && lookup.from.as_ref().is_none_or(stateless)
        }
        _ => true,
    }
}

/// For a report entry's test: the quick tests that run before anything stateful, so an entry they
/// fail can be skipped without changing what a later entry reads. `usize::MAX` stands for a test
/// that is itself one filtered `matches`.
pub(super) fn entry_quick_tests(when: &Expr) -> Vec<usize> {
    if quick_test(when) {
        return vec![usize::MAX];
    }
    let Expr::All(items) = when else {
        return Vec::new();
    };
    let mut tests = Vec::new();
    for (at, item) in items.iter().enumerate() {
        if quick_test(item) {
            tests.push(at);
        }
        if !stateless(item) {
            break;
        }
    }
    tests
}

fn quick_test(expr: &Expr) -> bool {
    matches!(expr, Expr::Matches { subject, pattern: Some(pattern), .. }
        if pattern.filters() && matches!(**subject, Expr::Field(_)))
}

/// Whether one of the quick tests of a rule's `when` fails for this record; a rule whose own test
/// fails reports nothing, so these may run first and in any order.
pub(super) fn quick_rejects(
    when: &Expr,
    tests: &[usize],
    records: &Records<'_>,
    subject: &Subject<'_, '_>,
) -> bool {
    tests.iter().any(|&at| {
        let item = match when {
            _ if at == usize::MAX => Some(when),
            Expr::All(items) => items.get(at),
            _ => None,
        };
        let Some(Expr::Matches {
            subject: field,
            pattern: Some(pattern),
            ..
        }) = item
        else {
            return false;
        };
        let Expr::Field(path) = &**field else {
            return false;
        };
        match resolve_path(records, subject, path) {
            Val::Str(text) => !pattern.may_match(&text),
            Val::Id(id) => !pattern.may_match(records.id_text(id)),
            // `matches` of anything but a text is false.
            _ => true,
        }
    })
}

/// A field path of the subject record outside a `some` (where `Item` has no value).
fn resolve_path<'a>(records: &Records<'a>, subject: &Subject<'_, 'a>, path: &[Seg]) -> Val<'a> {
    let mut found = match path.first() {
        Some(Seg::Field(name)) => records.subject_field(subject, *name),
        _ => return Val::Null,
    };
    for seg in path.iter().skip(1) {
        if found.is_null() {
            break;
        }
        found = records.step(&found, seg);
    }
    found
}

pub(super) fn field_expr(name: &str) -> Expr {
    Expr::Field(path(name))
}

/// The offset field that places a text field in the source (the `base` of C2-W-139).
fn base_path(text: &str) -> Option<Path> {
    let (prefix, last) = text.rsplit_once('.').map_or(("", text), |pair| pair);
    let base = match last {
        "body" | "raw_body" => "body_start",
        "text" => "start",
        "opener_text" => "opener_start",
        _ => return None,
    };
    Some(path(&if prefix.is_empty() {
        base.to_owned()
    } else {
        format!("{prefix}.{base}")
    }))
}

/// A value of the model: a literal, a text, or a reference to a record or list of the records.
#[derive(Clone, Debug)]
pub(super) enum Val<'a> {
    Null,
    Bool(bool),
    Num(i64),
    Str(Cow<'a, str>),
    /// The id of a leaf block, by range index; `None` is the empty id of a line without a leaf.
    Id(Option<u32>),
    Line(u32),
    /// The holder of a path, by holder index; `None` is the document.
    Owner(Option<u32>),
    Colon(u32),
    Ancestors(Option<u32>),
    Colons(u32),
    /// A node of the tree or a value inside it that is no scalar, by its handle (C2-W-140).
    Node(u32),
    /// The block or inline record of a range, as `item` of a lookup.
    Rec(u32),
    /// The owner of an attribute record, by index into the attribute table (C2-W-148).
    AttrOwner(u32),
    /// The types and the identities of the blocks that hold a block (C2-W-141).
    BlockAncestors(u32),
    AncestorIds(u32),
    /// The identities of the blocks of a line's `ancestors`, by its leaf (C2-W-150).
    LineAncestorIds(Option<u32>),
    /// An id record, by index into the id records (C2-W-150).
    IdRec(u32),
    /// The source layout entry of a block, by index into the layout entries (C2-L-009).
    Layout(u32),
    /// A list a lookup or the run gives.
    List(std::sync::Arc<[Val<'a>]>),
}

impl Val<'_> {
    pub fn num(value: usize) -> Self {
        Val::Num(value as i64)
    }

    fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }
}

pub(super) struct Eval<'r, 'a> {
    pub records: &'r Records<'a>,
    pub subject: Subject<'r, 'a>,
    pub line: &'a str,
    pub line_start: usize,
    pub lets: &'static [Expr],
    pub vars: Vec<Option<Val<'a>>>,
    pub item: Option<Val<'a>>,
    pub spans: Vec<(&'static str, usize, usize)>,
    pub base: usize,
}

pub(super) fn truth(value: &Val<'_>) -> bool {
    match value {
        Val::Null => false,
        Val::Bool(b) => *b,
        Val::Num(n) => *n != 0,
        Val::Str(s) => !s.is_empty(),
        Val::Id(id) => id.is_some(),
        _ => true,
    }
}

impl<'a> Eval<'_, 'a> {
    fn text(&self, value: &Val<'a>) -> Cow<'a, str> {
        match value {
            Val::Str(s) => s.clone(),
            Val::Num(n) => Cow::Owned(n.to_string()),
            Val::Bool(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
            Val::Id(id) => Cow::Borrowed(self.records.id_text(*id)),
            _ => Cow::Borrowed(""),
        }
    }

    /// The text of a value that is a string in the model: a text or a leaf id.
    fn str_of<'v>(&self, value: &'v Val<'a>) -> Option<&'v str> {
        match value {
            Val::Str(s) => Some(s),
            Val::Id(id) => Some(self.records.id_text(*id)),
            _ => None,
        }
    }

    fn number(&self, value: &Val<'a>) -> i64 {
        match value {
            Val::Num(n) => *n,
            Val::Bool(b) => i64::from(*b),
            Val::Str(s) => s.parse().unwrap_or(0),
            Val::Id(id) => self.records.id_text(*id).parse().unwrap_or(0),
            _ => 0,
        }
    }

    fn same(&self, left: &Val<'a>, right: &Val<'a>) -> bool {
        match (left, right) {
            (Val::Null, Val::Null) => true,
            (Val::Bool(a), Val::Bool(b)) => a == b,
            (Val::Num(a), Val::Num(b)) => a == b,
            (Val::Str(a), Val::Str(b)) => a == b,
            (Val::Id(id), Val::Str(s)) | (Val::Str(s), Val::Id(id)) => {
                self.records.id_text(*id) == s.as_ref()
            }
            (Val::Line(a), Val::Line(b)) | (Val::Colon(a), Val::Colon(b)) => a == b,
            (Val::Owner(a), Val::Owner(b)) | (Val::Id(a), Val::Id(b)) => a == b,
            _ => false,
        }
    }

    fn resolve(&self, path: &[Seg]) -> Val<'a> {
        let mut found = match path.first() {
            Some(Seg::Item) => self.item.clone().unwrap_or(Val::Null),
            Some(Seg::Field(name)) => self.records.subject_field(&self.subject, *name),
            _ => Val::Null,
        };
        for seg in path.iter().skip(1) {
            if found.is_null() {
                break;
            }
            found = self.records.step(&found, seg);
        }
        found
    }

    fn matched(&mut self, subject: &Expr, pattern: Option<&Pattern>, base: Option<&Path>) -> bool {
        let Some(pattern) = pattern else {
            return false;
        };
        let value = self.value(subject);
        let Some(haystack) = self.str_of(&value) else {
            return false;
        };
        if pattern.strip_quotes && !haystack.contains('\n') {
            let (start, end) = strip_quotes(haystack);
            self.spans.clear();
            self.spans
                .extend(pattern.groups.iter().map(|&(name, _)| (name, start, end)));
            self.base = base.map_or(0, |path| self.number(&self.resolve(path)).max(0) as usize);
            return true;
        }
        if !pattern.may_match(haystack) {
            return false;
        }
        // `captures` allocates its slots, so it runs only once a match is known to exist.
        let found = pattern.re.is_match(haystack);
        if pattern.groups.is_empty() || !found {
            return found;
        }
        let Some(found) = pattern.re.captures(haystack) else {
            return false;
        };
        self.spans.clear();
        for &(name, index) in &pattern.groups {
            if let Some(span) = found.get(index) {
                self.spans.push((name, span.start(), span.end()));
            }
        }
        self.base = base.map_or(0, |path| self.number(&self.resolve(path)).max(0) as usize);
        true
    }

    fn variable(&mut self, index: usize) -> Val<'a> {
        if self.vars.len() < self.lets.len() {
            self.vars.resize(self.lets.len(), None);
        }
        if let Some(Some(value)) = self.vars.get(index) {
            return value.clone();
        }
        let Some(expression) = self.lets.get(index) else {
            return Val::Null;
        };
        let spans = self.spans.clone();
        let base = self.base;
        let value = self.value(expression);
        self.spans = spans;
        self.base = base;
        if let Some(slot) = self.vars.get_mut(index) {
            *slot = Some(value.clone());
        }
        value
    }

    fn order(&mut self, op: Cmp, left: &Expr, right: &Expr) -> bool {
        let left = self.value(left);
        let owned;
        let right: &Val<'a> = if let Expr::Lit(literal) = right {
            literal
        } else {
            owned = self.value(right);
            &owned
        };
        match op {
            Cmp::Eq => return self.same(&left, right),
            Cmp::Ne => return !self.same(&left, right),
            _ => {}
        }
        if left.is_null() || right.is_null() {
            return false;
        }
        let (left, right) = (self.number(&left), self.number(right));
        match op {
            Cmp::Lt => left < right,
            Cmp::Le => left <= right,
            Cmp::Gt => left > right,
            _ => left >= right,
        }
    }

    fn contains(&mut self, left: &Expr, right: &Expr) -> bool {
        let haystack = self.value(left);
        let needle = self.value(right);
        if let Val::Ancestors(leaf) = haystack {
            return self.str_of(&needle).is_some_and(|n| {
                matches!(needle, Val::Str(_)) && self.records.ancestors(leaf).any(|kind| kind == n)
            });
        }
        if self.records.is_list(&haystack) {
            return (0..self.records.list_len(&haystack))
                .any(|at| self.same(&self.records.list_item(&haystack, at), &needle));
        }
        self.str_of(&haystack)
            .zip(self.str_of(&needle))
            .is_some_and(|(s, n)| holds(s, n))
    }

    fn some(&mut self, list: &Expr, from: usize, test: &Expr) -> bool {
        let list = self.value(list);
        let before = self.item.take();
        let mut result = false;
        for index in from..self.records.list_len(&list) {
            self.item = Some(self.records.list_item(&list, index));
            if self.test(test) {
                result = true;
                break;
            }
        }
        self.item = before;
        result
    }

    fn lookup(&mut self, lookup: &Lookup) -> Val<'a> {
        let id = lookup.site;
        if !self.records.has_lookup(id) {
            let built = self.build_lookup(lookup);
            self.records.store_lookup(id, built);
        }
        let value = match (&lookup.key, &lookup.value) {
            (Some(_), Some(value)) => self.value(value),
            _ => Val::Null,
        };
        let key = LookupKey::of(&value);
        if lookup.all {
            let found: Vec<Val<'a>> = self
                .records
                .lookup_all(id, &key)
                .into_iter()
                .map(|at| resolve_from(self.records, lookup.record(at), &lookup.field))
                .collect();
            return Val::List(found.into());
        }
        let start = match &lookup.from {
            Some(from) => self.value(from),
            None => self.records.subject_field(&self.subject, F::Start),
        };
        let start = self.number(&start).max(0) as usize;
        let side = if lookup.before {
            -1
        } else {
            i8::from(lookup.after)
        };
        self.records
            .lookup_at(id, &key, start, side)
            .map_or(Val::Null, |at| {
                resolve_from(self.records, lookup.record(at), &lookup.field)
            })
    }

    /// The records of a lookup in document order with their starts, by key (C2-W-141).
    fn build_lookup(&mut self, lookup: &Lookup) -> LookupIndex {
        let saved = (self.item.take(), std::mem::take(&mut self.spans));
        let mut index = LookupIndex::default();
        for (at, start) in self.records.lookup_records(lookup.over, &lookup.types) {
            let key = match &lookup.key {
                Some(key) => {
                    self.item = Some(lookup.record(at));
                    self.value(key)
                }
                None => Val::Null,
            };
            index
                .keys
                .entry(LookupKey::of(&key))
                .or_default()
                .push((start, at));
        }
        (self.item, self.spans) = saved;
        index
    }

    fn find(&mut self, list: &Expr, test: &Expr) -> Val<'a> {
        let list = self.value(list);
        let before = self.item.take();
        let mut found = Val::Null;
        for index in 0..self.records.list_len(&list) {
            let item = self.records.list_item(&list, index);
            self.item = Some(item.clone());
            if self.test(test) {
                found = item;
                break;
            }
        }
        self.item = before;
        found
    }

    fn count(&mut self, list: &Expr, test: &Expr) -> usize {
        let list = self.value(list);
        let before = self.item.take();
        let mut found = 0;
        for index in 0..self.records.list_len(&list) {
            self.item = Some(self.records.list_item(&list, index));
            found += usize::from(self.test(test));
        }
        self.item = before;
        found
    }

    fn shift(&mut self, left: bool, position: &Expr, chars: &Expr) -> Val<'a> {
        let position = self.value(position);
        let mut at = self.number(&position).max(0) as usize;
        let chars = self.value(chars);
        let chars = self.text(&chars);
        let bytes = self.line.as_bytes();
        if left {
            while at > self.line_start
                && bytes
                    .get(at - self.line_start - 1)
                    .is_some_and(|b| chars.as_bytes().contains(b))
            {
                at -= 1;
            }
        } else {
            while bytes
                .get(at.saturating_sub(self.line_start))
                .is_some_and(|b| chars.as_bytes().contains(b))
            {
                at += 1;
            }
        }
        Val::num(at)
    }

    fn slice(&mut self, text: &Expr, from: &Expr, to: Option<&Expr>) -> Val<'a> {
        let text = self.value(text);
        let text = self.text(&text);
        let from = self.value(from);
        let from = self.number(&from).max(0) as usize;
        let to = to.map_or(text.len(), |to| {
            let to = self.value(to);
            self.number(&to).max(0) as usize
        });
        let range = from.min(text.len())..to.min(text.len());
        Val::Str(match text {
            Cow::Borrowed(s) => Cow::Borrowed(s.get(range).unwrap_or("")),
            Cow::Owned(s) => Cow::Owned(s.get(range).unwrap_or("").to_owned()),
        })
    }

    pub fn value(&mut self, expr: &Expr) -> Val<'a> {
        match expr {
            Expr::Lit(value) => value.clone(),
            Expr::Field(path) => self.resolve(path),
            Expr::Var(index) => self.variable(*index),
            Expr::Has(path) => Val::Bool(!self.resolve(path).is_null()),
            Expr::Count { list, test } => Val::num(self.count(list, test)),
            Expr::Find { list, test } => self.find(list, test),
            Expr::Nth(args) => {
                let list = self.value(&args.0);
                let at = self.value(&args.1);
                let at = self.number(&at);
                let len = self.records.list_len(&list) as i64;
                let at = if at < 0 { len + at } else { at };
                if at < 0 {
                    Val::Null
                } else {
                    self.records.list_item(&list, at as usize)
                }
            }
            Expr::Join(args) => {
                let list = self.value(&args.0);
                let glue = self.string(&args.1);
                let mut out = String::new();
                for at in 0..self.records.list_len(&list) {
                    if at > 0 {
                        out.push_str(&glue);
                    }
                    let item = self.records.list_item(&list, at);
                    out.push_str(&self.text(&item));
                }
                Val::Str(Cow::Owned(out))
            }
            Expr::Lower(arg) => Val::Str(Cow::Owned(self.string(arg).to_lowercase())),
            Expr::PercentDecode(arg) => {
                let text = self.string(arg);
                Val::Str(Cow::Owned(
                    percent_decode(&text).unwrap_or_else(|| text.into_owned()),
                ))
            }
            Expr::Column(args) => {
                let at = self.value(&args.0);
                let quotes = self.value(&args.1);
                if at.is_null() {
                    return Val::Null;
                }
                let (at, quotes) = (self.number(&at).max(0), self.number(&quotes).max(0));
                Val::num(self.records.column_at(at as usize, quotes as usize))
            }
            Expr::RunExtensions => self.records.extensions(),
            Expr::Lookup(lookup) => self.lookup(lookup),
            Expr::IndexOf(args) => {
                let haystack = self.value(&args.0);
                if self.records.is_list(&haystack) {
                    let needle = self.value(&args.1);
                    return Val::Num(
                        (0..self.records.list_len(&haystack))
                            .find(|&at| self.same(&self.records.list_item(&haystack, at), &needle))
                            .map_or(-1, |at| at as i64),
                    );
                }
                let text = self.text(&haystack);
                let needle = self.string(&args.1);
                Val::Num(text.find(&*needle).map_or(-1, |at| at as i64))
            }
            Expr::Match { group, end } => self
                .spans
                .iter()
                .find(|(name, _, _)| name == group)
                .map_or(Val::Null, |(_, start, stop)| {
                    Val::num(self.base + if *end { *stop } else { *start })
                }),
            Expr::Len(arg) => {
                let value = self.value(arg);
                match value {
                    Val::Str(_) => Val::num(self.str_of(&value).map_or(0, str::len)),
                    Val::Null => Val::Null,
                    list => Val::num(self.records.list_len(&list)),
                }
            }
            Expr::JsonEscape(arg) => {
                let text = self.string(arg);
                let quoted = serde_json::to_string(&*text).unwrap_or_default();
                Val::Str(Cow::Owned(quoted[1..quoted.len() - 1].to_owned()))
            }
            // Arithmetic on an absent value is absent, so a comparison with it does not hold (C2-W-141).
            Expr::Add(args) => {
                let mut sum = 0;
                for arg in args {
                    let value = self.value(arg);
                    if value.is_null() {
                        return Val::Null;
                    }
                    sum += self.number(&value);
                }
                Val::Num(sum)
            }
            Expr::Sub(args) => {
                let left = self.value(&args.0);
                let right = self.value(&args.1);
                if left.is_null() || right.is_null() {
                    return Val::Null;
                }
                Val::Num(self.number(&left) - self.number(&right))
            }
            Expr::Slice(args) => self.slice(&args.0, &args.1, args.2.as_ref()),
            Expr::Concat(args) => {
                let mut out = String::new();
                for arg in args {
                    let value = self.value(arg);
                    out.push_str(&self.text(&value));
                }
                Val::Str(Cow::Owned(out))
            }
            Expr::TrimEnd(arg) => {
                let value = self.value(arg);
                Val::Str(match self.text(&value) {
                    Cow::Borrowed(s) => Cow::Borrowed(s.trim_end_matches([' ', '\t'])),
                    Cow::Owned(s) => Cow::Owned(s.trim_end_matches([' ', '\t']).to_owned()),
                })
            }
            Expr::Words(arg) => {
                let value = self.value(arg);
                Val::Str(Cow::Owned(self.text(&value).replace('_', " ")))
            }
            Expr::TextFrom(arg) => {
                let value = self.value(arg);
                let from = self.number(&value).max(0) as usize;
                Val::Str(Cow::Borrowed(
                    self.line
                        .get(from.saturating_sub(self.line_start)..)
                        .unwrap_or("")
                        .trim_end_matches([' ', '\t']),
                ))
            }
            Expr::Shift { left, args } => self.shift(*left, &args.0, &args.1),
            Expr::If(args) => {
                if self.test(&args.0) {
                    self.value(&args.1)
                } else {
                    self.value(&args.2)
                }
            }
            _ => Val::Bool(self.test(expr)),
        }
    }

    pub fn test(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::All(args) => args.iter().all(|arg| self.test(arg)),
            Expr::Any(args) => args.iter().any(|arg| self.test(arg)),
            Expr::Not(arg) => !self.test(arg),
            Expr::Cmp(op, args) => self.order(*op, &args.0, &args.1),
            Expr::In(subject, members) => {
                let value = self.value(subject);
                members.iter().any(|member| self.same(&value, member))
            }
            Expr::Contains(args) => self.contains(&args.0, &args.1),
            Expr::Starts(args) => {
                let haystack = self.value(&args.0);
                let prefix = self.value(&args.1);
                self.str_of(&haystack)
                    .zip(self.str_of(&prefix))
                    .is_some_and(|(s, p)| s.starts_with(p))
            }
            Expr::Matches {
                subject,
                pattern,
                base,
            } => self.matched(subject, pattern.as_ref(), base.as_ref()),
            Expr::Some { list, from, test } => self.some(list, *from, test),
            Expr::AttributeLine(arg) => {
                let text = self.string(arg);
                super::lines::valid_attribute_line(&text)
            }
            Expr::Within { field, test } => {
                let first = match &**test {
                    Expr::Matches {
                        pattern: Some(pattern),
                        ..
                    } => pattern.first.as_deref(),
                    _ => None,
                };
                let held = self.resolve(field);
                let possible = match (first, self.str_of(&held)) {
                    (Some(first), Some(text)) => text.bytes().any(|b| first[b as usize]),
                    _ => true,
                };
                possible && self.test(test)
            }
            Expr::ShapeIs(name) => match self.subject {
                Subject::Line(index) => self.records.line_shape_is(index, name),
                _ => false,
            },
            _ => {
                let value = self.value(expr);
                truth(&value)
            }
        }
    }

    pub fn offset(&mut self, expr: Option<&Expr>) -> usize {
        expr.map_or(0, |expr| {
            let value = self.value(expr);
            self.number(&value).max(0) as usize
        })
    }

    pub fn string(&mut self, expr: &Expr) -> Cow<'a, str> {
        let value = self.value(expr);
        self.text(&value)
    }
}

/// A key of a lookup, owned so the index outlives the values it was built from.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) enum LookupKey {
    Null,
    Bool(bool),
    Num(i64),
    Str(String),
}

impl LookupKey {
    fn of(value: &Val<'_>) -> Self {
        match value {
            Val::Bool(b) => Self::Bool(*b),
            Val::Num(n) => Self::Num(*n),
            Val::Str(text) => Self::Str(text.to_string()),
            _ => Self::Null,
        }
    }
}

/// The index of one lookup for one call: the starts and records per key, and for `before` those of
/// every record, each in document order.
#[derive(Default, Debug)]
pub(super) struct LookupIndex {
    pub keys: std::collections::HashMap<LookupKey, Vec<(usize, u32)>>,
}

/// The index site of a lookup: one per kind of records, types and key, numbered as the model compiles.
/// A key that reads anything but its `item` is the rule's own, so it gets a site of its own.
fn lookup_site(arg: &'static Value) -> usize {
    fn item_only(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                map.get("f")
                    .and_then(Value::as_str)
                    .is_none_or(|field| field == "item" || field.starts_with("item."))
                    && !map.contains_key("v")
                    && !map.contains_key("match")
                    && map.values().all(item_only)
            }
            Value::Array(items) => items.iter().all(item_only),
            _ => true,
        }
    }
    static SITES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let key = &arg["key"];
    let signature = if !item_only(key) {
        format!("{:p}", std::ptr::from_ref(arg))
    } else {
        format!("{} {} {}", arg["over"], arg["types"], key)
    };
    let mut sites = SITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    sites
        .iter()
        .position(|known| *known == signature)
        .unwrap_or_else(|| {
            sites.push(signature);
            sites.len() - 1
        })
}

/// The field a `let` value is a slice of, where it is that slice or the empty text: `slice` of a field,
/// or `if` whose branches are such a slice and `""`.
fn sliced_field(value: &'static Value) -> Option<&'static str> {
    if let Some(args) = value.get("slice").and_then(Value::as_array) {
        return args.first()?.get("f")?.as_str();
    }
    let args = value.get("if")?.as_array()?;
    if args.get(2)?.as_str() != Some("") {
        return None;
    }
    sliced_field(args.get(1)?)
}

/// `%HH` read as bytes and the bytes as UTF-8; `None` where a `%` is no escape or the bytes are no UTF-8 (C2-W-150).
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            out.push(
                u8::from_str_radix(hex, 16)
                    .ok()
                    .filter(|_| hex.bytes().all(|b| b.is_ascii_hexdigit()))?,
            );
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn resolve_from<'a>(records: &Records<'a>, mut found: Val<'a>, path: &[Seg]) -> Val<'a> {
    for seg in path {
        if found.is_null() {
            break;
        }
        found = records.step(&found, seg);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::{STRIP_QUOTES, pattern, strip_quotes};

    #[test]
    fn branches_give_the_groups_the_whole_pattern_gives() {
        let text = r"(?<link>\[[^\]\n]+\][ \t]+\((?<dest>[^)\n]*)\))|\[[^\]\n]+\][ \t]+\([^)\n]*|\[[^\]\n]*";
        let compiled = pattern(text, "").expect("pattern");
        let branches = compiled.branches.as_ref().expect("branches");
        let parts = ["[", "]", "a", " ", "(", ")", "\t", "[a] ("];
        let mut inputs = vec![String::new()];
        for _ in 0..5 {
            inputs = inputs
                .iter()
                .flat_map(|head| parts.iter().map(move |part| format!("{head}{part}")))
                .collect();
        }
        for input in &inputs {
            let whole: Vec<Vec<Option<(usize, usize)>>> = compiled
                .re
                .captures_iter(input)
                .map(|caps| {
                    compiled
                        .groups
                        .iter()
                        .map(|&(_, index)| caps.get(index).map(|m| (m.start(), m.end())))
                        .collect()
                })
                .collect();
            let split: Vec<Vec<Option<(usize, usize)>>> = compiled
                .re
                .find_iter(input)
                .map(|found| {
                    let span = found.as_str();
                    let (re, groups) = branches
                        .iter()
                        .find(|(re, _)| re.is_match(span))
                        .expect("a branch");
                    let caps = re.captures(span);
                    compiled
                        .groups
                        .iter()
                        .map(|&(name, _)| {
                            let index = groups.iter().find(|(own, _)| *own == name)?.1;
                            let group = caps.as_ref()?.get(index)?;
                            Some((found.start() + group.start(), found.start() + group.end()))
                        })
                        .collect()
                })
                .collect();
            assert_eq!(whole, split, "{input:?}");
        }
        assert_eq!(inputs.len(), 32_768);
    }

    #[test]
    fn strip_quotes_reads_the_group_the_regex_reads() {
        let re = pattern(STRIP_QUOTES, "").expect("pattern").re;
        let parts = [
            "", " ", "\t", ">", "> ", ">\t", ":", "a", " >", "x ", "\u{e9}", "\0",
        ];
        let mut checked = 0;
        for a in parts {
            for b in parts {
                for c in parts {
                    for d in parts {
                        let text = format!("{a}{b}{c}{d}");
                        let found = re.captures(&text).and_then(|caps| caps.get(1));
                        let expected = found.map(|group| (group.start(), group.end()));
                        assert_eq!(Some(strip_quotes(&text)), expected, "{text:?}");
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 12 * 12 * 12 * 12);
    }
}
