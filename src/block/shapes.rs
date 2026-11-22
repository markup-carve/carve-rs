//! Recognizers for the line shapes of `line-shapes.json`. Each takes a cursor
//! at the first character after the indent and returns what the shape holds.

use super::line::Cursor;
use crate::attr::{AttrBlock, parse_block, parse_glued};

/// The style of an ordered marker (C2-B-042, `ordered-styles.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

impl Style {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Decimal => "1",
            Self::LowerAlpha => "a",
            Self::UpperAlpha => "A",
            Self::LowerRoman => "i",
            Self::UpperRoman => "I",
        }
    }
}

/// What a list marker is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Bullet(char),
    /// `value` is `None` for the bare dot. `letter` holds a single letter,
    /// which a roman list reads as a numeral and an alphabetic list as a letter.
    Ordered {
        style: Style,
        delim: char,
        value: Option<u64>,
        letter: Option<char>,
    },
}

/// A matched list marker.
#[derive(Debug, Clone)]
pub struct ListMarker {
    pub kind: MarkerKind,
    pub attrs: Option<AttrBlock>,
    pub task: Option<char>,
    /// The cursor at the first content character.
    pub content: usize,
    pub content_col: usize,
}

fn roman_value(s: &str) -> Option<u64> {
    let lower = s.to_ascii_lowercase();
    let upper_ok =
        s.bytes().all(|b| b.is_ascii_uppercase()) || s.bytes().all(|b| b.is_ascii_lowercase());
    if !upper_ok {
        return None;
    }
    let mut rest = lower.as_str();
    let mut value = 0u64;
    let places: [(&str, &str, &str, u64); 3] = [
        ("c", "d", "m", 100),
        ("x", "l", "c", 10),
        ("i", "v", "x", 1),
    ];
    let m = rest.bytes().take_while(|&b| b == b'm').count();
    if m > 3 {
        return None;
    }
    value += 1000 * m as u64;
    rest = rest.get(m..)?;
    for (one, five, ten, unit) in places {
        let (v, used) = roman_place(rest, one, five, ten);
        value += v * unit;
        rest = rest.get(used..)?;
    }
    (rest.is_empty() && value > 0).then_some(value)
}

/// One decimal place of a roman numeral: `one ten`, `one five`, `five? one{0,3}`.
fn roman_place(s: &str, one: &str, five: &str, ten: &str) -> (u64, usize) {
    if s.starts_with(&format!("{one}{ten}")) {
        return (9, 2);
    }
    if s.starts_with(&format!("{one}{five}")) {
        return (4, 2);
    }
    let mut used = 0;
    let mut v = 0;
    if s.starts_with(five) {
        used = 1;
        v = 5;
    }
    let ones = s
        .get(used..)
        .unwrap_or("")
        .bytes()
        .take_while(|&b| char::from(b).to_string() == one)
        .take(3)
        .count();
    (v + ones as u64, used + ones)
}

/// C2-B-033: one space or more, then content that is not only spaces.
fn after_spaces(c: &Cursor<'_>, min: usize) -> Option<(usize, usize)> {
    let mut k = *c;
    let mut n = 0;
    while k.peek_byte() == Some(b' ') {
        k.bump();
        n += 1;
    }
    k = k.after_blanks();
    (n >= min && !k.is_blank()).then_some((k.pos, k.col))
}

fn glued_attrs(c: &mut Cursor<'_>) -> Option<Option<AttrBlock>> {
    if c.peek_byte() != Some(b'{') {
        return Some(None);
    }
    // C2-B-100: a brace block on a marker line closes on that line.
    let (block, end) = parse_glued(c.text, c.pos)?;
    if block.empty {
        return None;
    }
    let n = end - c.pos;
    c.bump_n(n);
    Some(Some(block))
}

