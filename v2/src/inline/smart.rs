//! Smart typography (C2-I-104 to C2-I-117) over the plain text of the flow.
//! Every match is a `smart_punctuation` node; the tables come from
//! `smart-punctuation.json`.

use super::delim::Ctx;
use crate::chars::{is_flank_space, is_word};
use crate::tables;

/// What stands before the current position for the quote rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prev {
    /// The start of the content or of a node's content (C2-I-104).
    Start,
    /// The end of a closed inline node, which counts as a word character.
    Word,
    Char(char),
}

/// One piece of a plain text run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    Text(usize, usize),
    Punct {
        v0: usize,
        v1: usize,
        kind: &'static str,
        glyph: &'static str,
    },
}

/// The state kept per content (C2-I-109).
pub struct Smart {
    pub prev: Prev,
    pub single_open: bool,
}

const DASHES: &str = "-\u{2013}\u{2014}";
const OPEN_QUOTES: &str = "\u{201c}\u{2018}";

fn quote_row(kind: &str) -> (&'static str, &'static str) {
    tables::SMART_QUOTES
        .iter()
        .find(|r| r.0 == kind)
        .map_or(("", ""), |r| (r.0, r.2))
}

fn dash_row(kind: &str) -> (&'static str, &'static str, usize) {
    tables::SMART_DASHES
        .iter()
        .find(|r| r.0 == kind)
        .map_or(("", "", 0), |r| (r.0, r.2, r.1.len()))
}

impl Smart {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            prev: Prev::Start,
            single_open: false,
        }
    }

    fn prev_space(&self) -> bool {
        match self.prev {
            Prev::Start => true,
            Prev::Word => false,
            Prev::Char(c) => is_flank_space(c),
        }
    }

    fn prev_word(&self) -> bool {
        match self.prev {
            Prev::Word => true,
            Prev::Start => false,
            Prev::Char(c) => is_word(c),
        }
    }

    /// Splits the plain text `v0..v1` of `ctx.t` into text and punctuation.
    pub fn run(&mut self, ctx: &Ctx<'_>, v0: usize, v1: usize, out: &mut Vec<Out>) {
        let t = ctx.t;
        let mut p = v0;
        let mut text_from = v0;
        // GAP INL-012: the exact-run tests see only unreplaced text of this run.
        let mut plain_before: Option<char> = match self.prev {
            Prev::Char(c) => Some(c),
            _ => None,
        };
        while p < v1 {
            let Some(rest) = t.get(p..v1) else { break };
            let Some(c) = rest.chars().next() else { break };
            if !starts_smart(c) {
                let n = rest.find(starts_smart).unwrap_or(rest.len());
                p += n;
                if let Some(last) = rest.get(..n).and_then(|s| s.chars().next_back()) {
                    self.prev = Prev::Char(last);
                    plain_before = Some(last);
                }
                continue;
            }
            let found = self.at(ctx, p, v1, c, plain_before);
            match found {
                Step::Text(len) => {
                    p += len;
                    if let Some(last) = t.get(..p).and_then(|s| s.chars().next_back()) {
                        self.prev = Prev::Char(last);
                        plain_before = Some(last);
                    }
                }
                Step::One(len, kind, glyph) => {
                    if text_from < p {
                        out.push(Out::Text(text_from, p));
                    }
                    self.punct(out, &mut p, [(len, kind, glyph)]);
                    plain_before = None;
                    text_from = p;
                }
                Step::Punct(list) => {
                    if text_from < p {
                        out.push(Out::Text(text_from, p));
                    }
                    self.punct(out, &mut p, list);
                    plain_before = None;
                    text_from = p;
                }
            }
        }
        if text_from < v1 {
            out.push(Out::Text(text_from, v1));
        }
    }

    fn punct(
        &mut self,
        out: &mut Vec<Out>,
        p: &mut usize,
        list: impl IntoIterator<Item = (usize, &'static str, &'static str)>,
    ) {
        for (len, kind, glyph) in list {
            out.push(Out::Punct {
                v0: *p,
                v1: *p + len,
                kind,
                glyph,
            });
            *p += len;
            self.prev = glyph.chars().next().map_or(Prev::Word, Prev::Char);
        }
    }

    fn at(&mut self, ctx: &Ctx<'_>, p: usize, v1: usize, c: char, before: Option<char>) -> Step {
        let t = ctx.t;
        if let Some((len, row)) = run_at(t, p, v1, before) {
            return match row {
                Some((kind, glyph)) => Step::one(len, kind, glyph),
                None => Step::Text(len),
            };
        }
        match c {
            '-' => dashes(t, p, v1),
            '"' | '\'' => {
                let kind = self.quote(ctx, p, c);
                let (kind, glyph) = quote_row(kind);
                Step::one(1, kind, glyph)
            }
            c => Step::Text(c.len_utf8()),
        }
    }

    /// C2-I-104 to C2-I-109: the kind of the quote at `p`.
    fn quote(&mut self, ctx: &Ctx<'_>, p: usize, q: char) -> &'static str {
        let next = ctx.after(p + 1);
        let single = q == '\'';
        if single && next.is_some_and(|n| n.is_ascii_digit()) {
            return "right_single_quote";
        }
        let open = self.quote_opens(next);
        if !single {
            return if open {
                "left_double_quote"
            } else {
                "right_double_quote"
            };
        }
        self.single_quote(ctx, p, next, open)
    }

    fn quote_opens(&self, next: Option<char>) -> bool {
        let dashlike = matches!(self.prev, Prev::Char(c) if DASHES.contains(c) || c == '/');
        let closes_after_dash = next.is_none_or(|n| {
            is_flank_space(n) || tables::QUOTE_CLOSES_AFTER_DASH_BEFORE_CHARS.contains(n)
        });
        let opens_ctx = match self.prev {
            Prev::Start => true,
            Prev::Word => false,
            Prev::Char(c) => {
                is_flank_space(c)
                    || tables::QUOTE_OPENS_AFTER_CHARS.contains(c)
                    || DASHES.contains(c)
                    || OPEN_QUOTES.contains(c)
            }
        };
        opens_ctx && !(dashlike && closes_after_dash)
    }

    fn single_quote(
        &mut self,
        ctx: &Ctx<'_>,
        p: usize,
        next: Option<char>,
        open: bool,
    ) -> &'static str {
        if open {
            if let Some(quoted) = apostrophe_word(ctx.t, p + 1) {
                if !quoted {
                    return "right_single_quote";
                }
            } else if self.single_open && next.is_some_and(is_word) {
                return "right_single_quote";
            }
            self.single_open = true;
            return "left_single_quote";
        }
        let inside_word = self.prev_word() && next.is_some_and(is_word);
        if !self.prev_space() && !inside_word {
            self.single_open = false;
        }
        "right_single_quote"
    }
}

