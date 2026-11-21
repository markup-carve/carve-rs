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
    Unknown,
}

const FIELDS: &[(&str, F)] = &[
    ("prev", F::Prev),
    ("prev2", F::Prev2),
    ("next", F::Next),
    ("next2", F::Next2),
    ("leaf_head", F::LeafHead),
    ("leaf_next", F::LeafNext),
    ("ancestors", F::Ancestors),
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
    ("content_column", F::ContentColumn),
    ("marker_column", F::MarkerColumn),
    ("width", F::Width),
    ("kind", F::Kind),
];

fn field_id(name: &str) -> F {
    FIELDS
        .iter()
        .find(|(known, _)| *known == name)
        .map_or(F::Unknown, |(_, id)| *id)
}

/// One step of a dotted field path.
#[derive(Clone, Copy, Debug)]
pub(super) enum Seg {
    Item,
    Field(F),
    At(usize),
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
            } else {
                Seg::Field(field_id(part))
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
    pub groups: Vec<&'static str>,
    /// For a pattern anchored at the start that cannot match the empty text: the bytes a match starts with.
    pub first: Option<Box<[bool; 256]>>,
    /// Bytes of which every match holds at least one.
    needs: Option<Box<[bool; 256]>>,
}

impl Pattern {
    /// Whether the text can match at all, from its first byte; a quick test before the regex runs.
    pub fn may_match(&self, text: &str) -> bool {
        self.first.as_ref().is_none_or(|first| {
            text.as_bytes()
                .first()
                .is_some_and(|byte| first[usize::from(*byte)])
        }) && self
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
    let re = RegexBuilder::new(&text.replace("(?<", "(?P<"))
        .case_insensitive(flags.contains('i'))
        .build()
        .ok()?;
    // The names live as long as the compiled model, which is never dropped.
    let groups = re
        .capture_names()
        .flatten()
        .map(|name| &*Box::leak(name.to_owned().into_boxed_str()))
        .collect();
    let hir = regex_syntax::ParserBuilder::new()
        .case_insensitive(flags.contains('i'))
        .build()
        .parse(&text.replace("(?<", "(?P<"))
        .ok();
    let mut set = Box::new([false; 256]);
    let first = hir
        .as_ref()
        .filter(|hir| anchored_first(hir, &mut set))
        .map(|_| set);
    let needs = hir.as_ref().and_then(needed_bytes).map(|bytes| {
        let mut set = Box::new([false; 256]);
        for byte in bytes {
            set[usize::from(byte)] = true;
        }
        set
    });
    Some(Pattern {
        re,
        groups,
        first,
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
        list: Path,
        from: usize,
        test: Box<Self>,
    },
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
            return Expr::Cmp(*cmp, pair(0, 1));
        }
        match op.as_str() {
            "f" => Expr::Field(path(arg.as_str().unwrap_or(""))),
            "v" => self.variable(arg.as_str().unwrap_or("")),
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
        Expr::Matches {
            subject: Box::new(self.compile(subject)),
            pattern: compiled,
            base: subject.get("f").and_then(Value::as_str).and_then(base_path),
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
                list: path(arg.get("in").and_then(Value::as_str).unwrap_or("")),
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
    pub flags: &'k [(F, bool)],
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
            Expr::Cmp(op @ (Cmp::Eq | Cmp::Ne), args) => self
                .equal(&args.0, literal_text(&args.1)?, over_line)
                .map(|same| same == matches!(op, Cmp::Eq)),
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
            _ => None,
        }
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
    fn str_of(&self, value: &Val<'a>) -> Option<Cow<'a, str>> {
        match value {
            Val::Str(s) => Some(s.clone()),
            Val::Id(id) => Some(Cow::Borrowed(self.records.id_text(*id))),
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
            found = match seg {
                Seg::Field(name) => self.records.field(&found, *name),
                Seg::At(index) => self.records.list_item(&found, *index),
                Seg::Item => Val::Null,
            };
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
        if !pattern.may_match(&haystack) {
            return false;
        }
        // `captures` allocates its slots, so it runs only once a match is known to exist.
        let found = pattern.re.is_match(&haystack);
        if pattern.groups.is_empty() || !found {
            return found;
        }
        let Some(found) = pattern.re.captures(&haystack) else {
            return false;
        };
        self.spans.clear();
        for &name in &pattern.groups {
            if let Some(span) = found.name(name) {
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
        let right = self.value(right);
        match op {
            Cmp::Eq => return self.same(&left, &right),
            Cmp::Ne => return !self.same(&left, &right),
            _ => {}
        }
        if left.is_null() || right.is_null() {
            return false;
        }
        let (left, right) = (self.number(&left), self.number(&right));
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
        self.str_of(&haystack)
            .zip(self.str_of(&needle))
            .is_some_and(|(s, n)| s.contains(n.as_ref()))
    }

    fn some(&mut self, list: &Path, from: usize, test: &Expr) -> bool {
        let list = self.resolve(list);
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
            Expr::Match { group, end } => self
                .spans
                .iter()
                .find(|(name, _, _)| name == group)
                .map_or(Val::Null, |(_, start, stop)| {
                    Val::num(self.base + if *end { *stop } else { *start })
                }),
            Expr::Len(arg) => {
                let value = self.value(arg);
                Val::num(self.str_of(&value).map_or(0, |s| s.len()))
            }
            Expr::Add(args) => {
                let mut sum = 0;
                for arg in args {
                    let value = self.value(arg);
                    sum += self.number(&value);
                }
                Val::Num(sum)
            }
            Expr::Sub(args) => {
                let left = self.value(&args.0);
                let right = self.value(&args.1);
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
                    .is_some_and(|(s, p)| s.starts_with(p.as_ref()))
            }
            Expr::Matches {
                subject,
                pattern,
                base,
            } => self.matched(subject, pattern.as_ref(), base.as_ref()),
            Expr::Some { list, from, test } => self.some(list, *from, test),
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