/// A bullet or ordered marker with its content (C2-B-033 to C2-B-046).
#[must_use]
pub fn list_marker(c: &Cursor<'_>) -> Option<ListMarker> {
    let mut k = *c;
    let first = k.peek_byte()?;
    let (kind, min_spaces) = match first {
        b'-' | b'*' => {
            k.bump();
            (MarkerKind::Bullet(char::from(first)), 1)
        }
        _ => ordered_head(&mut k)?,
    };
    let attrs = glued_attrs(&mut k)?;
    let (pos, col) = after_spaces(&k, min_spaces)?;
    let mut m = ListMarker {
        kind,
        attrs,
        task: None,
        content: pos,
        content_col: col,
    };
    if let MarkerKind::Ordered { value: None, .. } = kind {
        // C2-B-043: `. . . and then silence` is a paragraph.
        if k.text.get(pos..).is_some_and(|r| r.starts_with(". ")) {
            return None;
        }
    }
    if pos == k.pos + min_spaces {
        task_box(&k, &mut m);
    }
    Some(m)
}

/// C2-B-046: `[`, one of nine characters, `]`, one space, content.
fn task_box(k: &Cursor<'_>, m: &mut ListMarker) {
    let mut t = *k;
    t.pos = m.content;
    t.col = m.content_col;
    let r = t.rest().as_bytes();
    let (Some(b'['), Some(&ch), Some(b']'), Some(b' ')) = (r.first(), r.get(1), r.get(2), r.get(3))
    else {
        return;
    };
    if !crate::tables::TASK_STATES
        .iter()
        .any(|(c, ..)| *c == char::from(ch))
    {
        return;
    }
    t.bump_n(4);
    if t.is_blank() {
        return;
    }
    m.task = Some(char::from(ch));
    m.content = t.pos;
    m.content_col = t.col;
}

fn ordered_head(k: &mut Cursor<'_>) -> Option<(MarkerKind, usize)> {
    let rest = k.rest();
    let b = rest.as_bytes();
    if b.first() == Some(&b'.') {
        k.bump();
        return Some((
            MarkerKind::Ordered {
                style: Style::Decimal,
                delim: '.',
                value: None,
                letter: None,
            },
            1,
        ));
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        return decimal_head(k, digits);
    }
    let letters = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    if letters == 0 || letters > 9 {
        return None;
    }
    let delim = char::from(*b.get(letters)?);
    if !matches!(delim, '.' | ')') {
        return None;
    }
    let word = rest.get(..letters)?;
    let upper = word.bytes().all(|c| c.is_ascii_uppercase());
    let (style, value, letter) = letter_style(word, upper)?;
    // C2-B-040: an uppercase marker with a dot needs two spaces (see gap G-002).
    let min = if upper && delim == '.' { 2 } else { 1 };
    k.bump_n(letters + 1);
    Some((
        MarkerKind::Ordered {
            style,
            delim,
            value: Some(value),
            letter,
        },
        min,
    ))
}

fn decimal_head(k: &mut Cursor<'_>, digits: usize) -> Option<(MarkerKind, usize)> {
    let rest = k.rest();
    let delim = char::from(*rest.as_bytes().get(digits)?);
    if digits > 9 || !matches!(delim, '.' | ')') {
        return None;
    }
    let value = rest.get(..digits)?.parse().ok()?;
    k.bump_n(digits + 1);
    Some((
        MarkerKind::Ordered {
            style: Style::Decimal,
            delim,
            value: Some(value),
            letter: None,
        },
        1,
    ))
}

fn letter_style(word: &str, upper: bool) -> Option<(Style, u64, Option<char>)> {
    let mut chars = word.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        let roman = matches!(first.to_ascii_lowercase(), 'i');
        let style = match (roman, upper) {
            (true, false) => Style::LowerRoman,
            (true, true) => Style::UpperRoman,
            (false, false) => Style::LowerAlpha,
            (false, true) => Style::UpperAlpha,
        };
        let value = if roman {
            1
        } else {
            u64::from(first.to_ascii_lowercase() as u8 - b'a' + 1)
        };
        return Some((style, value, Some(first)));
    }
    let value = roman_value(word)?;
    Some((
        if upper {
            Style::UpperRoman
        } else {
            Style::LowerRoman
        },
        value,
        None,
    ))
}

