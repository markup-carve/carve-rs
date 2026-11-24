//! The inline nodes as Carve source (C2-W-069 to C2-W-080, C2-W-116). A line
//! break of the content is a line feed in the string; the block that holds the
//! content places the lines.

use super::attrs::{BlockParts, brace_block, plain_block, quoted};
use super::node::{N, attrs, children, has, id, s, text, ty};
use super::state::{Writer, lose, trim_end};
use crate::chars::fold_label;
use crate::hash::AddrSet;
use crate::view::{Arr, V};

/// The content stands in a table cell: a pipe in text takes a backslash (C2-W-098).
pub const IN_CELL: u32 = 1;
pub const IN_EMPHASIS: u32 = 2;
pub const IN_STRONG: u32 = 4;
/// C2-W-080: a hard break is the bare line ending.
pub const BARE_BREAKS: u32 = 8;
pub const IN_BRACKET: u32 = 16;
/// C2-W-117: inside the body of a figure group.
pub const IN_FIGURE_GROUP: u32 = 32;
/// C2-I-089: inside an inline footnote, where a caret pair opens nothing.
pub const IN_NOTE: u32 = 64;

#[derive(Default)]
struct BraceQuote {
    at: usize,
    brace: bool,
    quote: u8,
}

/// C2-I-033: a quoted brace value is opaque, including its backslashes.
fn in_quoted_brace(value: &[u8], at: usize, c: &mut BraceQuote) -> bool {
    while c.at < at {
        let code = value.get(c.at).copied().unwrap_or(0);
        if c.quote != 0 {
            if code == b'\\' {
                c.at += 2;
                continue;
            }
            if code == c.quote {
                c.quote = 0;
            }
        } else if code == b'{' {
            c.brace = true;
        } else if code == b'}' {
            c.brace = false;
        } else if c.brace && (code == b'"' || code == b'\'') {
            c.quote = code;
        }
        c.at += 1;
    }
    c.quote != 0
}

/// C2-W-097: a backslash that the next character of the same text would make an escape is written twice.
fn doubles(value: &[u8], at: usize, c: &mut BraceQuote) -> bool {
    let Some(&next) = value.get(at + 1) else {
        return false;
    };
    (next == b' ' || next.is_ascii_punctuation()) && !in_quoted_brace(value, at, c)
}

fn always_escaped(value: &[u8], at: usize, flags: u32, c: &mut BraceQuote) -> bool {
    let code = value.get(at).copied().unwrap_or(0);
    (code == b'\\' && doubles(value, at, c)) || (flags & IN_CELL != 0 && code == b'|')
}

/// The index of the first entry of `offsets` that is not below `from`.
pub fn first_from(offsets: &[usize], from: usize) -> usize {
    offsets.partition_point(|&o| o < from)
}

/// The offsets of `node` that take a backslash: those asked for and those of C2-W-097, C2-W-098.
pub fn escape_offsets(w: &Writer<'_>, node: V<'_>, flags: u32) -> Vec<usize> {
    let escapes = w.escapes_of(node);
    let bytes = text(node).as_bytes();
    let mut out = Vec::new();
    let mut next = 0;
    let mut cursor = BraceQuote::default();
    for at in 0..bytes.len() {
        let asked = escapes.get(next) == Some(&at);
        if asked {
            next += 1;
        }
        if asked || always_escaped(bytes, at, flags, &mut cursor) {
            out.push(at);
        }
    }
    out
}

/// The text `value[from, to)` with the backslashes it needs.
fn escaped(w: &Writer<'_>, node: V<'_>, value: &str, from: usize, to: usize, flags: u32) -> String {
    let escapes = w.escapes_of(node);
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(to.saturating_sub(from) + 4);
    let mut next = first_from(escapes, from);
    let mut start = from;
    let mut cursor = BraceQuote::default();
    for at in from..to {
        let asked = escapes.get(next) == Some(&at);
        if asked {
            next += 1;
        }
        if !asked && !always_escaped(bytes, at, flags, &mut cursor) {
            continue;
        }
        out.push_str(value.get(start..at).unwrap_or(""));
        out.push('\\');
        start = at;
    }
    out.push_str(value.get(start..to).unwrap_or(""));
    out
}

/// The offset in the value of the text `node` behind the `bytes` bytes of its
/// spelling that start at `from`, or `None` where no character ends there.
pub fn spelled_end(
    w: &Writer<'_>,
    node: V<'_>,
    from: usize,
    bytes: usize,
    flags: u32,
) -> Option<usize> {
    let value = text(node);
    let raw = value.as_bytes();
    let escapes = w.escapes_of(node);
    let mut next = first_from(escapes, from);
    let mut rest = bytes;
    let mut at = from;
    let mut cursor = BraceQuote::default();
    while rest > 0 && at < raw.len() {
        let asked = escapes.get(next) == Some(&at);
        if asked {
            next += 1;
        }
        let cost = 1 + usize::from(asked || always_escaped(raw, at, flags, &mut cursor));
        rest = rest.checked_sub(cost)?;
        at += 1;
    }
    (rest == 0 && value.is_char_boundary(at)).then_some(at)
}

fn text_node(w: &Writer<'_>, node: V<'_>, flags: u32) -> String {
    let value = text(node);
    let points = if flags & IN_CELL == 0 {
        None
    } else {
        w.wrap_points.as_ref().and_then(|p| p.get(&id(node)))
    };
    if points.is_none()
        && !w.has_escapes(node)
        && !value.contains('\\')
        && (flags & IN_CELL == 0 || !value.contains('|'))
    {
        return value.to_string();
    }
    let mut out = String::new();
    let mut from = 0;
    for &point in points.into_iter().flatten() {
        // C2-L-021: the joining space before the fragment is the line break of the continuation row.
        out.push_str(&escaped(
            w,
            node,
            value,
            from,
            from.max(point.saturating_sub(1)),
            flags,
        ));
        out.push('\n');
        from = point;
    }
    out.push_str(&escaped(w, node, value, from, value.len(), flags));
    out
}

/// C2-I-023, C2-I-024: a line comment that starts or ends the content of a span.
pub fn edge_comment(node: V<'_>) -> bool {
    let c = children(node);
    let line =
        |n: Option<V<'_>>| n.is_some_and(|n| ty(n) == "comment" && s(n, "form") == Some("line"));
    line(c.first()) || line(c.last())
}

/// Whether emphasis (bit 1) and strong (bit 2) occur below `node` (C2-W-070).
pub fn kinds_below(w: &mut Writer<'_>, node: V<'_>) -> u8 {
    if let Some(&k) = w.kinds_below.get(&id(node)) {
        return k;
    }
    let mut kinds = 0;
    for c in children(node) {
        match ty(c) {
            "emphasis" => kinds |= 1,
            "strong" => kinds |= 2,
            _ => {}
        }
        if has(c, "children") {
            kinds |= kinds_below(w, c);
        }
    }
    w.kinds_below.insert(id(node), kinds);
    kinds
}

pub fn braced_pair(w: &mut Writer<'_>, node: V<'_>, flags: u32, bit: u8, inside: u32) -> bool {
    flags & inside != 0
        || kinds_below(w, node) & bit != 0
        || w.form(node) > 0
        || edge_comment(node)
        || edge_mark(node, if bit == 2 { '*' } else { '/' })
        || braced_by_content(node, if bit == 2 { '*' } else { '/' })
}

/// C2-W-069 decided from the content alone, before any repair round (carve#6482).
pub fn braced_by_content(node: V<'_>, mark: char) -> bool {
    edge_space(node) || holds_pairing_mark(children(node), mark, true)
}