impl Default for Smart {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `c` can begin a replacement: a quote, a hyphen or the first
/// character of a replaced run. Any other character is text as it stands.
fn starts_smart(c: char) -> bool {
    match u8::try_from(c) {
        Ok(b) => ASCII_STARTS.get(usize::from(b)).copied().unwrap_or(false),
        Err(_) => *WIDE_STARTS && starts_smart_slow(c),
    }
}

fn starts_smart_slow(c: char) -> bool {
    matches!(c, '-' | '"' | '\'') || tables::SMART_RUNS.iter().any(|r| r.1 && r.0.starts_with(c))
}

/// Whether any replaced run begins outside ASCII.
static WIDE_STARTS: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    tables::SMART_RUNS
        .iter()
        .any(|r| r.1 && r.0.chars().next().is_some_and(|c| !c.is_ascii()))
});

static ASCII_STARTS: std::sync::LazyLock<[bool; 128]> = std::sync::LazyLock::new(|| {
    let mut t = [false; 128];
    for (b, slot) in (0u8..).zip(t.iter_mut()) {
        *slot = starts_smart_slow(char::from(b));
    }
    t
});

/// C2-I-107, C2-I-108: `Some(quoted)` when an apostrophe word follows `i`.
fn apostrophe_word(t: &str, i: usize) -> Option<bool> {
    let rest = t.get(i..)?;
    let len = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
    let word = rest.get(..len)?.to_ascii_lowercase();
    if len == 0 || !tables::APOSTROPHE_WORDS.contains(&word.as_str()) {
        return None;
    }
    let mut after = rest.get(len..)?.chars();
    match after.next() {
        Some(c) if !c.is_ascii() && c.is_alphabetic() => None,
        Some('\'') if word != "n" => Some(!after.next().is_some_and(is_word)),
        _ => Some(false),
    }
}

enum Step {
    Text(usize),
    One(usize, &'static str, &'static str),
    Punct(Vec<(usize, &'static str, &'static str)>),
}

impl Step {
    const fn one(len: usize, kind: &'static str, glyph: &'static str) -> Self {
        Self::One(len, kind, glyph)
    }
}

/// C2-I-114 to C2-I-117: the longest run of the table at `p`, with the exact
/// run rule (C2-I-116) for hyphens and for `+-`.
fn run_at(
    t: &str,
    p: usize,
    v1: usize,
    before: Option<char>,
) -> Option<(usize, Option<(&'static str, &'static str)>)> {
    let rest = t.get(p..v1)?;
    let mut best: Option<(usize, Option<(&'static str, &'static str)>)> = None;
    for &(src, replaced, kind, glyph) in tables::SMART_RUNS {
        // GAP INL-014: a run that is not replaced (`->`) takes no characters.
        if !replaced || !rest.starts_with(src) || best.is_some_and(|(l, _)| l >= src.len()) {
            continue;
        }
        let edge: &str = if src == "+-" { "+-" } else { "-" };
        // GAP INL-012: `+-` reads the source after it, an arrow only its own text.
        let after = if src == "+-" {
            t.get(p + src.len()..)
        } else {
            rest.get(src.len()..)
        }
        .and_then(|s| s.chars().next());
        let first = src.chars().next().unwrap_or(' ');
        let last = src.chars().next_back().unwrap_or(' ');
        if (edge.contains(first) && before.is_some_and(|b| edge.contains(b)))
            || (edge.contains(last) && after.is_some_and(|a| edge.contains(a)))
        {
            continue;
        }
        best = Some((src.len(), replaced.then_some((kind, glyph))));
    }
    best
}

/// C2-I-111: a hyphen run of two or more becomes dashes.
fn dashes(t: &str, p: usize, v1: usize) -> Step {
    let n = t
        .get(p..v1)
        .map_or(0, |s| s.bytes().take_while(|&b| b == b'-').count());
    let (em_kind, em_glyph, em_len) = dash_row("em_dash");
    let (half_kind, half_glyph, half_len) = dash_row("en_dash");
    if n < half_len.min(em_len) || em_len == 0 || half_len == 0 {
        return Step::Text(1);
    }
    let mut list = Vec::new();
    let mut left = n;
    loop {
        if left % em_len == 0 {
            list.extend((0..left / em_len).map(|_| (em_len, em_kind, em_glyph)));
            break;
        }
        if left % half_len == 0 {
            list.extend((0..left / half_len).map(|_| (half_len, half_kind, half_glyph)));
            break;
        }
        if left < em_len {
            break;
        }
        list.push((em_len, em_kind, em_glyph));
        left -= em_len;
    }
    if list.is_empty() {
        return Step::Text(1);
    }
    Step::Punct(list)
}