/// The value of a single letter read in a list of `style`.
#[must_use]
pub fn letter_value(letter: char, style: Style) -> Option<u64> {
    match style {
        Style::LowerRoman | Style::UpperRoman => roman_value(&letter.to_string()),
        _ => Some(u64::from(letter.to_ascii_lowercase() as u8).saturating_sub(u64::from(b'a')) + 1),
    }
}

/// C2-B-049: whether a marker continues a list of the given kind.
#[must_use]
pub fn marker_matches(list: MarkerKind, m: MarkerKind) -> bool {
    match (list, m) {
        (MarkerKind::Bullet(a), MarkerKind::Bullet(b)) => a == b,
        (
            MarkerKind::Ordered {
                style: ls,
                delim: ld,
                ..
            },
            MarkerKind::Ordered {
                style,
                delim,
                letter,
                ..
            },
        ) => {
            if ld != delim {
                return false;
            }
            if style == ls {
                return true;
            }
            let Some(l) = letter else { return false };
            let upper = l.is_ascii_uppercase();
            match ls {
                // C2-B-039: in an alphabetic list every single letter is a sibling.
                Style::LowerAlpha => !upper,
                Style::UpperAlpha => upper,
                // C2-B-038: in a roman list a single roman letter is a sibling.
                Style::LowerRoman => !upper && roman_value(&l.to_string()).is_some(),
                Style::UpperRoman => upper && roman_value(&l.to_string()).is_some(),
                Style::Decimal => false,
            }
        }
        _ => false,
    }
}

/// Quote mark (C2-B-031): `>` followed by a space or by the end of the line.
/// Returns the cursor behind the mark and its one space.
#[must_use]
pub fn quote_mark<'a>(c: &Cursor<'a>) -> Option<Cursor<'a>> {
    if c.peek_byte() != Some(b'>') {
        return None;
    }
    let mut k = *c;
    k.bump();
    match k.peek_byte() {
        None => Some(k),
        Some(b' ') => {
            k.bump();
            Some(k)
        }
        _ => None,
    }
}

/// A heading (C2-B-151, C2-B-152): level and the content cursor.
#[must_use]
pub fn heading(c: &Cursor<'_>) -> Option<(u8, usize)> {
    let b = c.rest().as_bytes();
    let ch = *b.first()?;
    if ch != b'=' && ch != b'#' {
        return None;
    }
    let n = b.iter().take_while(|&&x| x == ch).count();
    if n > 6 || b.get(n) != Some(&b' ') {
        return None;
    }
    let rest = c.rest().get(n + 1..)?;
    if rest.bytes().all(|x| x == b' ' || x == b'\t') {
        return None;
    }
    let skip = rest
        .bytes()
        .take_while(|&x| x == b' ' || x == b'\t')
        .count();
    u8::try_from(n).ok().map(|l| (l, c.pos + n + 1 + skip))
}

/// A thematic break (C2-B-159).
#[must_use]
pub fn thematic_break(c: &Cursor<'_>) -> bool {
    let r = c.rest();
    let n = r.bytes().take_while(|&b| b == b'*').count();
    n >= 3 && r.get(n..).is_some_and(crate::chars::is_blank_line)
}

/// A run of one fence character and its length.
#[must_use]
pub fn fence_run(c: &Cursor<'_>, ch: u8) -> usize {
    c.rest().bytes().take_while(|&b| b == ch).count()
}

/// The parts of a code fence info string (C2-B-117).
#[derive(Debug, Clone, Default)]
pub struct FenceInfo {
    pub language: Option<String>,
    pub raw: Option<String>,
    pub title: Option<(usize, usize)>,
    pub label: Option<String>,
    /// C2-B-122: further words after the opener's parts, which it does not read.
    pub further: Option<usize>,
    pub glued: Option<(usize, usize)>,
}

/// A code fence opener: the run length and the info (C2-B-108, C2-B-118).
#[must_use]
pub fn code_fence_opener(c: &Cursor<'_>) -> Option<(usize, FenceInfo)> {
    let n = fence_run(c, b'`');
    if n < 3 {
        return None;
    }
    let info = c.rest().get(n..)?;
    if info.contains('`') {
        return None;
    }
    Some((n, fence_info(info, c.offset() + n)))
}