/// C2-W-069, C2-I-053: the content holds the node's own mark where it could open or close, or between two word
/// characters outside a braced span. A bracket bounds the delimiter stack, and a run of two (C2-I-049) is text
/// unless a brace beside it takes one mark.
fn text_holds_mark(t: &str, mark: char, words: bool) -> bool {
    use crate::chars::{is_flank_space, is_word};
    if !t.contains(mark) {
        return false;
    }
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars.get(i) != Some(&mark) {
            i += 1;
            continue;
        }
        let before = i.checked_sub(1).and_then(|j| chars.get(j)).copied();
        let run = chars.iter().skip(i).take_while(|&&c| c == mark).count();
        if run > 1 && before != Some('{') && chars.get(i + run) != Some(&'}') {
            i += run;
            continue;
        }
        let after = chars.get(i + 1).copied();
        let counts = if before.is_some_and(is_word) && after.is_some_and(is_word) {
            words
        } else {
            !(before.is_some_and(is_flank_space) && after.is_some_and(is_flank_space))
        };
        if counts {
            return true;
        }
        i += 1;
    }
    false
}

// A mark inside a nested pair can meet the enclosing opener only when it can close.
// An opening mark there belongs to the inner pair's delimiter scope.
fn nested_closing_mark(nodes: Arr<'_>, mark: char) -> bool {
    nodes.iter().any(|n| match ty(n) {
        "text" => {
            let mut before = None;
            let mut chars = text(n).chars().peekable();
            let mut closes = false;
            while let Some(ch) = chars.next() {
                if ch == mark
                    && before.is_some_and(|c| !crate::chars::is_flank_space(c))
                    && before != Some(mark)
                    && chars.peek() != Some(&mark)
                    && chars.peek().is_none_or(|&c| !crate::chars::is_word(c))
                {
                    closes = true;
                    break;
                }
                before = Some(ch);
            }
            closes
        }
        "emphasis" | "strong" => nested_closing_mark(children(n), mark),
        _ => false,
    })
}

fn holds_pairing_mark(nodes: Arr<'_>, mark: char, words: bool) -> bool {
    nodes.iter().any(|n| match ty(n) {
        "text" => text_holds_mark(text(n), mark, words),
        "emphasis" | "strong" => nested_closing_mark(children(n), mark),
        "link" | "image" | "span" | "inline_footnote" | "inline_extension" | "citation_group" => {
            false
        }
        t => {
            has(n, "children")
                && holds_pairing_mark(
                    children(n),
                    mark,
                    words
                        && !matches!(
                            t,
                            "underline"
                                | "strike"
                                | "highlight"
                                | "superscript"
                                | "subscript"
                                | "insert"
                                | "delete"
                        ),
                )
        }
    })
}

/// C2-W-069 with C2-I-047, C2-I-048 and C2-I-050 (carve#6482): a bare mark does not open before whitespace or a
/// joining character, nor close behind whitespace, so the form is braced before any repair round.
fn edge_space(node: V<'_>) -> bool {
    let c = children(node);
    let opens = c.first().is_some_and(|n| match ty(n) {
        "text" => text(n)
            .chars()
            .next()
            .is_some_and(|ch| crate::chars::is_flank_space(ch) || crate::chars::is_joining(ch)),
        "soft_break" | "non_breaking_space" => true,
        _ => false,
    });
    opens
        || c.last().is_some_and(|n| match ty(n) {
            "text" => text(n)
                .chars()
                .next_back()
                .is_some_and(crate::chars::is_flank_space),
            "soft_break" | "hard_break" | "non_breaking_space" => true,
            _ => false,
        })
}

/// C2-W-069 (carve#6469): a content that starts or ends with the node's own mark would pair with the opener or
/// closer beside it, so the node is braced, whatever stands behind it.
fn edge_mark(node: V<'_>, mark: char) -> bool {
    let c = children(node);
    c.first()
        .is_some_and(|n| ty(n) == "text" && text(n).starts_with(mark))
        || c.last()
            .is_some_and(|n| ty(n) == "text" && text(n).ends_with(mark))
}

fn refs(nodes: Arr<'_>) -> Vec<V<'_>> {
    nodes.iter().collect()
}

/// C2-W-116: a node that needs content and has none is left out.
fn content_of<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> Option<String> {
    let c = children(node);
    if c.is_empty() {
        lose(w, node, None);
        return None;
    }
    Some(write_inlines(w, &refs(c), flags))
}

fn paired<'t>(
    w: &mut Writer<'t>,
    node: N<'t>,
    flags: u32,
    mark: char,
    bit: u8,
    inside: u32,
) -> String {
    let Some(content) = content_of(w, node, flags | inside) else {
        return String::new();
    };
    if braced_pair(w, node, flags, bit, inside) {
        enclosed(&['{', mark], &content, &[mark, '}'])
    } else {
        enclosed(&[mark], &content, &[mark])
    }
}

fn enclosed(open: &[char], content: &str, close: &[char]) -> String {
    let mut out = String::with_capacity(content.len() + 4 * (open.len() + close.len()));
    out.extend(open);
    out.push_str(content);
    out.extend(close);
    out
}

fn braced<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32, open: char, close: char) -> String {
    content_of(w, node, flags)
        .map_or_else(String::new, |c| enclosed(&['{', open], &c, &[close, '}']))
}

/// C2-W-059: a braced comment, written `{%%}` when its value is empty.
pub fn braced_comment(value: &str) -> String {
    if value.is_empty() {
        "{%%}".into()
    } else {
        format!("{{% {value} %}}")
    }
}

/// C2-W-077: the shortest backtick run that does not occur in `value`, with padding where the ends need it.
pub fn code_span(value: &str) -> String {
    let mut taken = std::collections::HashSet::new();
    let mut run = 0usize;
    for b in value.bytes().chain(std::iter::once(0)) {
        if b == b'`' {
            run += 1;
        } else {
            if run > 0 {
                taken.insert(run);
            }
            run = 0;
        }
    }
    let mut length = 1;
    while taken.contains(&length) {
        length += 1;
    }
    let mut out = String::with_capacity(value.len() + 2 * length + 2);
    out.extend(std::iter::repeat_n('`', length));
    if value.starts_with('`') {
        out.push(' ');
    }
    out.push_str(value);
    if value.ends_with('`') {
        out.push(' ');
    }
    out.extend(std::iter::repeat_n('`', length));
    out
}

/// C2-W-077: a space and a backtick at either end lose that space on read-back (C2-I-016).
fn unspellable_code(value: &str) -> bool {
    value.starts_with(" `") || value.ends_with("` ")
}

fn span_value(w: &mut Writer<'_>, node: V<'_>, flags: u32) -> String {
    let spelled = cell_value(w, node, text(node), flags);
    if unspellable_code(&spelled) {
        lose(w, node, Some("value"));
    }
    code_span(&spelled)
}

/// C2-W-067, C2-I-120: a line feed has no spelling in a cell; a space stands in its place.
fn cell_value(w: &mut Writer<'_>, node: V<'_>, value: &str, flags: u32) -> String {
    if flags & IN_CELL == 0 || !value.contains('\n') {
        return value.to_string();
    }
    lose(w, node, Some("value"));
    value.replace('\n', " ")
}