fn is_lang_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'#' | b'+' | b'-' | b'.' | b'/' | b'_')
}

fn is_raw_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')
}

fn fence_info(info: &str, base: usize) -> FenceInfo {
    let mut fi = FenceInfo::default();
    let b = info.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b' ') {
        i = 1;
    }
    if b.get(i) == Some(&b'=') {
        let n = b
            .get(i + 1..)
            .map_or(0, |r| r.iter().take_while(|&&x| is_raw_char(x)).count());
        if n > 0 && b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            fi.raw = info.get(i + 1..i + 1 + n).map(String::from);
            let end = skip_glued(info, i + 1 + n, base, &mut fi.glued);
            fi.further = further_start(info, end, base);
            return fi;
        }
    }
    let n = b
        .get(i..)
        .map_or(0, |r| r.iter().take_while(|&&x| is_lang_char(x)).count());
    if n > 0 {
        fi.language = info.get(i..i + n).map(String::from);
        i += n;
    }
    // C2-B-105: a brace block after the language is further words (C2-B-122).
    i = skip_glued(info, i, base, &mut fi.glued);
    let end = title_and_label(info, i, base, n == 0, &mut fi.title, &mut fi.label);
    fi.further = if info.starts_with('\t') {
        Some(base)
    } else {
        further_start(info, end, base)
    };
    fi
}

fn skip_glued(s: &str, at: usize, base: usize, range: &mut Option<(usize, usize)>) -> usize {
    if s.as_bytes().get(at) == Some(&b'{')
        && let Some((_, end)) = parse_glued(s, at)
    {
        *range = Some((base + at, base + end));
        return end;
    }
    at
}

fn further_start(s: &str, at: usize, base: usize) -> Option<usize> {
    let rest = s.get(at..)?;
    if rest.is_empty() {
        return None;
    }
    let skipped = rest
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count();
    if skipped == rest.len() {
        return (at == 1 && s.starts_with("  ")).then_some(base + 1);
    }
    Some(base + at + skipped)
}