/// C2-I-013: a destination reads only the escaped parenthesis and the escaped backslash; C2-W-098: in a cell a pipe too.
fn destination(value: &str, pipes: bool) -> String {
    let bytes = value.as_bytes();
    let mut unmatched = std::collections::HashSet::new();
    let mut open = Vec::new();
    for (at, &b) in bytes.iter().enumerate() {
        if b == b'(' {
            open.push(at);
        } else if b == b')' && open.pop().is_none() {
            unmatched.insert(at);
        }
    }
    unmatched.extend(open);
    let mut out = String::with_capacity(value.len());
    for (at, c) in value.char_indices() {
        let next = bytes.get(at + 1).copied().unwrap_or(b')');
        if unmatched.contains(&at) || (c == '\\' && b"()\\".contains(&next)) || (pipes && c == '|')
        {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn escape_pipes(value: &str) -> String {
    value.replace('|', "\\|")
}

/// C2-I-076, C2-I-079: a backslash at the end of a label would take its closing bracket.
fn label_text(key: &str) -> String {
    if key.ends_with('\\') {
        format!("{key} ")
    } else {
        key.to_string()
    }
}

fn reference_label(node: V<'_>, key: &str, pipes: bool) -> String {
    match s(node, "refForm") {
        Some("collapsed") => return "[]".into(),
        Some("shortcut") => return String::new(),
        _ => {}
    }
    let label = if pipes {
        escape_pipes(key)
    } else {
        key.to_string()
    };
    // C2-I-078: a label that starts with a caret or an at-sign is no reference label.
    if key.starts_with('^') || key.starts_with('@') {
        format!("[ {}]", label_text(&label))
    } else {
        format!("[{}]", label_text(&label))
    }
}

/// C2-W-073, C2-W-074: the link tail, or the label of a reference.
pub fn target(node: V<'_>, href: &str, pipes: bool) -> String {
    if let Some(key) = s(node, "key") {
        return reference_label(node, key, pipes);
    }
    let title = s(node, "tooltip").map_or(String::new(), |t| format!(" {}", quoted(t, pipes)));
    format!("({}{title})", destination(href, pipes))
}

/// C2-W-075, C2-W-084: the verbatim text of a reference, where the sidecar holds it.
pub fn raw_reference(w: &Writer<'_>, node: V<'_>, flags: u32) -> Option<String> {
    if !has(node, "key") || w.form(node) > 0 {
        return None;
    }
    let raw = w.fact(node)?.raw_ref.as_ref()?;
    if !raw.contains('\n') {
        return Some(raw.clone());
    }
    if flags & IN_CELL != 0 {
        let mut out = String::new();
        for (i, part) in raw.split('\n').enumerate() {
            if i > 0 {
                out = trim_end(&out).to_string();
                out.push(' ');
                out.push_str(part.trim_start_matches([' ', '\t']));
            } else {
                out.push_str(part);
            }
        }
        return Some(out);
    }
    Some(trim_line_ends(raw))
}

/// C2-W-022: no space or tab at the end of a line, except one an odd run of backslashes escapes.
fn trim_line_ends(raw: &str) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = raw.split('\n').collect();
    let last = lines.len().saturating_sub(1);
    for (i, line) in lines.iter().enumerate() {
        if i == last {
            out.push_str(line);
            break;
        }
        let b = line.as_bytes();
        let mut cut = b.len();
        while cut > 0 && matches!(b.get(cut - 1), Some(b' ' | b'\t')) {
            cut -= 1;
        }
        let mut slashes = 0;
        while cut > slashes && b.get(cut - 1 - slashes) == Some(&b'\\') {
            slashes += 1;
        }
        if slashes % 2 == 1 && b.get(cut) == Some(&b' ') {
            cut += 1;
        }
        out.push_str(line.get(..cut).unwrap_or(""));
        out.push('\n');
    }
    out
}

/// C2-I-079: a collapsed or shortcut reference whose key, its content, starts with a caret.
pub fn key_is_content(node: V<'_>) -> bool {
    matches!(s(node, "refForm"), Some("collapsed" | "shortcut"))
        && s(node, "key").is_some_and(|k| k.starts_with('^'))
}

/// C2-W-125 case five: an unescaped leading caret of link or span text reads as a footnote reference.
fn escape_leading_caret(w: &mut Writer<'_>, kids: Arr<'_>, flags: u32) {
    if flags & IN_NOTE != 0 {
        return;
    }
    if let Some(first) = kids.first() {
        if ty(first) == "text"
            && text(first).starts_with('^')
            && w.escapes_of(first).first() != Some(&0)
        {
            w.add_escape(first, 0);
        }
    }
}

#[derive(Clone, Copy)]
enum BracketPlace<'t> {
    Text(N<'t>, usize),
    Count(usize),
}

#[derive(Default)]
struct BracketScan<'t> {
    open: Vec<BracketPlace<'t>>,
    size: usize,
    lone: Vec<(N<'t>, usize)>,
    lone_count: usize,
}

fn pop_brackets(scan: &mut BracketScan<'_>, count: usize) {
    let mut left = count.min(scan.size);
    scan.size -= left;
    while left > 0 {
        match scan.open.pop() {
            Some(BracketPlace::Text(..)) => left -= 1,
            Some(BracketPlace::Count(n)) if n > left => {
                scan.open.push(BracketPlace::Count(n - left));
                left = 0;
            }
            Some(BracketPlace::Count(n)) => left -= n,
            None => break,
        }
    }
}

fn counted_brackets(scan: &mut BracketScan<'_>, (closers, openers): (usize, usize)) {
    scan.lone_count += closers.saturating_sub(scan.size);
    pop_brackets(scan, closers);
    if openers == 0 {
        return;
    }
    if let Some(BracketPlace::Count(n)) = scan.open.last_mut() {
        *n += openers;
    } else {
        scan.open.push(BracketPlace::Count(openers));
    }
    scan.size += openers;
}

fn link_bracket_counts<'t>(w: &mut Writer<'t>, link: N<'t>) -> (usize, usize) {
    if let Some(&counts) = w.link_brackets.get(&id(link)) {
        return counts;
    }
    let mut scan = BracketScan::default();
    scan_link_brackets(w, children(link), &mut scan);
    let counts = (scan.lone_count, scan.size);
    w.link_brackets.insert(id(link), counts);
    counts
}

/// Scan one link label. A nested link is a boundary for bracket pairs in the
/// containing label; counts stand in for its own brackets to keep deep scans linear.
fn scan_link_brackets<'t>(w: &mut Writer<'t>, nodes: Arr<'t>, scan: &mut BracketScan<'t>) {
    for node in nodes {
        match ty(node) {
            "text" => {
                for (at, ch) in text(node).bytes().enumerate() {
                    if ch == b'[' {
                        scan.open.push(BracketPlace::Text(node, at));
                        scan.size += 1;
                    } else if ch == b']' {
                        if scan.size > 0 {
                            pop_brackets(scan, 1);
                        } else {
                            scan.lone.push((node, at));
                            scan.lone_count += 1;
                        }
                    }
                }
            }
            "link" => {
                for place in scan.open.drain(..) {
                    if let BracketPlace::Text(n, at) = place {
                        scan.lone.push((n, at));
                    }
                }
                scan.lone_count += scan.size;
                scan.size = 0;
                let counts = link_bracket_counts(w, node);
                counted_brackets(scan, counts);
            }
            "citation_group" => {}
            _ => scan_link_brackets(w, children(node), scan),
        }
    }
}

/// C2-W-125 case five: brackets of text without a partner in this link label
/// must not take the link's closer, including across a nested link boundary.
fn escape_lone_link_brackets<'t>(w: &mut Writer<'t>, link: N<'t>) {
    if !w.link_escapes.contains_key(&id(link)) {
        let mut scan = BracketScan::default();
        scan_link_brackets(w, children(link), &mut scan);
        let mut positions = scan.lone;
        positions.extend(scan.open.into_iter().filter_map(|p| match p {
            BracketPlace::Text(n, at) => Some((n, at)),
            BracketPlace::Count(_) => None,
        }));
        w.link_escapes.insert(id(link), positions);
    }
    if let Some(positions) = w.link_escapes.get(&id(link)).cloned() {
        for (node, at) in positions {
            w.add_escape(node, at);
        }
    }
}

fn link_node<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> String {
    if let Some(raw) = raw_reference(w, node, flags) {
        w.raw_refs.push(raw.clone());
        return raw;
    }
    // A backslash before the caret would change the key; the check places it only where the link needs it.
    if !key_is_content(node) {
        escape_leading_caret(w, children(node), flags);
    }
    w.saw_link = true;
    if w.link_bracket_fallback {
        escape_lone_link_brackets(w, node);
    }
    let content = write_inlines(w, &refs(children(node)), flags | IN_BRACKET);
    let content = collapsed_content(w, node, content, flags);
    format!(
        "[{content}]{}",
        target(node, s(node, "href").unwrap_or(""), flags & IN_CELL != 0)
    )
}

/// C2-W-075: the content of a collapsed reference folds to its key (C2-I-079).
fn collapsed_content(w: &mut Writer<'_>, node: V<'_>, content: String, flags: u32) -> String {
    let Some(key) = s(node, "key") else {
        return content;
    };
    if s(node, "refForm") != Some("collapsed")
        || flags & IN_CELL != 0
        || fold_label(&content) == key
    {
        return content;
    }
    for candidate in [escaped_like(&content, key), Some(key.to_string())]
        .into_iter()
        .flatten()
    {
        if reads_as_collapsed(w, &candidate, key, children(node)) {
            return candidate;
        }
    }
    content
}

fn reads_as_collapsed(w: &mut Writer<'_>, candidate: &str, key: &str, kids: Arr<'_>) -> bool {
    let keys = std::iter::once(key.to_string()).collect();
    let chunk = super::check::Chunk::new(format!("[{candidate}][]\n"));
    let Some(p) = children(chunk.parse_keys(w, Some(&keys))).single() else {
        return false;
    };
    let Some(link) = children(p).single() else {
        return false;
    };
    ty(link) == "link"
        && s(link, "key") == Some(key)
        && super::node::lists_equal(children(link), kids)
}

fn is_label_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n')
}

/// C2-I-001, C2-I-079: the content with the escapes the key holds, or `None`.
fn escaped_like(content: &str, key: &str) -> Option<String> {
    let c = content.as_bytes();
    let k = key.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(c.len());
    let (mut at, mut ki) = (0usize, 0usize);
    while let Some(&ch) = c.get(at) {
        (at, ki) = if is_label_space(ch) {
            label_space_run(c, k, at, ki, &mut out)?
        } else {
            label_char(c, k, at, ki, &mut out)?
        };
    }
    let out = String::from_utf8(out).ok()?;
    (ki == k.len() && fold_label(&out) == key).then_some(out)
}

fn label_space_run(
    c: &[u8],
    k: &[u8],
    at: usize,
    ki: usize,
    out: &mut Vec<u8>,
) -> Option<(usize, usize)> {
    let mut end = at;
    while c.get(end).is_some_and(|&b| is_label_space(b)) {
        end += 1;
    }
    out.extend_from_slice(c.get(at..end)?);
    let skip = ki > 0 && end < c.len() && k.get(ki) == Some(&b' ');
    Some((end, ki + usize::from(skip)))
}

fn label_char(
    c: &[u8],
    k: &[u8],
    at: usize,
    ki: usize,
    out: &mut Vec<u8>,
) -> Option<(usize, usize)> {
    let ch = *c.get(at)?;
    let next = c.get(at + 1).copied();
    if ch == b'\\'
        && next.is_some_and(|n| n != b'\\' && n.is_ascii_punctuation() && k.get(ki) == Some(&n))
    {
        out.push(next.unwrap_or(b' '));
        return Some((at + 2, ki + 1));
    }
    let escape = k.get(ki) == Some(&b'\\')
        && k.get(ki + 1) == Some(&ch)
        && ch != b'\\'
        && ch.is_ascii_punctuation();
    if !escape && k.get(ki) != Some(&ch) {
        return None;
    }
    if escape {
        out.push(b'\\');
    }
    out.push(ch);
    Some((at + 1, ki + if escape { 2 } else { 1 }))
}

fn image_node(w: &mut Writer<'_>, node: V<'_>, flags: u32) -> String {
    if let Some(raw) = raw_reference(w, node, flags) {
        w.raw_refs.push(raw.clone());
        return format!("!{raw}");
    }
    let pipes = flags & IN_CELL != 0;
    let alt = image_alt(w, node);
    let alt = collapsed_alt(w, node, alt);
    let alt = if pipes { escape_pipes(&alt) } else { alt };
    format!(
        "![{alt}]{}",
        target(node, s(node, "src").unwrap_or(""), pipes)
    )
}

fn collapsed_alt(w: &mut Writer<'_>, node: V<'_>, alt: String) -> String {
    let Some(key) = s(node, "key") else {
        return alt;
    };
    if s(node, "refForm") != Some("collapsed") || fold_label(&alt) == key {
        return alt;
    }
    let keys = std::iter::once(key.to_string()).collect();
    let chunk = super::check::Chunk::new(format!("![{key}][]\n"));
    let ok = children(chunk.parse_keys(w, Some(&keys)))
        .single()
        .and_then(|p| children(p).single())
        .is_some_and(|img| {
            ty(img) == "image"
                && s(img, "key") == Some(key)
                && img.get("alt").map(V::to_value) == node.get("alt").map(V::to_value)
        });
    if ok { key.to_string() } else { alt }
}

/// The end of the code span, braced comment, autolink or cross-reference that opens at `at` of `text` (C2-I-005):
/// `Some(0)` if none opens, `None` if a code span or comment has no closer.
fn atom_end(text: &str, at: usize) -> Option<usize> {
    let alt = text.as_bytes();
    let ch = alt.get(at).copied();
    if ch == Some(b'<') {
        return Some(match crate::inline::angle(text, at) {
            Some(
                crate::inline::Angle::Xref { end, .. } | crate::inline::Angle::Autolink { end, .. },
            ) => end,
            None => 0,
        });
    }
    if ch == Some(b'{') && matches!(alt.get(at + 1), Some(b'#' | b'%')) {
        let mark = alt.get(at + 1).copied().unwrap_or(b'#');
        let tail = alt.get(at + 2..).unwrap_or(&[]);
        return tail
            .windows(2)
            .position(|w| w.first() == Some(&mark) && w.get(1) == Some(&b'}'))
            .map(|p| at + 2 + p + 2);
    }
    if ch != Some(b'`') || (at > 0 && alt.get(at - 1) == Some(&b'`')) {
        return Some(0);
    }
    let mut size = 1;
    while alt.get(at + size) == Some(&b'`') {
        size += 1;
    }
    let mut next = at + size;
    while next < alt.len() {
        if alt.get(next) != Some(&b'`') {
            next += 1;
            continue;
        }
        let mut length = 1;
        while alt.get(next + length) == Some(&b'`') {
            length += 1;
        }
        if length == size {
            return Some(next + size);
        }
        next += length;
    }
    None
}