/// C2-I-033: a backslash escapes a quote inside a title.
fn closing_quote(s: &str, mut i: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    loop {
        match *bytes.get(i)? {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
}

/// C2-B-119: an optional title and label follow the opener; returns the
/// offset behind what they took.
pub fn title_and_label(
    s: &str,
    mut i: usize,
    base: usize,
    allow_glued_first: bool,
    title: &mut Option<(usize, usize)>,
    label: &mut Option<String>,
) -> usize {
    let b = s.as_bytes();
    let sp = b
        .get(i..)
        .map_or(0, |r| r.iter().take_while(|&&x| x == b' ').count());
    if (sp > 0 || allow_glued_first) && b.get(i + sp) == Some(&b'"') {
        let start = i + sp + 1;
        if let Some(close) = closing_quote(s, start) {
            *title = Some((base + start, base + close));
            i = close + 1;
        }
    }
    let sp = b
        .get(i..)
        .map_or(0, |r| r.iter().take_while(|&&x| x == b' ').count());
    if (sp > 0 || (allow_glued_first && title.is_none())) && b.get(i + sp) == Some(&b'[') {
        if let Some(close) = s.get(i + sp + 1..).and_then(|r| r.find(']')) {
            *label = s.get(i + sp + 1..i + sp + 1 + close).map(String::from);
            i += sp + close + 2;
        }
    }
    i
}

/// A bare closer run: the character run and nothing but trailing whitespace.
#[must_use]
pub fn bare_run(c: &Cursor<'_>, ch: u8) -> usize {
    let n = fence_run(c, ch);
    if n >= 3 && c.rest().get(n..).is_some_and(crate::chars::is_blank_line) {
        n
    } else {
        0
    }
}

/// What a div opener holds (C2-B-119).
#[derive(Debug, Clone, Default)]
pub struct DivInfo {
    pub name: Option<String>,
    pub sigil: Option<char>,
    pub title: Option<(usize, usize)>,
    pub label: Option<String>,
    pub attrs: Option<AttrBlock>,
    /// C2-B-122: further words after the opener's parts.
    pub further: Option<usize>,
    pub glued: Option<(usize, usize)>,
}

/// A div opener or a bare colon run (C2-B-119, C2-B-125, C2-B-196).
#[must_use]
pub fn div_opener(c: &Cursor<'_>) -> Option<(usize, DivInfo)> {
    let n = fence_run(c, b':');
    if n < 3 {
        return None;
    }
    let rest = c.rest().get(n..)?;
    let base = c.offset() + n;
    let mut info = DivInfo::default();
    if crate::chars::is_blank_line(rest) {
        return Some((n, info));
    }
    let b = rest.as_bytes();
    let sp = b.iter().take_while(|&&x| x == b' ').count();
    if sp == 0 {
        return None;
    }
    let i = sp;
    let name_len = b.get(i..).map_or(0, |r| {
        r.iter()
            .take_while(|&&x| x.is_ascii_alphanumeric() || x == b'_' || x == b'-')
            .count()
    });
    let end = if name_len > 0 {
        info.name = rest.get(i..i + name_len).map(String::from);
        after_div_name(rest, i + name_len, base, &mut info)
    } else if let Some(&s @ (b'>' | b'\\' | b'|')) = b.get(i) {
        info.sigil = Some(char::from(s));
        i + 1
    } else {
        title_and_label(rest, 0, base, false, &mut info.title, &mut info.label)
    };
    info.further = further_start(rest, end, base);
    Some((n, info))
}

fn after_div_name(rest: &str, i: usize, base: usize, info: &mut DivInfo) -> usize {
    let j = skip_glued(rest, i, base, &mut info.glued);
    title_and_label(rest, j, base, false, &mut info.title, &mut info.label)
}

/// A table row (C2-B-166, C2-B-167): its separators, row attribute and
/// the start of a trailing line comment.
pub(crate) struct TableRowShape {
    pub attrs: Option<AttrBlock>,
    pub comment: Option<usize>,
    /// Separator offsets relative to the start of the cursor's rest, through
    /// the chosen closing pipe. The cell pass reuses these positions.
    pub separators: super::table::Separators,
}

/// The cell area `(start, end)` in the line, row attributes and a trailing
/// comment start. The block parser uses [`table_row_shape`] to retain the
/// separator positions as well.
#[must_use]
pub fn table_row(c: &Cursor<'_>) -> Option<(usize, usize, Option<AttrBlock>, Option<usize>)> {
    let shape = table_row_shape(c)?;
    let last = *shape.separators.last()?;
    Some((c.pos + 1, c.pos + last, shape.attrs, shape.comment))
}

pub(crate) fn table_row_shape(c: &Cursor<'_>) -> Option<TableRowShape> {
    let r = c.rest();
    if !r.starts_with('|') {
        return None;
    }
    let seps = super::table::compact_separators(r);
    if seps.len() < 2 || seps.first() != Some(&0) {
        return None;
    }
    let (last, attrs, comment) = seps.iter().skip(1).rev().find_map(|&last| {
        let tail = r.get(last + 1..)?;
        let (attrs, comment) = row_tail(tail)?;
        Some((last, attrs, comment))
    })?;
    let mut seps = seps;
    if seps.last() != Some(&last) {
        seps.truncate(seps.partition_point(|&p| p <= last));
    }
    Some(TableRowShape {
        attrs,
        comment: comment.map(|p| c.pos + last + 1 + p),
        separators: seps,
    })
}

/// After the closing pipe: a row attribute glued or one space after it, then
/// a line comment, then nothing else (C2-B-167).
fn row_tail(tail: &str) -> Option<(Option<AttrBlock>, Option<usize>)> {
    let mut i = 0;
    let b = tail.as_bytes();
    let mut attrs = None;
    let brace_at = if b.first() == Some(&b'{') {
        Some(0)
    } else if b.first() == Some(&b' ') && b.get(1) == Some(&b'{') {
        Some(1)
    } else {
        None
    };
    if let Some(at) = brace_at {
        if let Some((block, end)) = parse_glued(tail, at) {
            if !block.empty {
                attrs = Some(block);
                i = end;
            }
        }
    }
    let rest = tail.get(i..)?;
    let trimmed = rest.trim_start_matches([' ', '\t']);
    if trimmed.is_empty() {
        return Some((attrs, None));
    }
    // C2-B-133: after whitespace `%%` is a line comment, `%%%` included; only a
    // `%%%` that starts a line opens a comment block.
    if trimmed.starts_with("%%") && trimmed.len() < rest.len() && !trimmed.contains('|') {
        return Some((attrs, Some(i)));
    }
    None
}

/// An attribute line (C2-B-090, C2-B-193): glued blocks alone on the line.
#[must_use]
pub fn attribute_line(c: &Cursor<'_>) -> Option<AttrBlock> {
    let r = c.rest();
    if !r.starts_with('{') {
        return None;
    }
    let (block, end) = parse_glued(r, 0)?;
    if block.empty || !r.get(end..).is_some_and(crate::chars::is_blank_line) {
        return None;
    }
    Some(block)
}

/// A caption line (C2-B-146): `^`, spaces, content.
#[must_use]
pub fn caption(c: &Cursor<'_>) -> Option<usize> {
    let r = c.rest();
    if !r.starts_with('^') {
        return None;
    }
    let mut k = *c;
    k.bump();
    let (pos, _) = after_spaces(&k, 1)?;
    Some(pos)
}

/// A term line `:: ` or a description line `: ` (C2-B-067, C2-B-194).
/// Returns whether it is a term and the content position and column.
#[must_use]
pub fn desc_mark(c: &Cursor<'_>) -> Option<(bool, usize, usize)> {
    let r = c.rest();
    let term = r.starts_with("::");
    if !term && !r.starts_with(':') {
        return None;
    }
    let mut k = *c;
    k.bump_n(if term { 2 } else { 1 });
    let (pos, col) = after_spaces(&k, 1)?;
    Some((term, pos, col))
}

/// A link definition (C2-B-083): label, destination, title.
#[derive(Debug, Clone)]
pub struct LinkDef {
    pub label: String,
    pub href: String,
    pub title: Option<String>,
    pub attrs: Option<AttrBlock>,
}

/// The label of `[label]:` or `[^label]:` at the start of `r`, with the
/// offset behind the colon. The label holds no unescaped bracket (C2-B-082).
fn def_label(r: &str, caret: bool) -> Option<(&str, usize)> {
    let open = if caret { "[^" } else { "[" };
    if !r.starts_with(open) {
        return None;
    }
    let start = open.len();
    let b = r.as_bytes();
    let mut i = start;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 2,
            b'[' => return None,
            b']' => break,
            _ => i += 1,
        }
    }
    if b.get(i) != Some(&b']') || b.get(i + 1) != Some(&b':') {
        return None;
    }
    let label = r.get(start..i)?;
    if crate::chars::fold_label(label).is_empty() {
        return None;
    }
    Some((label, i + 2))
}