fn raw_atoms(text: &str, lone: &mut AddrSet<usize>) -> Vec<bool> {
    let alt = text.as_bytes();
    let mut raw = vec![false; alt.len()];
    let mut at = 0;
    while at < alt.len() {
        match atom_end(text, at) {
            Some(0) => {}
            Some(end) => {
                for r in raw.iter_mut().take(end).skip(at) {
                    *r = true;
                }
                at = end;
                continue;
            }
            None => {
                lone.insert(at);
                while alt.get(at) == Some(&b'`') && alt.get(at + 1) == Some(&b'`') {
                    at += 1;
                    lone.insert(at);
                }
            }
        }
        at += 1;
    }
    raw
}

/// The brackets of `alt` outside raw content that have no partner in it. A `[^` and its partner read as a footnote
/// label, so a `]` right behind `[^` counts as lone, and with `swap` a lone `]` after such a pair takes over its `[`.
fn unpartnered(alt: &[u8], raw: &[bool], lone: &mut AddrSet<usize>, swap: bool) {
    let mut open: Vec<usize> = Vec::new();
    let mut notes: Vec<usize> = Vec::new();
    let mut at = 0;
    while let Some(&ch) = alt.get(at) {
        if raw.get(at) == Some(&true) {
        } else if ch == b'[' {
            open.push(at);
        } else if ch != b']' {
        } else if at >= 2 && alt.get(at - 1) == Some(&b'^') && open.last() == Some(&(at - 2)) {
            lone.insert(at);
        } else if let Some(opener) = open.pop() {
            at = close_bracket(alt, opener, at, &mut notes, raw);
        } else {
            let swapped = if swap { notes.pop() } else { None };
            lone.insert(swapped.unwrap_or(at));
        }
        at += 1;
    }
    lone.extend(open);
}

/// C2-I-066 to C2-I-069: the end of the link tail whose `(` is at `at` of `alt`; it stands on one line. The writer
/// doubles every backslash before punctuation (C2-W-097), so no backslash of the alt escapes a parenthesis.
fn tail_end(alt: &[u8], at: usize) -> Option<usize> {
    let mut level = 0usize;
    let mut p = at + 1;
    while let Some(&c) = alt.get(p) {
        if is_label_space(c) {
            break;
        }
        if c == b'(' {
            level += 1;
        } else if c == b')' {
            if level == 0 {
                return Some(p + 1);
            }
            level -= 1;
        }
        p += 1;
    }
    if level == 0 && alt.get(p) == Some(&b' ') {
        title_tail_end(alt, p + 1)
    } else {
        None
    }
}

/// C2-I-068: the end of a quoted title at `at` of `alt`, optional spaces and `)`.
fn title_tail_end(alt: &[u8], at: usize) -> Option<usize> {
    let quote = *alt.get(at).filter(|&&q| q == b'"' || q == b'\'')?;
    let mut q = at + 1;
    while alt.get(q).is_some_and(|&c| c != quote && c != b'\n') {
        q += 1;
    }
    if alt.get(q) != Some(&quote) {
        return None;
    }
    q += 1;
    while alt.get(q) == Some(&b' ') {
        q += 1;
    }
    (alt.get(q) == Some(&b')')).then_some(q + 1)
}

/// Closes the pair `opener` at `at` and returns the offset the scan goes on from: a link tail glued to it is an atom.
fn close_bracket(
    alt: &[u8],
    opener: usize,
    at: usize,
    notes: &mut Vec<usize>,
    raw: &[bool],
) -> usize {
    if alt.get(opener + 1) == Some(&b'^') {
        notes.push(at);
        return at;
    }
    if alt.get(at + 1) != Some(&b'(') || raw.get(at + 1) == Some(&true) {
        return at;
    }
    tail_end(alt, at + 1).map_or(at, |end| end - 1)
}

/// The spellings of an alt text that [`alt_source`] can write.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AltForm {
    /// C2-W-097: each bracket without a partner takes a backslash.
    Clause,
    /// No bracket takes one.
    Bare,
    /// As `Clause`, but a lone `]` keeps its own backslash rather than the `]` of a `[^` pair.
    Unswapped,
    /// Every bracket outside raw content takes one.
    Every,
}

/// C2-W-097: alt text doubles a backslash before ASCII punctuation or at its
/// end, and escapes the brackets that `form` names.
fn alt_source(alt: &str, form: AltForm) -> String {
    let bytes = alt.as_bytes();
    let mut lone = AddrSet::default();
    let raw = raw_atoms(alt, &mut lone);
    match form {
        AltForm::Clause => unpartnered(bytes, &raw, &mut lone, true),
        AltForm::Unswapped => unpartnered(bytes, &raw, &mut lone, false),
        AltForm::Every => lone.extend(
            bytes
                .iter()
                .enumerate()
                .filter(|&(at, &c)| raw.get(at) != Some(&true) && (c == b'[' || c == b']'))
                .map(|(at, _)| at),
        ),
        AltForm::Bare => {}
    }
    let mut out = String::with_capacity(alt.len() + 4);
    for (at, ch) in alt.char_indices() {
        let next = bytes.get(at + 1);
        if (ch == '\\' && next.is_none_or(u8::is_ascii_punctuation)) || lone.contains(&at) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// C2-W-089, C2-W-097: the alt text of an image. The clause's bracket count cannot see every atom (a reference label
/// after `[^` is taken whole, C2-I-076), so where its spelling does not read back as the image with its alt, the first
/// of the bare, unswapped and every-bracket forms that does is taken.
fn image_alt(w: &mut Writer<'_>, node: V<'_>) -> String {
    let alt = s(node, "alt").unwrap_or("");
    let spelled = alt_source(alt, AltForm::Clause);
    // C2-W-075: a collapsed or shortcut alt is checked against its key by `collapsed_alt`.
    let keyed = matches!(s(node, "refForm"), Some("collapsed" | "shortcut"));
    if keyed || !may_misread(alt) {
        return spelled;
    }
    let tail = if has(node, "key") { "[k]" } else { "()" };
    if reads_back_alt(w, &spelled, alt, tail) {
        return spelled;
    }
    for form in [AltForm::Bare, AltForm::Unswapped, AltForm::Every] {
        let candidate = alt_source(alt, form);
        if candidate != spelled && reads_back_alt(w, &candidate, alt, tail) {
            return candidate;
        }
    }
    spelled
}

/// Whether `alt` holds a bracket beside a form that changes how brackets pair: a caret (C2-I-087, C2-I-089), a tail or
/// label glued to a closing bracket (C2-I-066, C2-I-072), or the opener of an atom (C2-I-005).
fn may_misread(alt: &str) -> bool {
    alt.contains(['[', ']']) && (alt.contains(['^', '(', '<', '`', '{']) || alt.contains("]["))
}

/// Whether `![spelled]` with `tail` reads as one image whose alt is `alt` (C2-I-074).
fn reads_back_alt(w: &mut Writer<'_>, spelled: &str, alt: &str, tail: &str) -> bool {
    let keys = std::iter::once("k".to_string()).collect();
    let chunk = super::check::Chunk::new(format!("![{spelled}]{tail}\n"));
    children(chunk.parse_keys(w, Some(&keys)))
        .single()
        .and_then(|p| children(p).single())
        .is_some_and(|img| ty(img) == "image" && s(img, "alt") == Some(alt))
}

fn autolink_node(w: &mut Writer<'_>, node: V<'_>) -> String {
    let href = s(node, "href").unwrap_or("");
    let Some(t) = s(node, "text") else {
        return format!("<{href}>");
    };
    if href != format!("mailto:{t}") {
        lose(w, node, Some("text"));
    }
    href.strip_prefix("mailto:")
        .map_or_else(|| format!("<{href}>"), |rest| format!("<{rest}>"))
}

fn mention_node(w: &mut Writer<'_>, node: V<'_>) -> String {
    if has(node, "label") {
        lose(w, node, Some("label"));
    } else if s(node, "kind").is_some_and(|k| k != "user") {
        lose(w, node, Some("kind"));
    }
    format!("@{}", s(node, "target").unwrap_or(""))
}

/// C2-W-022, C2-W-117: a line comment; a value that ends with a space or a tab loses it.
pub fn line_comment_text(w: &mut Writer<'_>, node: V<'_>, value: &str) -> String {
    let kept = trim_end(value);
    if kept.len() != value.len() {
        lose(w, node, Some("value"));
    }
    if kept.is_empty() {
        "%%".into()
    } else {
        format!("%% {kept}")
    }
}

fn comment_node(w: &mut Writer<'_>, node: V<'_>, flags: u32) -> String {
    let value = cell_value(w, node, text(node), flags);
    // C2-W-005, C2-A-074: where a padded line reads as a block opener, the value stands glued to the marks.
    let glued = w.form(node) > 0 && !value.starts_with(' ') && !value.ends_with(' ');
    if s(node, "form") == Some("braced") {
        return if glued {
            format!("{{%{value}%}}")
        } else {
            braced_comment(&value)
        };
    }
    line_comment_text(w, node, &value)
}

fn abbreviation_node(w: &mut Writer<'_>, node: V<'_>) -> String {
    let key = s(node, "key").unwrap_or("");
    // C2-W-117: without a definition of its key the word reads back as text.
    if !w.abbreviations.iter().any(|(k, _)| k == key) {
        lose(w, node, None);
    }
    key.to_string()
}

fn ruby_node<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> String {
    lose(w, node, None);
    let mut out = String::new();
    for pair in super::node::list(node, "pairs").unwrap_or_else(Arr::empty) {
        let base = write_inlines(
            w,
            &refs(super::node::list(pair, "base").unwrap_or_else(Arr::empty)),
            flags,
        );
        let annotation = write_inlines(
            w,
            &refs(super::node::list(pair, "annotation").unwrap_or_else(Arr::empty)),
            flags,
        );
        out.push_str(&base);
        out.push('(');
        out.push_str(&annotation);
        out.push(')');
    }
    out
}

/// C2-A-175: a node with no spelling of its own is its children, or its fallback.
fn children_only<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> String {
    lose(w, node, None);
    let kids = super::node::list(node, "fallback").unwrap_or_else(|| children(node));
    write_inlines(w, &refs(kids), flags)
}

fn inline_extension<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> String {
    let name = s(node, "name").unwrap_or("");
    if !name.contains('.') {
        let content = write_inlines(w, &refs(children(node)), flags);
        return format!(":{name}[{content}]");
    }
    if let Some(source) = node
        .get("source")
        .filter(|src| s(src, "form") == Some("inline"))
    {
        let form = format!(
            ":{}[{}]",
            s(source, "name").unwrap_or(""),
            s(source, "body").unwrap_or("")
        );
        let same = w.extensions.is_empty() || {
            let chunk = super::check::Chunk::new(format!("{form}{}\n", plain_block(node)));
            children(chunk.parse(w)).single().is_some_and(|p| {
                ty(p) == "paragraph"
                    && children(p)
                        .single()
                        .is_some_and(|x| super::node::trees_equal(node, x))
            })
        };
        if same {
            return form;
        }
    }
    lose(w, node, None);
    let kids = super::node::list(node, "fallback").unwrap_or_else(|| children(node));
    write_inlines(w, &refs(kids), flags)
}

fn own<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> Option<String> {
    Some(match ty(node) {
        "text" => text_node(w, node, flags),
        "soft_break" => "\n".into(),
        "hard_break" => if flags & BARE_BREAKS != 0 {
            "\n"
        } else {
            "\\\n"
        }
        .into(),
        "emphasis" => paired(w, node, flags, '/', 1, IN_EMPHASIS),
        "strong" => paired(w, node, flags, '*', 2, IN_STRONG),
        "underline" => braced(w, node, flags, '_', '_'),
        "strike" => braced(w, node, flags, '~', '~'),
        "highlight" => braced(w, node, flags, '=', '='),
        "superscript" => braced(w, node, flags, '^', '^'),
        "subscript" => braced(w, node, flags, ',', ','),
        "insert" => braced(w, node, flags, '+', '+'),
        "delete" => braced(w, node, flags, '-', '-'),
        "critic_comment" => format!("{{#{}#}}", cell_value(w, node, text(node), flags)),
        "span" => format!("[{}]", write_inlines(w, &refs(children(node)), flags)),
        "link" => link_node(w, node, flags),
        "image" => image_node(w, node, flags),
        "inline_footnote" => format!(
            "^[{}]",
            write_inlines(w, &refs(children(node)), flags | IN_NOTE)
        ),
        "inline_extension" => inline_extension(w, node, flags),
        "footnote_ref" => format!("[^{}]", label_text(s(node, "key").unwrap_or(""))),
        "code" => span_value(w, node, flags),
        "literal_inline" => format!("!{}", span_value(w, node, flags)),
        "math" => {
            let mark = if super::node::flag(node, "display") {
                "$$"
            } else {
                "$"
            };
            format!("{mark}{}", span_value(w, node, flags))
        }
        "raw_inline" => format!(
            "{}{{={}}}",
            span_value(w, node, flags),
            s(node, "format").unwrap_or("")
        ),
        "symbol" => format!(":{}:", s(node, "name").unwrap_or("")),
        "mention" => mention_node(w, node),
        "tag" => format!("#{}", s(node, "name").unwrap_or("")),
        "autolink" => autolink_node(w, node),
        "xref" => format!("</#{}>", s(node, "key").unwrap_or("")),
        "non_breaking_space" => "\\ ".into(),
        "smart_punctuation" => s(node, "source").unwrap_or("").to_string(),
        "abbreviation" => abbreviation_node(w, node),
        // The mark of a caption number is the first bare `#` of the caption (C2-B-207).
        "caption_number" => "#".into(),
        "comment" => comment_node(w, node, flags),
        "citation_group" => {
            lose(w, node, None);
            s(node, "raw").unwrap_or("").to_string()
        }
        "ruby" => ruby_node(w, node, flags),
        _ => return None,
    })
}

fn write_inline_into<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32, out: &mut String) {
    if !own_into(w, node, flags, out) {
        out.push_str(&children_only(w, node, flags));
    }
    let no_attrs = match node {
        V::Node(n) => n.attrs().is_none(),
        _ => attrs(node).is_none(),
    };
    if no_attrs && s(node, "lang").is_none() {
        if ty(node) == "span" {
            out.push_str("{}");
        }
        return;
    }
    // C2-W-076: the attribute block is glued to its host. C2-I-040: a span takes the block with no part.
    let block = brace_block(
        node,
        &BlockParts {
            pipes: flags & IN_CELL != 0,
            ..BlockParts::default()
        },
    );
    if ty(node) == "span" && block.is_empty() {
        out.push_str("{}");
    } else {
        out.push_str(&block);
    }
}

/// [`own`] written onto `out`; false, with nothing written, where `own` gives `None`.
fn own_into<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32, out: &mut String) -> bool {
    let (open, close) = match ty(node) {
        "underline" => ('_', '_'),
        "strike" => ('~', '~'),
        "highlight" => ('=', '='),
        "superscript" => ('^', '^'),
        "subscript" => (',', ','),
        "insert" => ('+', '+'),
        "delete" => ('-', '-'),
        "emphasis" => return paired_into(w, node, flags, ('/', 1, IN_EMPHASIS), out),
        "strong" => return paired_into(w, node, flags, ('*', 2, IN_STRONG), out),
        _ => {
            let Some(part) = own(w, node, flags) else {
                return false;
            };
            out.push_str(&part);
            return true;
        }
    };
    let c = children(node);
    if c.is_empty() {
        lose(w, node, None);
        return true;
    }
    out.push('{');
    out.push(open);
    write_kids_into(w, c, flags, out);
    out.push(close);
    out.push('}');
    true
}