/// A footnote definition `[^label]: content`: label and content offset in `r`.
#[must_use]
pub fn footnote_def(r: &str) -> Option<(String, usize)> {
    let (label, after) = def_label(r, true)?;
    let rest = r.get(after..)?;
    if !rest.starts_with(' ') || crate::chars::is_blank_line(rest) {
        return None;
    }
    let skip = rest.bytes().take_while(|&b| b == b' ').count();
    Some((label.to_string(), after + skip))
}

/// Offset just past the parsed `[^label]:` marker.
#[must_use]
pub fn footnote_marker_len(r: &str) -> Option<usize> {
    def_label(r, true).map(|(_, after)| after)
}

/// A link definition line (C2-B-083, C2-B-089).
#[must_use]
pub fn link_def(r: &str) -> Option<LinkDef> {
    if r.starts_with("[^") {
        return None;
    }
    let (label, after) = def_label(r, false)?;
    let rest = r.get(after..)?;
    let sp = rest.bytes().take_while(|&b| b == b' ').count();
    if sp == 0 {
        return None;
    }
    let rest = rest.get(sp..)?;
    let dlen = rest
        .bytes()
        .take_while(|&b| b != b' ' && b != b'\t')
        .count();
    if dlen == 0 {
        return None;
    }
    // C2-B-083, C2-I-013: the destination of a link tail reads its escapes.
    let href = crate::block::include::unescape(rest.get(..dlen)?);
    let tail = rest.get(dlen..)?;
    let (title, attrs) = def_tail(tail)?;
    Some(LinkDef {
        label: label.to_string(),
        href,
        title,
        attrs,
    })
}