fn write_kids_into<'t>(w: &mut Writer<'t>, kids: Arr<'t>, flags: u32, out: &mut String) {
    let mut list = w.kid_lists.pop().unwrap_or_default();
    list.extend(kids.iter());
    write_inlines_into(w, &list, flags, out);
    list.clear();
    w.kid_lists.push(list);
}

/// [`paired`] written onto `out`: the opener goes in once the content is written.
fn paired_into<'t>(
    w: &mut Writer<'t>,
    node: N<'t>,
    flags: u32,
    (mark, bit, inside): (char, u8, u32),
    out: &mut String,
) -> bool {
    let c = children(node);
    if c.is_empty() {
        lose(w, node, None);
        return true;
    }
    let start = out.len();
    write_kids_into(w, c, flags | inside, out);
    if braced_pair(w, node, flags, bit, inside) {
        out.insert(start, mark);
        out.insert(start, '{');
        out.push(mark);
        out.push('}');
    } else {
        out.insert(start, mark);
        out.push(mark);
    }
    true
}

/// C2-W-112: the code span `nodes[at]` has the code span behind it glued to it.
pub fn joins_next_code(nodes: &[N<'_>], at: usize) -> bool {
    let Some(&node) = nodes.get(at) else {
        return false;
    };
    ty(node) == "code"
        && plain_block(node).is_empty()
        && nodes.get(at + 1).is_some_and(|n| ty(n) == "code")
}

fn part_end(w: &mut Writer<'_>, nodes: &[N<'_>], at: usize) -> usize {
    if !joins_next_code(nodes, at) {
        return at + 1;
    }
    let mut end = at + 1;
    while joins_next_code(nodes, end - 1) {
        if let Some(&n) = nodes.get(end) {
            lose(w, n, None);
        }
        end += 1;
    }
    end
}

/// C2-W-112: code spans with nothing between them are written as one span.
fn code_run(nodes: &[N<'_>], from: usize, end: usize) -> String {
    let mut value = String::new();
    for n in nodes.get(from..end).unwrap_or(&[]) {
        if ty(n) == "code" {
            value.push_str(text(n));
        }
    }
    let last = nodes
        .get(end.saturating_sub(1))
        .map_or(String::new(), |n| plain_block(*n));
    code_span(&value) + &last
}

fn unspelled_space(w: &mut Writer<'_>, node: V<'_>) -> String {
    lose(w, node, None);
    "\u{a0}".into()
}

fn write_part_into<'t>(
    w: &mut Writer<'t>,
    nodes: &[N<'t>],
    (at, end): (usize, usize),
    flags: u32,
    out: &mut String,
) {
    let Some(&node) = nodes.get(at) else {
        return;
    };
    let next = nodes.get(at + 1).copied();
    if ty(node) == "non_breaking_space" && super::node::is(next, "soft_break") {
        out.push_str(&unspelled_space(w, node));
        return;
    }
    let start = out.len();
    if end > at + 1 {
        out.push_str(&code_run(nodes, at, end));
    } else {
        write_inline_into(w, node, flags, out);
    }
    // C2-W-022: no space or tab of text stands at the end of a line.
    if ty(node) == "text" && super::node::is(next, "soft_break") {
        let kept = trim_end(out.get(start..).unwrap_or("")).len();
        out.truncate(start + kept);
    }
}

/// C2-W-069: a bare mark glued to the same mark before it makes one run, so the node is braced.
fn glued_part_into<'t>(
    w: &mut Writer<'t>,
    nodes: &[N<'t>],
    (at, end): (usize, usize),
    flags: u32,
    before: Option<char>,
    out: &mut String,
) {
    let Some(&node) = nodes.get(at) else {
        return;
    };
    let mark = match ty(node) {
        "emphasis" => Some('/'),
        "strong" => Some('*'),
        _ => None,
    };
    if let (Some(m), Some(next)) = (mark, nodes.get(end)) {
        if ty(next) == "text" && text(next).starts_with(m) {
            w.brace(node);
        }
    }
    let start = out.len();
    write_part_into(w, nodes, (at, end), flags, out);
    let part = out.get(start..).unwrap_or("");
    if mark.is_none() || part.is_empty() {
        return;
    }
    let strict = w.strict_pairs
        && !w.is_lost(node)
        && cannot_pair(part, mark, before, nodes.get(end).copied());
    if part.chars().next() != before && !strict {
        return;
    }
    out.truncate(start);
    // A strict pass is a one-off, so only a form the glue alone asked for is decided again (carve#6420).
    if !strict && w.form(node) == 0 {
        w.glued.insert(id(node));
    }
    w.brace(node);
    write_part_into(w, nodes, (at, end), flags, out);
}

/// C2-W-069: a bare pair whose opener cannot open or whose closer cannot close at its neighbors
/// (C2-I-047, C2-I-048). A character before the opener that an escape could join is left to the repair.
fn cannot_pair(part: &str, mark: Option<char>, before: Option<char>, next: Option<N<'_>>) -> bool {
    let Some(content) = mark
        .and_then(|m| part.strip_prefix(m)?.strip_suffix(m))
        .filter(|content| !content.is_empty())
    else {
        return false;
    };
    if before.is_some_and(|b| "\\!^$`{".contains(b)) {
        return false;
    }
    let space = |c: Option<char>| c.is_none_or(crate::chars::is_flank_space);
    let after = next
        .filter(|n| ty(*n) == "text")
        .and_then(|n| text(n).chars().next());
    before.is_some_and(crate::chars::is_word)
        || space(content.chars().next())
        || space(content.chars().last())
        || after.is_some_and(crate::chars::is_word)
}

/// C2-I-102: in a hard-break block a line ending alone at the start of a line is a blank line.
pub fn line_start_break(node: V<'_>, line_start: bool) -> Option<&'static str> {
    (ty(node) == "hard_break" && line_start).then_some("\\\n")
}

fn unclosed_braces(value: &str) -> Vec<usize> {
    let b = value.as_bytes();
    let mut open: Vec<(u8, usize)> = Vec::new();
    let mut at = 0;
    while at + 1 < b.len() {
        let c = b.get(at).copied().unwrap_or(0);
        let n = b.get(at + 1).copied().unwrap_or(0);
        if c == b'\\' && n.is_ascii_punctuation() {
            at += 2;
            continue;
        }
        if c == b'{' && b"*/-=".contains(&n) {
            open.push((n, at));
        } else if n == b'}' && open.last().is_some_and(|o| o.0 == c) {
            open.pop();
        }
        at += 1;
    }
    open.into_iter().map(|o| o.1).collect()
}

fn brace_before_comment_closer(
    w: &mut Writer<'_>,
    node: V<'_>,
    comment: Option<V<'_>>,
    after: Option<V<'_>>,
    flags: u32,
) {
    let Some(comment) = comment else { return };
    if ty(comment) != "comment"
        || s(comment, "form") != Some("line")
        || after.is_some()
        || flags & (IN_NOTE | IN_BRACKET) == 0
    {
        return;
    }
    for at in unclosed_braces(text(node)) {
        w.add_escape(node, at);
    }
}

fn span_closing_bracket(w: &mut Writer<'_>, node: V<'_>, before: Option<V<'_>>) {
    let Some(before) = before else { return };
    if !matches!(ty(before), "strong" | "emphasis") || !text(node).contains(']') {
        return;
    }
    let opens = children(before).iter().any(|c| {
        if ty(c) != "text" {
            return false;
        }
        let v = text(c);
        v.strip_prefix("^[").is_some_and(|rest| {
            rest.find('[')
                .is_some_and(|open| rest.get(..open).is_some_and(|r| !r.contains(']')))
        })
    });
    if !opens {
        return;
    }
    if let Some(closing) = text(node).find(']') {
        if !w.escapes_of(node).contains(&closing) {
            w.add_escape(node, closing);
        }
    }
}

/// C2-W-097: a backslash that ends a text is written as two where what follows
/// it is ASCII punctuation, a space or the end of a line.
fn double_last_backslash<'t>(
    w: &mut Writer<'t>,
    node: N<'t>,
    nodes: &[N<'t>],
    next: usize,
    flags: u32,
) {
    let value = text(node);
    if !value.ends_with('\\') {
        return;
    }
    let last = value.len() - 1;
    if w.escapes_of(node).last() == Some(&last) {
        return;
    }
    if let Some(&after) = nodes.get(next) {
        if !matches!(ty(after), "soft_break" | "hard_break") {
            let end = part_end(w, nodes, next);
            let mut spelled = String::new();
            write_part_into(w, nodes, (next, end), flags, &mut spelled);
            let first = spelled.bytes().next();
            if first.is_some_and(|c| c != b' ' && !c.is_ascii_punctuation()) {
                return;
            }
        }
    }
    w.sp.escapes.entry(id(node)).or_default().push(last);
}

/// C2-W-125 case six: a backtick of text glued behind a span lengthens its closing run.
fn escape_glued_backtick(w: &mut Writer<'_>, node: V<'_>, before: Option<V<'_>>) {
    let Some(before) = before else { return };
    if !text(node).starts_with('`')
        || has(before, "attrs")
        || !matches!(ty(before), "code" | "literal_inline" | "math")
    {
        return;
    }
    if w.escapes_of(node).first() != Some(&0) {
        w.add_escape(node, 0);
    }
}

fn place_escapes<'t>(
    w: &mut Writer<'t>,
    nodes: &[N<'t>],
    at: usize,
    end: usize,
    flags: u32,
    after_bang: bool,
) {
    let Some(&node) = nodes.get(at) else { return };
    if ty(node) == "text" {
        double_last_backslash(w, node, nodes, end, flags);
        escape_glued_backtick(
            w,
            node,
            at.checked_sub(1).and_then(|i| nodes.get(i)).copied(),
        );
        brace_before_comment_closer(
            w,
            node,
            nodes.get(end).copied(),
            nodes.get(end + 1).copied(),
            flags,
        );
        span_closing_bracket(
            w,
            node,
            at.checked_sub(1).and_then(|i| nodes.get(i)).copied(),
        );
        // The readback rounds decide whether this brace actually opens a pair.
    }
    // Behind an unescaped `!` the bracket reads as span text whatever the caret starts.
    if ty(node) == "span" && !after_bang {
        escape_leading_caret(w, children(node), flags);
    }
}

/// Writes one node without placing escapes: the escapes a list places for its end would land in the writer.
pub fn write_inline<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> String {
    let mut out = String::new();
    write_inline_into(w, node, flags, &mut out);
    out
}

pub fn write_inlines<'t>(w: &mut Writer<'t>, nodes: &[N<'t>], flags: u32) -> String {
    #[cfg(test)]
    super::INLINE_BUFFERS.with(|c| c.set(c.get() + 1));
    let mut out = String::new();
    write_inlines_into(w, nodes, flags, &mut out);
    out
}

fn write_inlines_into<'t>(w: &mut Writer<'t>, nodes: &[N<'t>], flags: u32, out: &mut String) {
    let base = out.len();
    let mut line_start = true;
    let mut last: Option<char> = None;
    let mut at = 0;
    while at < nodes.len() {
        let Some(&node) = nodes.get(at) else { break };
        // C2-I-023: a line comment starts at the start of a line or after whitespace.
        if ty(node) == "comment" && s(node, "form") != Some("braced") && !line_start {
            out.push(' ');
            last = Some(' ');
        }
        // C2-L-021: a fragment of a continuation row starts at this node.
        if wraps_at(w, node, flags) {
            if out.len() > base && out.ends_with(' ') {
                out.pop();
            }
            out.push('\n');
            line_start = true;
            last = Some('\n');
        }
        let end = part_end(w, nodes, at);
        let after_bang = last == Some('!')
            && !out
                .get(base..out.len().saturating_sub(1))
                .is_some_and(|o| o.ends_with('\\'));
        place_escapes(w, nodes, at, end, flags, after_bang);
        let start = out.len();
        inline_part_into(w, nodes, (at, end), flags, (line_start, last), out);
        let part = out.get(start..).unwrap_or("");
        if let Some(c) = part.chars().last() {
            last = Some(c);
        }
        line_start = last == Some('\n') || (line_start && part.is_empty());
        at = end;
    }
}

fn wraps_at(w: &Writer<'_>, node: V<'_>, flags: u32) -> bool {
    flags & IN_CELL != 0
        && w.wrap_starts
            .as_ref()
            .is_some_and(|s| s.contains(&id(node)))
}

fn inline_part_into<'t>(
    w: &mut Writer<'t>,
    nodes: &[N<'t>],
    (at, end): (usize, usize),
    flags: u32,
    (line_start, last): (bool, Option<char>),
    out: &mut String,
) {
    let Some(&node) = nodes.get(at) else {
        return;
    };
    if let Some(b) = line_start_break(node, line_start) {
        out.push_str(b);
    } else if ty(node) == "hard_break"
        && flags & BARE_BREAKS != 0
        && matches!(last, Some(' ' | '\t'))
    {
        out.push_str("\\\n");
    } else {
        glued_part_into(w, nodes, (at, end), flags, last, out);
    }
}

/// The source of the content of a text leaf: no space or tab of text stands at its end (C2-W-022).
pub fn write_content<'t>(w: &mut Writer<'t>, nodes: &[N<'t>], flags: u32) -> String {
    let Some(&last) = nodes.last() else {
        return String::new();
    };
    if ty(last) == "non_breaking_space" {
        let head = write_inlines(w, nodes.get(..nodes.len() - 1).unwrap_or(&[]), flags);
        return head + &unspelled_space(w, last);
    }
    let out = write_inlines(w, nodes, flags);
    if ty(last) == "text" {
        return trim_end(&out).to_string();
    }
    if ty(last) != "hard_break" || !out.ends_with('\n') {
        return out;
    }
    let b = out.as_bytes();
    let before = b.len().checked_sub(3).and_then(|i| b.get(i)).copied();
    let spelled =
        out.ends_with("\\\n") && (out.len() == 2 || matches!(before, Some(b'\n' | b' ' | b'\t')));
    let trimmed = out.get(..out.len() - 1).unwrap_or("");
    if flags & BARE_BREAKS != 0 && !spelled {
        format!("{trimmed}\\")
    } else {
        trimmed.to_string()
    }
}