fn def_tail(tail: &str) -> Option<(Option<String>, Option<AttrBlock>)> {
    if crate::chars::is_blank_line(tail) {
        return Some((None, None));
    }
    let tail = tail.strip_prefix(' ')?;
    if tail.starts_with(' ') || tail.starts_with('\t') {
        return None;
    }
    let (title, rest) = if matches!(tail.as_bytes().first(), Some(b'\'' | b'"')) {
        let (title, end) = def_title(tail)?;
        (Some(title), tail.get(end..)?)
    } else {
        (None, tail)
    };
    if crate::chars::is_blank_line(rest) {
        return Some((title, None));
    }
    let rest = if title.is_some() {
        rest.strip_prefix(' ')?
    } else {
        rest
    };
    let (attrs, end) = parse_block(rest, 0)?;
    if attrs.empty || !rest.get(end..).is_some_and(crate::chars::is_blank_line) {
        return None;
    }
    Some((title, Some(attrs)))
}

fn def_title(t: &str) -> Option<(String, usize)> {
    let q = t.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let mut out = String::new();
    let mut it = t.char_indices().skip(1);
    while let Some((i, c)) = it.next() {
        if c == '\\' {
            match it.next() {
                Some((_, n)) if n.is_ascii_punctuation() => out.push(n),
                Some((_, n)) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            }
        } else if c == q {
            return Some((out, i + 1));
        } else {
            out.push(c);
        }
    }
    None
}

/// An abbreviation definition `*[KEY]: expansion` (C2-B-084).
#[must_use]
pub fn abbr_def(r: &str) -> Option<(String, String)> {
    let inner = r.strip_prefix("*[")?;
    let n = inner.bytes().take_while(u8::is_ascii_alphanumeric).count();
    if n == 0 {
        return None;
    }
    let after = inner.get(n..)?.strip_prefix("]: ")?;
    let expansion = after.trim_end_matches([' ', '\t']);
    if expansion.trim_start_matches(' ').is_empty() {
        return None;
    }
    Some((inner.get(..n)?.to_string(), expansion.to_string()))
}

/// A frontmatter opener on line 1 (C2-B-161, C2-B-164): the format name.
#[must_use]
pub fn frontmatter_opener(line: &str) -> Option<String> {
    let rest = line.strip_prefix("---")?;
    let rest = rest.trim_end_matches([' ', '\t']);
    if rest.is_empty() {
        return Some("yaml".into());
    }
    let word = rest.strip_prefix(' ').unwrap_or(rest);
    let b = word.as_bytes();
    let ok = !b.is_empty() && b.iter().all(u8::is_ascii_alphanumeric);
    ok.then(|| word.to_string())
}

/// The frontmatter closer (C2-B-162).
#[must_use]
pub fn frontmatter_closer(line: &str) -> bool {
    line.strip_prefix("---")
        .is_some_and(crate::chars::is_blank_line)
}

/// Whether `r` is a comment-only line: `%%` at its start (C2-B-137).
#[must_use]
pub fn comment_only(r: &str) -> bool {
    r.starts_with("%%") && !r.starts_with("%%%")
}

/// The value of a line comment (C2-A-074).
#[must_use]
pub fn line_comment_value(r: &str) -> String {
    let body = r.strip_prefix("%%").unwrap_or(r);
    let body = body.strip_prefix(' ').unwrap_or(body);
    body.trim_end_matches([' ', '\t']).to_string()
}

/// A single brace block at `at`, used where only one block may stand.
#[must_use]
pub fn one_block(s: &str, at: usize) -> Option<(AttrBlock, usize)> {
    parse_block(s, at)
}
