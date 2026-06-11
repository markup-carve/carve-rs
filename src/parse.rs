//! Carve parser (MVP subset).
//!
//! Block-level reads line by line; inline does a single linear scan
//! over each block's text. No backtracking.

use crate::ast::*;
use crate::extension::{BlockMatch, InlineMatch, MatcherContext, Options};
use std::collections::BTreeMap;

pub fn parse(source: &str) -> Document {
    parse_with_options(source, &Options::default())
}

pub fn parse_with_options(source: &str, options: &Options<'_>) -> Document {
    let (frontmatter, body) = split_frontmatter(source);
    let (body, footnote_defs_src) = extract_footnote_defs(body);
    let (body, link_defs) = extract_link_defs(&body);
    let footnote_defs = footnote_defs_src
        .into_iter()
        .map(|(label, source)| (label, parse_blocks_with_options(&source, options)))
        .collect();
    let children = parse_blocks_with_options(&body, options);
    let mut doc = Document {
        frontmatter,
        footnote_defs,
        children,
    };
    resolve_reference_links(&mut doc, &link_defs);
    apply_abbreviations(&mut doc);
    resolve_crossrefs(&mut doc);
    for ext in &options.extensions {
        doc = ext.after_parse(doc);
    }
    doc
}

fn extract_footnote_defs(source: &str) -> (String, BTreeMap<String, String>) {
    let lines: Vec<&str> = source.lines().collect();
    let mut body = Vec::new();
    let mut defs = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some((label, first)) = parse_footnote_def_line(lines[i]) {
            i += 1;
            let mut def_lines = vec![first.to_string()];
            while i < lines.len() {
                let line = lines[i];
                if parse_footnote_def_line(line).is_some() {
                    break;
                }
                if line.trim().is_empty() {
                    if i + 1 < lines.len() && leading_ws(lines[i + 1]) >= 4 {
                        def_lines.push(String::new());
                        i += 1;
                        continue;
                    }
                    break;
                }
                if leading_ws(line) > 0 {
                    def_lines.push(line.trim_start().to_string());
                    i += 1;
                    continue;
                }
                break;
            }
            defs.insert(label.to_string(), def_lines.join("\n"));
            // Leave a blank line where the (invisible) definition was, so it
            // still acts as a block boundary — a following paragraph or a
            // lazy blockquote continuation does not absorb across it.
            body.push("");
        } else {
            body.push(lines[i]);
            i += 1;
        }
    }
    (body.join("\n"), defs)
}

fn parse_footnote_def_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("[^")?;
    let (label, body) = rest.split_once("]:")?;
    Some((label, body.trim_start()))
}

#[derive(Clone)]
struct LinkDef {
    href: String,
    title: Option<String>,
}

fn extract_link_defs(source: &str) -> (String, BTreeMap<String, LinkDef>) {
    let mut body = Vec::new();
    let mut defs = BTreeMap::new();
    for line in source.lines() {
        if let Some((label_part, target_part)) =
            line.strip_prefix('[').and_then(|s| s.split_once("]:"))
        {
            defs.insert(
                label_part.to_string(),
                parse_link_def_target(target_part.trim()),
            );
            // Leave a blank line in place of the (invisible) definition so it
            // still acts as a block boundary (matches carve-js, where a
            // definition interrupts a paragraph / ends a lazy blockquote).
            body.push("");
        } else {
            body.push(line);
        }
    }
    (body.join("\n"), defs)
}

fn parse_link_def_target(target: &str) -> LinkDef {
    let bytes = target.as_bytes();
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let href = target[..i].to_string();
    let rest = target[i..].trim();
    let title = if (rest.starts_with('"') && rest.ends_with('"'))
        || (rest.starts_with('\'') && rest.ends_with('\''))
    {
        Some(rest[1..rest.len() - 1].to_string())
    } else {
        None
    };
    LinkDef { href, title }
}

fn split_frontmatter(source: &str) -> (BTreeMap<String, String>, &str) {
    // Opening fence: `---` optionally followed by a type token (`---yaml`,
    // `---json`, `---toml`, ...; canonical has no space). Closer is a bare `---`.
    if !source.starts_with("---") {
        return (BTreeMap::new(), source);
    }
    let Some(first_nl) = source.find('\n') else {
        return (BTreeMap::new(), source);
    };
    let kind = source[3..first_nl].trim();
    if !kind.is_empty() && !kind.chars().all(|c| c.is_ascii_alphanumeric()) {
        return (BTreeMap::new(), source);
    }
    let rest = &source[first_nl + 1..];
    let Some(close) = rest.find("\n---\n") else {
        return (BTreeMap::new(), source);
    };
    let frontmatter_src = &rest[..close];
    let body = &rest[close + 5..];
    let mut frontmatter = BTreeMap::new();
    // Only the bare / yaml form is key:value; typed blocks (json/toml) are
    // structured and just stripped.
    if kind.is_empty() || kind.eq_ignore_ascii_case("yaml") {
        for line in frontmatter_src.lines() {
            if let Some((key, value)) = line.split_once(':') {
                frontmatter.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    (frontmatter, body)
}

pub(crate) fn parse_blocks_with_options(source: &str, options: &Options<'_>) -> Vec<BlockNode> {
    let mut lines: Vec<&str> = source.lines().collect();
    // `lines()` already drops a single trailing newline; nothing more to do.
    let _ = &mut lines;

    let mut cursor = LineCursor {
        lines: &lines,
        pos: 0,
    };
    parse_blocks(&mut cursor, options)
}

struct LineCursor<'a> {
    lines: &'a [&'a str],
    pos: usize,
}

impl<'a> LineCursor<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.pos).copied()
    }
    fn consume(&mut self) -> Option<&'a str> {
        let line = self.peek();
        if line.is_some() {
            self.pos += 1;
        }
        line
    }
    fn eof(&self) -> bool {
        self.pos >= self.lines.len()
    }
}

fn parse_blocks(cur: &mut LineCursor, options: &Options<'_>) -> Vec<BlockNode> {
    let mut out = Vec::new();
    let mut pending_attrs: Option<Attrs> = None;
    while !cur.eof() {
        let line = cur.peek().unwrap();
        if line.trim().is_empty() {
            cur.consume();
            continue;
        }
        if line.trim_start().starts_with("%%%") {
            cur.consume();
            while let Some(line) = cur.peek() {
                cur.consume();
                if line.trim_start().starts_with("%%%") {
                    break;
                }
            }
            continue;
        }
        if line.trim_start().starts_with("%%") {
            cur.consume();
            continue;
        }
        if let Some(attrs) = parse_standalone_attrs_block(cur) {
            merge_attrs(&mut pending_attrs, attrs);
            continue;
        }
        if let Some(node) = parse_block(cur, options) {
            let mut node = node;
            if let Some(attrs) = pending_attrs.take() {
                apply_attrs_to_block(&mut node, attrs);
            }
            out.push(node);
        }
    }
    out
}

fn parse_block(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    let line = cur.peek()?;
    if let Some(fence_marker) = detect_fence_open(line) {
        return Some(parse_fence(cur, fence_marker));
    }
    if detect_thematic_break(line) {
        cur.consume();
        return Some(BlockNode::ThematicBreak);
    }
    if let Some((level, first_text)) = detect_heading(line) {
        cur.consume();
        // Headings are multi-line (like Djot, and like blockquotes): the text
        // spills onto following lines until a blank line. A continuation line
        // may carry the same-or-lower number of `#` (stripped) or none; a
        // higher/other heading marker starts a new heading, and a caption or a
        // fenced comment (`%%%`) ends it. Per §10 nothing else interrupts.
        let mut joined = first_text.to_string();
        while let Some(next) = cur.peek() {
            if next.trim().is_empty() {
                break;
            }
            if let Some(cont) = heading_continuation_same_or_lower(next, level) {
                joined.push('\n');
                joined.push_str(cont);
                cur.consume();
                continue;
            }
            if is_heading_marker_line(next) || next.starts_with("^ ") || is_comment_fence_line(next)
            {
                break;
            }
            joined.push('\n');
            joined.push_str(next);
            cur.consume();
        }
        let (text, attrs) = split_trailing_attrs(&joined);
        return Some(BlockNode::Heading(Heading {
            attrs,
            level,
            children: parse_inline_with_options(text, options),
        }));
    }
    if line.starts_with('>') {
        return Some(parse_blockquote(cur, options));
    }
    if is_list_marker(line) {
        return Some(parse_list(cur, options));
    }
    if is_table_start(line) {
        return Some(parse_table(cur, options));
    }
    if is_definition_list_start(line) {
        return Some(parse_definition_list(cur, options));
    }
    if detect_container_open(line).is_some() {
        return Some(parse_container(cur, options));
    }
    if let Some(abbr) = detect_abbreviation_def(line) {
        cur.consume();
        return Some(BlockNode::AbbreviationDef(abbr));
    }
    if let Some(img) = detect_block_image(line) {
        cur.consume();
        if let Some(caption) = consume_caption(cur, options) {
            return Some(BlockNode::Figure(Figure {
                attrs: None,
                target: FigureTarget::Image(img),
                caption,
            }));
        }
        return Some(BlockNode::BlockImage(img));
    }
    if let Some(matched) = try_extension_block(cur, options) {
        return Some(matched);
    }
    Some(parse_paragraph(cur, options))
}

fn detect_heading(line: &str) -> Option<(u8, &str)> {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if !(1..=6).contains(&hashes) {
        return None;
    }
    if hashes >= bytes.len() || bytes[hashes] != b' ' {
        return None;
    }
    // Skip all spaces after the marker (the delimiter is one-or-more spaces;
    // per the Carve grammar it is a space, not a tab).
    let mut start = hashes;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    let rest = line[start..].trim_end();
    if rest.is_empty() {
        return None;
    }
    Some((hashes as u8, rest))
}

/// A heading continuation line carrying 1..=`level` `#` markers, a space, then
/// non-empty text. Returns the text after the markers (markers stripped), as
/// in Djot ("may be preceded by the same number of `#` characters").
fn heading_continuation_same_or_lower(line: &str, level: u8) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if hashes == 0 || hashes > level as usize {
        return None;
    }
    if hashes >= bytes.len() || bytes[hashes] != b' ' {
        return None;
    }
    // Skip all spaces after the marker, mirroring detect_heading.
    let mut start = hashes;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    let rest = line[start..].trim_end();
    if rest.is_empty() {
        return None;
    }
    Some(rest)
}

/// Any ATX heading marker line (`#`..`######` followed by a space or EOL) —
/// such a line starts a NEW heading rather than continuing the current one.
fn is_heading_marker_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    (1..=6).contains(&hashes) && (hashes == bytes.len() || bytes[hashes] == b' ')
}

/// A fenced-comment opener line (a run of 3+ `%`, nothing else) — ends a heading.
fn is_comment_fence_line(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 3 && t.bytes().all(|b| b == b'%')
}

fn detect_thematic_break(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    trimmed.bytes().all(|b| b == b'-')
        || trimmed.bytes().all(|b| b == b'*')
        || trimmed.bytes().all(|b| b == b'_')
}

#[derive(Debug, Clone, Copy)]
struct FenceOpen {
    indent: usize,
    fence_char: u8,
    fence_len: usize,
    lang_start: usize,
    lang_end: usize,
}

fn detect_fence_open(line: &str) -> Option<FenceOpen> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let indent = i;
    if i >= bytes.len() {
        return None;
    }
    let fence_char = bytes[i];
    if fence_char != b'`' && fence_char != b'~' {
        return None;
    }
    let fence_start = i;
    while i < bytes.len() && bytes[i] == fence_char {
        i += 1;
    }
    let fence_len = i - fence_start;
    if fence_len < 3 {
        return None;
    }
    // Optional whitespace then language identifier
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    let lang_start = i;
    // Language token charset covers real-world tags with punctuation
    // (c++, c#, f#, asp.net); the token is still anchored (no whitespace),
    // so a multiword/quoted info string is not a fence.
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric()
            || bytes[i] == b'_'
            || bytes[i] == b'-'
            || bytes[i] == b'+'
            || bytes[i] == b'#'
            || bytes[i] == b'.')
    {
        i += 1;
    }
    let lang_end = i;
    if &line[lang_start..lang_end] == "raw" {
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
        {
            i += 1;
        }
    } else {
        // Optional bracketed [label] after the language token (info string =
        // language token + optional [label]); the label is metadata and does
        // not affect the language class.
        let mut j = i;
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'[' {
            while j < bytes.len() && bytes[j] != b']' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' {
                i = j + 1;
            }
        }
    }
    // Must be only whitespace after the info string
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i != bytes.len() {
        return None;
    }
    Some(FenceOpen {
        indent,
        fence_char,
        fence_len,
        lang_start,
        lang_end,
    })
}

fn parse_fence(cur: &mut LineCursor, open: FenceOpen) -> BlockNode {
    let open_line = cur.consume().unwrap();
    let open_trim = open_line[open.lang_start..].trim();
    let raw_format = open_trim.strip_prefix("raw ").map(str::to_string);
    let lang = if raw_format.is_none() && open.lang_start < open.lang_end {
        Some(open_line[open.lang_start..open.lang_end].to_string())
    } else {
        None
    };
    let mut content_lines: Vec<String> = Vec::new();
    while let Some(line) = cur.peek() {
        if is_fence_close(line, open) {
            cur.consume();
            break;
        }
        cur.consume();
        // Strip leading whitespace up to the opening fence's indent
        let strip = leading_ws(line).min(open.indent);
        content_lines.push(line[strip..].to_string());
    }
    if let Some(format) = raw_format {
        BlockNode::RawBlock(RawBlock {
            format,
            content: content_lines.join("\n"),
        })
    } else {
        BlockNode::CodeBlock(CodeBlock {
            attrs: None,
            lang,
            content: content_lines.join("\n"),
        })
    }
}

fn is_fence_close(line: &str, open: FenceOpen) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i > 3 {
        return false;
    }
    let start = i;
    while i < bytes.len() && bytes[i] == open.fence_char {
        i += 1;
    }
    if i - start < open.fence_len {
        return false;
    }
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    i == bytes.len()
}

fn leading_ws(line: &str) -> usize {
    line.bytes()
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count()
}

// Visual column of the leading whitespace, expanding tabs to the next
// CommonMark tab stop (a multiple of 4). For space-only indentation this
// equals leading_ws(). Used for list-nesting comparisons.
fn indent_columns(line: &str) -> usize {
    let mut col = 0;
    for b in line.bytes() {
        match b {
            b' ' => col += 1,
            b'\t' => col += 4 - (col % 4),
            _ => break,
        }
    }
    col
}

// Drop leading whitespace up to `cols` columns (tab-stop aware) and return the
// remainder. By default a tab straddling the boundary is consumed whole, so a
// block opener (quote, heading) dedents flush to column 0 and parses -- Carve
// has no indent-sensitive block where the leftover column would change meaning.
// With keep_residual (used only for sub-list marker lines), the unconsumed
// columns of a straddling tab are re-emitted as spaces so tab+space-aligned
// sibling markers keep the same visual column and the recursive parse re-derives
// the child base from it. For space-only indentation there is never a residual.
fn slice_columns(line: &str, cols: usize, keep_residual: bool) -> String {
    let bytes = line.as_bytes();
    let mut col = 0;
    let mut i = 0;
    while i < bytes.len() && col < cols {
        match bytes[i] {
            b' ' => {
                col += 1;
                i += 1;
            }
            b'\t' => {
                col += 4 - (col % 4);
                i += 1;
            }
            _ => break,
        }
    }
    if keep_residual && col > cols {
        let mut s = " ".repeat(col - cols);
        s.push_str(&line[i..]);
        s
    } else {
        line[i..].to_string()
    }
}

fn parse_blockquote(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let mut inner = Vec::new();
    let mut para_open = false;
    let mut in_fence: Option<FenceOpen> = None;
    while let Some(line) = cur.peek() {
        if let Some(rest) = line.strip_prefix('>') {
            cur.consume();
            let stripped = rest.strip_prefix(' ').unwrap_or(rest);
            if let Some(open) = in_fence {
                if is_fence_close(stripped, open) {
                    in_fence = None;
                }
                para_open = false;
            } else if let Some(open) = detect_fence_open(stripped) {
                if !para_open {
                    // Fence at block start opens (unterminated renders to end).
                    in_fence = Some(open);
                    para_open = false;
                } else {
                    // After an open paragraph a fence interrupts only with a
                    // matching closer ahead (§10); else it is inline verbatim.
                    let has_closer = cur.lines[cur.pos..]
                        .iter()
                        .take_while(|l| l.starts_with('>'))
                        .any(|l| {
                            let s = l.strip_prefix('>').unwrap_or(l);
                            is_fence_close(s.strip_prefix(' ').unwrap_or(s), open)
                        });
                    if has_closer {
                        in_fence = Some(open);
                        para_open = false;
                    }
                }
            } else {
                para_open = !stripped.trim().is_empty()
                    && detect_container_open(stripped).is_none()
                    && !stripped.trim_start().starts_with("%%");
            }
            inner.push(stripped.to_string());
            continue;
        }
        // Lazy continuation: a non-`>` line folds into an OPEN paragraph. A
        // blank line, a caption, or a line that starts a block ends the quote.
        if !para_open
            || line.trim().is_empty()
            || line.starts_with("^ ")
            || interrupts_paragraph(line, &cur.lines[cur.pos + 1..])
        {
            break;
        }
        cur.consume();
        inner.push(line.to_string());
    }
    let joined = inner.join("\n");
    let sub_lines: Vec<&str> = joined.lines().collect();
    let mut sub_cursor = LineCursor {
        lines: &sub_lines,
        pos: 0,
    };
    let children = parse_blocks(&mut sub_cursor, options);
    let quote = BlockQuote {
        attrs: None,
        children,
        attribution: None,
    };
    if let Some(caption) = consume_caption(cur, options) {
        BlockNode::Figure(Figure {
            attrs: None,
            target: FigureTarget::BlockQuote(quote),
            caption,
        })
    } else {
        BlockNode::BlockQuote(quote)
    }
}

fn is_list_marker(line: &str) -> bool {
    detect_task(line).is_some()
        || detect_unordered(line).is_some()
        || detect_ordered(line).is_some()
}

fn detect_unordered(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    // `+` is the list continuation marker, not a bullet (#80).
    if c != b'-' && c != b'*' {
        return None;
    }
    if i + 1 >= bytes.len() || bytes[i + 1] != b' ' {
        return None;
    }
    Some(line[i + 2..].trim_end())
}

fn detect_ordered(line: &str) -> Option<&str> {
    detect_ordered_full(line).map(|(content, _, _)| content)
}

fn detect_ordered_full(line: &str) -> Option<(&str, Option<usize>, Option<OrderedListType>)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let marker_start = i;
    while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
        i += 1;
    }
    if i == marker_start {
        return None;
    }
    if i + 1 >= bytes.len() || (bytes[i] != b'.' && bytes[i] != b')') || bytes[i + 1] != b' ' {
        return None;
    }
    let marker = &line[marker_start..i];
    if marker.bytes().all(|b| b.is_ascii_digit()) {
        return Some((
            line[i + 2..].trim_end(),
            marker.parse::<usize>().ok().filter(|n| *n != 1),
            None,
        ));
    }
    if marker.len() == 1 {
        let b = marker.as_bytes()[0];
        if b.is_ascii_lowercase() {
            return Some((
                line[i + 2..].trim_end(),
                Some((b - b'a' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::LowerAlpha),
            ));
        }
        if b.is_ascii_uppercase() {
            return Some((
                line[i + 2..].trim_end(),
                Some((b - b'A' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::UpperAlpha),
            ));
        }
    }
    let roman = roman_to_int(marker)?;
    Some((
        line[i + 2..].trim_end(),
        Some(roman).filter(|n| *n != 1),
        Some(if marker.chars().all(|c| c.is_ascii_uppercase()) {
            OrderedListType::UpperRoman
        } else {
            OrderedListType::LowerRoman
        }),
    ))
}

fn roman_to_int(s: &str) -> Option<usize> {
    let mut total = 0isize;
    let mut prev = 0isize;
    for ch in s.chars().rev() {
        let val = match ch.to_ascii_lowercase() {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => return None,
        };
        if val < prev {
            total -= val;
        } else {
            total += val;
            prev = val;
        }
    }
    (total > 0).then_some(total as usize)
}

fn detect_task(line: &str) -> Option<(bool, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    // `+` is the list continuation marker, not a bullet (#80).
    if c != b'-' && c != b'*' {
        return None;
    }
    if i + 1 >= bytes.len() || bytes[i + 1] != b' ' {
        return None;
    }
    i += 2;
    if i + 3 >= bytes.len() || bytes[i] != b'[' {
        return None;
    }
    let marker = bytes[i + 1];
    if bytes[i + 2] != b']' || bytes[i + 3] != b' ' {
        return None;
    }
    let checked = matches!(marker, b'x' | b'X');
    Some((checked, line[i + 4..].trim_end()))
}

fn parse_list(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let first = cur.peek().unwrap();
    let first_marker = detect_list_marker_full(first).unwrap();
    let base_indent = first_marker.indent;
    let is_task = first_marker.checked.is_some();
    let is_ordered = first_marker.ordered;
    let start = first_marker.start;
    let ol_type = first_marker.ol_type;
    let mut items: Vec<ListItem> = Vec::new();
    let mut tight = true;
    let mut pending_blank = false;
    // The current item's content column (where its content begins after the
    // marker). Nested content and sub-blocks of the last item dedent by this, so
    // it persists across iterations and is updated as each item is opened.
    let mut content_col = base_indent + 2;
    while let Some(line) = cur.peek() {
        if line.trim().is_empty() {
            // A blank alone does not loosen the list; it loosens only when the
            // next line is a sibling item (handled at the marker branch via
            // pending_blank) or an indented second paragraph (below). A blank
            // before any other indented block keeps it compact (#74).
            pending_blank = true;
            cur.consume();
            continue;
        }
        // Lone `+` continuation marker (Carve): attaches the next flush-left
        // block to the current item without indentation.
        if line.trim() == "+" && indent_columns(line) == base_indent {
            cur.consume();
            pending_blank = false;
            if let Some(block) = parse_block(cur, options) {
                if let Some(last) = items.last_mut() {
                    last.children.push(block);
                }
            }
            continue;
        }
        let Some(marker) = detect_list_marker_full(line) else {
            let indent = indent_columns(line);
            if indent > base_indent {
                // After a blank line, lazy continuation no longer applies: a line
                // must be indented to the item's content column to keep belonging
                // to it. A shallower line ends the list (corpus 81-list-lazy-5).
                if pending_blank && indent < base_indent + 2 {
                    break;
                }
                if let Some(last) = items.last_mut() {
                    let nested = collect_indented_block(cur, base_indent, content_col);
                    let nested_children = parse_blocks_with_options(&nested, options);
                    // A blank before an indented sub-block loosens only when it
                    // is a genuine second paragraph (#74 compact list blocks).
                    if pending_blank
                        && matches!(nested_children.first(), Some(BlockNode::Paragraph(_)))
                    {
                        tight = false;
                    }
                    pending_blank = false;
                    last.children.extend(nested_children);
                    continue;
                }
            }
            break;
        };
        if marker.indent < base_indent {
            break;
        }
        if marker.indent > base_indent {
            // A marker indented past the base nests as a sub-list. (An ordered
            // marker BELOW the content column never reaches here -- it folds
            // into the item paragraph in the per-item loop below, §10. Unordered
            // and task markers always interrupt, so they nest at any indent.)
            if pending_blank && marker.indent < base_indent + 2 {
                break;
            }
            if let Some(last) = items.last_mut() {
                let nested = collect_indented_block(cur, base_indent, content_col);
                let nested_children = parse_blocks_with_options(&nested, options);
                last.children.extend(nested_children);
                continue;
            }
            break;
        }
        if marker.ordered != is_ordered || marker.checked.is_some() != is_task {
            break;
        }
        if pending_blank {
            tight = false;
            pending_blank = false;
        }
        // This item's content column. For ordered/unordered it is where the
        // marker content begins (`- `=2, `1. `=3, `10. `=4). For a TASK the
        // checkbox is content, not marker, so the column is the bullet width
        // (`- `/`* ` = 2) -- a child indented to 2 nests, matching the spec's
        // task attribute/continuation convention (`- [x] x` / `  {.c}`).
        content_col = if marker.checked.is_some() {
            base_indent + 2
        } else {
            let l = cur.peek().unwrap();
            let byte_off = (marker.content.as_ptr() as usize).saturating_sub(l.as_ptr() as usize);
            indent_columns(l) + byte_off.saturating_sub(leading_ws(l))
        };
        cur.consume();
        // First-block form `- +` (grammar §17): a lone `+` as the marker
        // content means the item's first block is the following flush-left
        // block (no inline paragraph).
        if marker.content.trim() == "+" {
            let mut item = ListItem {
                attrs: None,
                checked: marker.checked,
                children: Vec::new(),
            };
            if let Some(block) = parse_block(cur, options) {
                item.children.push(block);
            }
            items.push(item);
            continue;
        }
        // The item's first paragraph is the marker content plus any
        // immediately-following indented prose lines (lazy continuation).
        // It stops at a blank line or a list marker: a nested sublist still
        // interrupts (the one Carve deviation, grammar §10), while every other
        // block opener -- heading, fence, etc. -- stays paragraph text.
        let mut para_lines = vec![marker.content.to_string()];
        while let Some(next) = cur.peek() {
            if next.trim().is_empty() || next.trim() == "+" {
                break;
            }
            if let Some(nm) = detect_list_marker_full(next) {
                // An ordered marker indented past the base but below this item's
                // content column is lazy continuation, not a sub-list: ordered
                // markers do not interrupt a paragraph (§10). Fold it. Any other
                // marker (or one at/above the content column) ends the paragraph.
                let folds = nm.ordered
                    && nm.checked.is_none()
                    && nm.indent > base_indent
                    && nm.indent < content_col;
                if !folds {
                    break;
                }
            }
            let indent = indent_columns(next);
            if indent <= base_indent {
                // Lazy continuation: a non-indented line that does not start a
                // block folds into the item's open paragraph (djot/CommonMark).
                if interrupts_paragraph(next, &cur.lines[cur.pos + 1..]) {
                    break;
                }
                para_lines.push(next.trim_start().to_string());
                cur.consume();
                continue;
            }
            // An indented block opener (block quote, heading, fence, div, table)
            // at the item's content column interrupts the lead paragraph and nests
            // as a child block rather than folding in as lazy text. The interrupt
            // test keys off column 0, so check the dedented line; true lazy
            // continuation text does not interrupt and stays in the paragraph.
            let dedented = slice_columns(next, content_col.min(indent), false);
            if interrupts_paragraph(&dedented, &cur.lines[cur.pos + 1..]) {
                break;
            }
            para_lines.push(dedented);
            cur.consume();
        }
        let para_text = para_lines.join("\n");
        items.push(ListItem {
            attrs: None,
            checked: marker.checked,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_inline_with_options(&para_text, options),
            })],
        });
    }
    BlockNode::List(List {
        attrs: None,
        ordered: is_ordered,
        start,
        ol_type,
        tight,
        items,
    })
}

#[derive(Clone, Copy)]
struct ListMarker<'a> {
    indent: usize,
    ordered: bool,
    checked: Option<bool>,
    start: Option<usize>,
    ol_type: Option<OrderedListType>,
    content: &'a str,
}

fn detect_list_marker_full(line: &str) -> Option<ListMarker<'_>> {
    let indent = indent_columns(line);
    if let Some((checked, content)) = detect_task(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: Some(checked),
            start: None,
            ol_type: None,
            content,
        });
    }
    if let Some((content, start, ol_type)) = detect_ordered_full(line) {
        return Some(ListMarker {
            indent,
            ordered: true,
            checked: None,
            start,
            ol_type,
            content,
        });
    }
    if let Some(content) = detect_unordered(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: None,
            start: None,
            ol_type: None,
            content,
        });
    }
    None
}

fn collect_indented_block(cur: &mut LineCursor, parent_indent: usize, strip_cols: usize) -> String {
    let mut lines = Vec::new();
    let mut block_indent: Option<usize> = None;
    while let Some(line) = cur.peek() {
        if line.trim().is_empty() {
            // Lazy continuation does not cross a blank line: after a blank, only
            // keep collecting if the next non-blank line is still indented to the
            // block's own level. A shallower line (e.g. a dedent landing below a
            // sublist) ends the block and is left for the caller, so it can close
            // the list rather than fold in (grammar §10, corpus 81-list-lazy-5).
            if let Some(bi) = block_indent {
                let mut k = cur.pos + 1;
                while k < cur.lines.len() && cur.lines[k].trim().is_empty() {
                    k += 1;
                }
                let continues = k < cur.lines.len() && indent_columns(cur.lines[k]) >= bi;
                if !continues {
                    break;
                }
            }
            lines.push(String::new());
            cur.consume();
            continue;
        }
        let indent = indent_columns(line);
        if indent <= parent_indent {
            break;
        }
        if block_indent.is_none() {
            block_indent = Some(indent);
        }
        // Dedent by the item's content column so a nested block (sub-list, block
        // quote, heading) reaches column 0 and parses. A sub-list marker line is
        // dedented residual-aware so tab+space-aligned siblings keep the same
        // visual column (the recursive parse re-derives the child base); other
        // lines use whole-tab dedent so they land flush at column 0.
        let is_marker = detect_list_marker_full(line).is_some();
        lines.push(slice_columns(line, strip_cols.min(indent), is_marker));
        cur.consume();
    }
    lines.join("\n")
}

fn detect_block_image(line: &str) -> Option<Image> {
    if !line.starts_with("![") {
        return None;
    }
    let (img, consumed) = parse_image_at(line.as_bytes(), 0)?;
    let after = &line[consumed..];
    if !after.trim().is_empty() {
        return None;
    }
    Some(img)
}

fn parse_paragraph(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let mut lines: Vec<&str> = Vec::new();
    while let Some(line) = cur.peek() {
        if line.trim().is_empty() {
            break;
        }
        // First line is always part of the paragraph; from the second on, a
        // visible block opener interrupts (§10).
        if !lines.is_empty() && interrupts_paragraph(line, &cur.lines[cur.pos + 1..]) {
            break;
        }
        cur.consume();
        // Leading indentation is not significant in a paragraph (djot has no
        // indented code blocks); strip it so an indented line like ` c` renders
        // as `<p>c</p>`, matching list-item continuation handling.
        lines.push(line.trim_start());
    }
    let joined = lines.join("\n");
    let (text, attrs) = split_trailing_attrs(&joined);
    BlockNode::Paragraph(Paragraph {
        attrs,
        children: parse_inline_with_options(text, options),
    })
}

/// Whether `line`, seen while accumulating a paragraph, ends it and starts a
/// new block (grammar §10, post-Markdown default).
///
/// A VISIBLE block interrupts an open paragraph with no blank line, at the top
/// level and nested: heading, thematic break, block quote, bullet/task list, a
/// valid table row, and a fenced code / `:::` block that has a matching closer
/// ahead (`rest` is the lines after the current one). INVISIBLE constructs
/// (comments, abbreviation definitions) interrupt too. ORDERED lists do NOT
/// interrupt, `+` is the continuation marker not a bullet, and a bare image
/// stays inline.
fn interrupts_paragraph(line: &str, rest: &[&str]) -> bool {
    // §10 (post-Markdown default): a VISIBLE block interrupts an open paragraph
    // with no blank line. Invisible constructs (comments, abbreviation defs)
    // interrupt too. Ordered lists do NOT interrupt, `+` is the continuation
    // marker not a bullet, and a bare image stays inline.
    if line.trim_start().starts_with("%%") || detect_abbreviation_def(line).is_some() {
        return true;
    }
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || line.starts_with('>')
        || detect_task(line).is_some()
        || is_interrupting_bullet(line)
        || is_table_start(line)
    {
        return true;
    }
    // Fenced code / `:::` interrupt only with a matching closer ahead.
    if let Some(open) = detect_fence_open(line) {
        if rest.iter().any(|l| is_fence_close(l, open)) {
            return true;
        }
    }
    if let Some(open) = detect_container_open(line) {
        let len = open.fence_len;
        if rest.iter().any(|l| {
            let t = l.trim();
            !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= len
        }) {
            return true;
        }
    }
    false
}

/// A `- ` or `* ` bullet (NOT `+`, the continuation marker; not ordered).
///
/// Leading tabs are skipped as well as spaces: a bullet opens a list at any
/// indentation (Rule B), so a tab-indented bullet interrupts a paragraph too.
fn is_interrupting_bullet(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i < bytes.len()
        && (bytes[i] == b'-' || bytes[i] == b'*')
        && i + 1 < bytes.len()
        && bytes[i + 1] == b' '
}

fn is_definition_list_start(line: &str) -> bool {
    line.starts_with(":: ") || line.starts_with(":  ")
}

fn parse_definition_list(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let mut terms = Vec::new();
    let mut defs = Vec::new();
    while let Some(line) = cur.peek() {
        if let Some(term) = line.strip_prefix(":: ") {
            terms.push(parse_inline_with_options(term.trim_end(), options));
            cur.consume();
        } else if let Some(def) = line.strip_prefix(":  ") {
            defs.push(vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_inline_with_options(def.trim_end(), options),
            })]);
            cur.consume();
        } else {
            break;
        }
    }
    BlockNode::DefinitionList(DefinitionList {
        attrs: None,
        items: vec![DefinitionItem {
            terms,
            definitions: defs,
        }],
    })
}

fn consume_caption(cur: &mut LineCursor, options: &Options<'_>) -> Option<Vec<InlineNode>> {
    let saved = cur.pos;
    while matches!(cur.peek(), Some(line) if line.trim().is_empty()) {
        cur.consume();
    }
    let Some(line) = cur.peek() else {
        cur.pos = saved;
        return None;
    };
    let Some(text) = line.strip_prefix("^ ") else {
        cur.pos = saved;
        return None;
    };
    cur.consume();
    Some(parse_inline_with_options(text.trim_end(), options))
}

fn is_table_start(line: &str) -> bool {
    // A table STARTS on a `|` row. `+` continuation cells are consumed inside
    // parse_table from that first row; a `+` line never starts a table (#80).
    line.trim_start().starts_with("|=") || line.trim_start().starts_with('|')
}

fn parse_table(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let mut rows = Vec::new();
    while let Some(line) = cur.peek() {
        // Continue on a `|` row or a `+` multi-line-cell continuation.
        if !is_table_start(line) && !is_table_continuation(line) {
            break;
        }
        cur.consume();
        if is_table_continuation(line) {
            if let Some(last) = rows.last_mut() {
                apply_table_continuation(last, line, options);
            }
        } else {
            rows.push(parse_table_row(line, options));
        }
    }
    let table = Table {
        attrs: None,
        caption: consume_caption(cur, options),
        rows,
    };
    if table.caption.is_some() {
        return BlockNode::Table(table);
    }
    BlockNode::Table(table)
}

fn is_table_continuation(line: &str) -> bool {
    line.trim_start().starts_with('+')
}

fn apply_table_continuation(row: &mut TableRow, line: &str, options: &Options<'_>) {
    let mut content = line.trim();
    if let Some(stripped) = content.strip_prefix('+') {
        content = stripped;
    }
    if let Some(stripped) = content.strip_suffix('|') {
        content = stripped;
    }
    for (idx, cell) in split_table_cells(content).into_iter().enumerate() {
        let text = cell.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(target) = row.cells.get_mut(idx) {
            if !target.children.is_empty() {
                target.children.push(InlineNode::Text(" ".to_string()));
            }
            target
                .children
                .extend(parse_inline_with_options(text, options));
        }
    }
}

fn parse_table_row(line: &str, options: &Options<'_>) -> TableRow {
    let mut content = line.trim();
    if let Some(stripped) = content.strip_prefix('|') {
        content = stripped;
    }
    if let Some(stripped) = content.strip_suffix('|') {
        content = stripped;
    }
    let cells = split_table_cells(content)
        .into_iter()
        .map(|cell| parse_table_cell(&cell, options))
        .collect();
    TableRow { cells }
}

fn split_table_cells(content: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut buf = String::new();
    let mut escaped = false;
    let mut code_ticks = 0usize;
    for ch in content.chars() {
        if escaped {
            buf.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '`' {
            code_ticks ^= 1;
            buf.push(ch);
            continue;
        }
        if ch == '|' && code_ticks == 0 {
            cells.push(std::mem::take(&mut buf));
            continue;
        }
        buf.push(ch);
    }
    cells.push(buf);
    cells
}

fn parse_table_cell(cell: &str, options: &Options<'_>) -> TableCell {
    let trimmed = cell.trim();
    let header = trimmed.starts_with('=');
    let mut text = if header { trimmed[1..].trim() } else { trimmed };
    let align = if text.len() > 1 {
        match text.as_bytes()[0] {
            b'>' if text.as_bytes()[1] == b' ' => {
                text = text[1..].trim();
                Some(TableAlign::Right)
            }
            b'<' if text.as_bytes()[1] == b' ' => {
                text = text[1..].trim();
                Some(TableAlign::Left)
            }
            b'~' if text.as_bytes()[1] == b' ' => {
                text = text[1..].trim();
                Some(TableAlign::Center)
            }
            b'>' | b'<' | b'~' => {
                text = text[1..].trim();
                Some(match trimmed.as_bytes()[if header { 1 } else { 0 }] {
                    b'>' => TableAlign::Right,
                    b'<' => TableAlign::Left,
                    _ => TableAlign::Center,
                })
            }
            _ => None,
        }
    } else {
        None
    };
    let span = match text {
        "^" => Some(TableCellSpan::Rowspan),
        "<" => Some(TableCellSpan::Colspan),
        _ => None,
    };
    TableCell {
        header,
        span,
        align,
        children: if span.is_some() {
            Vec::new()
        } else {
            parse_inline_with_options(text, options)
        },
    }
}

struct ContainerOpen {
    fence_len: usize,
    kind: Option<String>,
    title: Option<String>,
    attrs: Option<Attrs>,
}

fn detect_container_open(line: &str) -> Option<ContainerOpen> {
    let trimmed = line.trim();
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    let rest = trimmed[fence_len..].trim();
    if rest.is_empty() {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            attrs: None,
        });
    }
    if rest.starts_with('{') && rest.ends_with('}') {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            attrs: parse_attrs(&rest[1..rest.len() - 1]),
        });
    }
    let mut parts = rest.splitn(2, char::is_whitespace);
    let kind = parts.next()?.to_string();
    let title = parts
        .next()
        .map(str::trim)
        .filter(|s| s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        .map(|s| s[1..s.len() - 1].to_string());
    Some(ContainerOpen {
        fence_len,
        kind: Some(kind),
        title,
        attrs: None,
    })
}

fn parse_container(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let open = detect_container_open(cur.peek().unwrap()).unwrap();
    cur.consume();
    let mut inner = Vec::new();
    while let Some(line) = cur.peek() {
        if line.trim().bytes().all(|b| b == b':') && line.trim().len() >= open.fence_len {
            cur.consume();
            break;
        }
        inner.push(line.to_string());
        cur.consume();
    }
    let children = parse_blocks_with_options(&inner.join("\n"), options);
    if let Some(kind) = open.kind {
        BlockNode::Admonition(Admonition {
            attrs: open.attrs,
            kind,
            title: open.title.map(|t| parse_inline_with_options(&t, options)),
            children,
        })
    } else {
        BlockNode::Div(Div {
            attrs: open.attrs,
            children,
        })
    }
}

fn detect_abbreviation_def(line: &str) -> Option<AbbreviationDef> {
    let rest = line.strip_prefix("*[")?;
    let (abbr, expansion) = rest.split_once("]:")?;
    Some(AbbreviationDef {
        abbr: abbr.to_string(),
        expansion: expansion.trim().to_string(),
    })
}

fn split_trailing_attrs(text: &str) -> (&str, Option<Attrs>) {
    let trimmed = text.trim_end();
    if !trimmed.ends_with('}') {
        return (text, None);
    }
    let Some(open) = find_attr_open(trimmed) else {
        return (text, None);
    };
    // The attribute block must be separated from the preceding text by
    // whitespace (a space/tab, or a newline when it trails a multi-line
    // heading/paragraph) — so `foo{#id}` stays literal but `foo {#id}` and a
    // final `{#id}` line do not.
    if open == 0 || !trimmed[..open].ends_with([' ', '\t', '\n']) {
        return (text, None);
    }
    let attrs = parse_attrs(&trimmed[open + 1..trimmed.len() - 1]);
    match attrs {
        Some(attrs) => (trimmed[..open].trim_end(), Some(attrs)),
        None => (text, None),
    }
}

fn find_attr_open(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut last = None;
    for (idx, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '{' {
            last = Some(idx);
        }
    }
    last
}

fn read_attrs_at(bytes: &[u8], start: usize) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            if bytes[i] == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            quote = Some(bytes[i]);
            i += 1;
            continue;
        }
        if bytes[i] == b'}' {
            break;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let inner = std::str::from_utf8(&bytes[start + 1..i]).ok()?;
    Some((parse_attrs(inner)?, i + 1))
}

fn parse_attrs(src: &str) -> Option<Attrs> {
    if src.trim().is_empty() {
        return None;
    }
    let mut attrs = Attrs::default();
    for token in attr_tokens(src) {
        if let Some(id) = token.strip_prefix('#') {
            if id.is_empty() {
                return None;
            }
            if attrs.id.is_none() {
                attrs.order.push(AttrSlot::Id);
            }
            attrs.id = Some(id.to_string());
        } else if let Some(class) = token.strip_prefix('.') {
            if class.is_empty() {
                return None;
            }
            if attrs.classes.is_empty() {
                attrs.order.push(AttrSlot::Class);
            }
            attrs.classes.push(class.to_string());
        } else if let Some((key, value)) = token.split_once('=') {
            if key.is_empty() {
                return None;
            }
            if !attrs.key_values.contains_key(key) {
                attrs.order.push(AttrSlot::Key(key.to_string()));
            }
            let value = value
                .trim_matches('"')
                .trim_matches('\'')
                .replace("\\\"", "\"")
                .replace("\\'", "'");
            attrs.key_values.insert(key.to_string(), value);
        } else {
            return None;
        }
    }
    Some(attrs)
}

fn attr_tokens(src: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in src.chars() {
        if escaped {
            buf.push('\\');
            buf.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            buf.push(ch);
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            buf.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !buf.is_empty() {
                tokens.push(std::mem::take(&mut buf));
            }
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    tokens
}

fn parse_standalone_attrs(line: &str) -> Option<Attrs> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }
    parse_attrs(&trimmed[1..trimmed.len() - 1])
}

/// A standalone block-attribute block, possibly spanning several contiguous
/// (non-blank) lines: it opens with `{` and closes with `}` on a later line
/// (`{#id` / ` .foo}`). Consumes the lines and returns the parsed attributes,
/// or leaves the cursor untouched if it is not a valid attribute block.
fn parse_standalone_attrs_block(cur: &mut LineCursor) -> Option<Attrs> {
    let first = cur.peek()?;
    if !first.trim_start().starts_with('{') {
        return None;
    }
    if let Some(attrs) = parse_standalone_attrs(first) {
        cur.consume();
        return Some(attrs);
    }
    // Multi-line: join contiguous lines until one closes with `}`.
    let mut joined = String::new();
    let mut count = 0usize;
    while let Some(line) = cur.lines.get(cur.pos + count).copied() {
        if line.trim().is_empty() {
            return None;
        }
        if !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(line.trim());
        count += 1;
        if line.trim_end().ends_with('}') {
            let inner = joined.trim();
            if inner.starts_with('{') && inner.ends_with('}') {
                if let Some(attrs) = parse_attrs(&inner[1..inner.len() - 1]) {
                    for _ in 0..count {
                        cur.consume();
                    }
                    return Some(attrs);
                }
            }
            return None;
        }
    }
    None
}

fn merge_attrs(target: &mut Option<Attrs>, incoming: Attrs) {
    if target.is_none() {
        *target = Some(incoming);
        return;
    }
    let target = target.as_mut().unwrap();
    if incoming.id.is_some() {
        target.id = incoming.id;
    }
    target.classes.extend(incoming.classes);
    target.key_values.extend(incoming.key_values);
    // Merge the render order too: a later id/key overrides the value but keeps
    // its original slot position, so consecutive attribute lines emit in
    // first-appearance order (`{#a}` / `{k=v}` / `{.c}` -> id, then k, then
    // class). Without this only the last line's slots were rendered.
    for slot in incoming.order {
        if !target.order.contains(&slot) {
            target.order.push(slot);
        }
    }
}

fn apply_attrs_to_block(node: &mut BlockNode, attrs: Attrs) {
    match node {
        BlockNode::Heading(n) => n.attrs = Some(attrs),
        BlockNode::Paragraph(n) => n.attrs = Some(attrs),
        BlockNode::CodeBlock(n) => n.attrs = Some(attrs),
        BlockNode::List(n) => n.attrs = Some(attrs),
        BlockNode::BlockQuote(n) => n.attrs = Some(attrs),
        BlockNode::Table(n) => n.attrs = Some(attrs),
        BlockNode::Admonition(n) => n.attrs = Some(attrs),
        BlockNode::Div(n) => n.attrs = Some(attrs),
        BlockNode::DefinitionList(n) => n.attrs = Some(attrs),
        BlockNode::Figure(n) => n.attrs = Some(attrs),
        BlockNode::Extension(n) => n.attrs = Some(attrs),
        _ => {}
    }
}

fn apply_attrs_to_inline(node: &mut InlineNode, attrs: Attrs) {
    match node {
        InlineNode::Emphasis(n) => n.attrs = Some(attrs),
        InlineNode::Link(n) => n.attrs = Some(attrs),
        InlineNode::Image(n) => n.attrs = Some(attrs),
        InlineNode::Span(n) => n.attrs = Some(attrs),
        InlineNode::Math(n) => n.attrs = Some(attrs),
        InlineNode::AutoLink(n) => n.attrs = Some(attrs),
        InlineNode::Extension(n) => n.attrs = Some(attrs),
        _ => {}
    }
}

fn try_extension_block(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    if options.extensions.is_empty() {
        return None;
    }
    let ctx = MatcherContext::new(options);
    for ext in &options.extensions {
        if let Some(BlockMatch {
            node,
            lines_consumed,
        }) = ext.match_block(cur.lines, cur.pos, &ctx)
        {
            if lines_consumed == 0 || cur.pos + lines_consumed > cur.lines.len() {
                continue;
            }
            cur.pos += lines_consumed;
            return Some(node);
        }
    }
    None
}

// ============================================================================
// Inline parsing
// ============================================================================

pub(crate) fn parse_inline_with_options(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    let bytes = text.as_bytes();
    // A `[` only opens an inline link, reference link, or span when a `](`,
    // `][`, or `]{` follows (there is no bare shortcut-reference form). If none
    // occur, those attempts -- each an O(n) bracket scan -- can be skipped, so a
    // deeply nested run like `[[[[x]]]]` stays O(n) instead of O(n^2). Footnotes
    // (`[^...]`) are handled separately and cheaply gated on `[^`.
    let has_link_trigger = text.contains("](") || text.contains("][") || text.contains("]{");
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];

        // Backslash escapes
        if c == b'\\' && i + 1 < bytes.len() {
            let nxt = bytes[i + 1];
            if is_escapable(nxt) {
                buf.push('\\');
                buf.push(nxt as char);
                i += 2;
                continue;
            }
        }

        // Trailing line comment: `%%` at start of line or after whitespace runs
        // to end of line and is dropped (`text %% comment`).
        if c == b'%'
            && bytes.get(i + 1) == Some(&b'%')
            && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t' || bytes[i - 1] == b'\n')
        {
            while buf.ends_with(' ') || buf.ends_with('\t') {
                buf.pop();
            }
            match bytes[i..].iter().position(|&b| b == b'\n') {
                Some(p) => i += p,
                None => i = bytes.len(),
            }
            continue;
        }

        // Inline code spans
        if c == b'`' {
            if let Some((value, consumed)) = parse_inline_code(bytes, i) {
                if let Some((raw, raw_consumed)) =
                    parse_raw_inline_after_code(bytes, i, &value, consumed)
                {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::RawInline(raw));
                    i += raw_consumed;
                    continue;
                }
                flush_text(&mut out, &mut buf);
                // An inline attribute block right after the code span attaches
                // to it (`` `code`{.cls} `` -> <code class="cls">), matching the
                // general "attributes attach to the preceding inline" rule.
                let (attrs, code_consumed) = match read_attrs_at(bytes, i + consumed) {
                    Some((parsed, next)) => (Some(parsed), next - i),
                    None => (None, consumed),
                };
                out.push(InlineNode::Code(value, attrs));
                i += code_consumed;
                continue;
            }
        }

        if c == b'$' {
            if let Some((math, consumed)) = parse_math(bytes, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Math(math));
                i += consumed;
                continue;
            }
        }

        if c == b'{' {
            if let Some((critic, consumed)) = parse_critic_markup(bytes, i, options) {
                flush_text(&mut out, &mut buf);
                out.push(critic);
                i += consumed;
                continue;
            }
        }

        // Image: ![alt](src)
        if c == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some((img, consumed)) = parse_image_at(bytes, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Image(img));
                i += consumed;
                continue;
            }
        }

        // Inline link: [text](href)
        if c == b'[' {
            if let Some((footnote, consumed)) = parse_footnote_ref(bytes, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Footnote(footnote));
                i += consumed;
                continue;
            }
            if has_link_trigger {
                if let Some((link, consumed)) = parse_inline_link_with_options(bytes, i, options) {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Link(link));
                    i += consumed;
                    continue;
                }
                if let Some((link, consumed)) = parse_reference_link(bytes, i, options) {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Link(link));
                    i += consumed;
                    continue;
                }
                if let Some((span, consumed)) = parse_span(bytes, i, options) {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Span(span));
                    i += consumed;
                    continue;
                }
            }
        }

        if c == b'<' {
            if let Some((crossref, consumed)) = parse_crossref(text, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::CrossRef(crossref));
                i += consumed;
                continue;
            }
        }

        if c == b'@' {
            if let Some((mention, consumed)) = parse_mention(text, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Mention(mention));
                i += consumed;
                continue;
            }
        }

        if c == b'#' {
            if let Some((tag, consumed)) = parse_tag(text, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Tag(tag));
                i += consumed;
                continue;
            }
        }

        if c == b'<' {
            if let Some((autolink, consumed)) = parse_autolink(text, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::AutoLink(autolink));
                i += consumed;
                continue;
            }
        }

        // Inline extension: :name[content]
        if c == b':' {
            if let Some((emoji, consumed)) = parse_emoji(text, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Emoji(emoji));
                i += consumed;
                continue;
            }
            if let Some((node, consumed)) = parse_inline_extension(bytes, i, options) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Extension(node));
                i += consumed;
                continue;
            }
        }

        // Bold-italic, sub, highlight, then single-char emphasis
        if let Some((mut node, consumed)) = match_emphasis(bytes, i, options) {
            let mut consumed = consumed;
            if bytes.get(i + consumed) == Some(&b'{') {
                if let Some((attrs, next)) = read_attrs_at(bytes, i + consumed) {
                    apply_attrs_to_inline(&mut node, attrs);
                    consumed = next - i;
                }
            }
            flush_text(&mut out, &mut buf);
            out.push(node);
            i += consumed;
            continue;
        }

        // Soft break
        if c == b'\n' {
            if buf.ends_with('\\') {
                buf.pop();
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::HardBreak);
                i += 1;
                continue;
            }
            flush_text(&mut out, &mut buf);
            out.push(InlineNode::SoftBreak);
            i += 1;
            continue;
        }

        if let Some(InlineMatch { node, end }) = try_extension_inline(text, i, options) {
            if end > i && end <= text.len() {
                flush_text(&mut out, &mut buf);
                out.push(node);
                i = end;
                continue;
            }
        }

        let ch = text[i..].chars().next().unwrap();
        buf.push(ch);
        i += ch.len_utf8();
    }
    flush_text(&mut out, &mut buf);
    out
}

fn parse_critic_markup(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
) -> Option<(InlineNode, usize)> {
    let rest = std::str::from_utf8(&bytes[start..]).ok()?;
    if let Some(inner) = rest.strip_prefix("{+") {
        let end = inner.find("+}")?;
        return Some((
            InlineNode::CriticInsert(CriticInsert {
                children: parse_inline_with_options(&inner[..end], options),
            }),
            end + 4,
        ));
    }
    if let Some(inner) = rest.strip_prefix("{-") {
        let end = inner.find("-}")?;
        return Some((
            InlineNode::CriticDelete(CriticDelete {
                children: parse_inline_with_options(&inner[..end], options),
            }),
            end + 4,
        ));
    }
    if let Some(inner) = rest.strip_prefix("{~") {
        let sep = inner.find("~>")?;
        let end = inner.find("~}")?;
        return Some((
            InlineNode::CriticSubstitute(CriticSubstitute {
                old_text: inner[..sep].to_string(),
                new_text: inner[sep + 2..end].to_string(),
            }),
            end + 4,
        ));
    }
    if let Some(inner) = rest.strip_prefix("{#") {
        let end = inner.find("#}")?;
        return Some((
            InlineNode::CriticComment(CriticComment {
                text: inner[..end].to_string(),
            }),
            end + 4,
        ));
    }
    None
}

fn parse_footnote_ref(bytes: &[u8], start: usize) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'[') || bytes.get(start + 1) != Some(&b'^') {
        return None;
    }
    let mut i = start + 2;
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let id = std::str::from_utf8(&bytes[start + 2..i]).ok()?.to_string();
    Some((
        Footnote {
            id: Some(id),
            inline: None,
            number: None,
            ref_id: None,
        },
        i + 1 - start,
    ))
}

fn parse_raw_inline_after_code(
    bytes: &[u8],
    start: usize,
    value: &str,
    code_consumed: usize,
) -> Option<(RawInline, usize)> {
    let attr_start = start + code_consumed;
    if bytes.get(attr_start) != Some(&b'{') || bytes.get(attr_start + 1) != Some(&b'=') {
        return None;
    }
    let mut i = attr_start + 2;
    let format_start = i;
    while i < bytes.len() && bytes[i] != b'}' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    Some((
        RawInline {
            format: std::str::from_utf8(&bytes[format_start..i])
                .ok()?
                .to_string(),
            content: value.to_string(),
        },
        i + 1 - start,
    ))
}

fn parse_math(bytes: &[u8], start: usize) -> Option<(Math, usize)> {
    let display = bytes.get(start + 1) == Some(&b'$');
    let tick = if display { start + 2 } else { start + 1 };
    if bytes.get(tick) != Some(&b'`') {
        return None;
    }
    let rest = std::str::from_utf8(&bytes[tick + 1..]).ok()?;
    let close = rest.find('`')?;
    Some((
        Math {
            attrs: None,
            display,
            content: rest[..close].to_string(),
        },
        tick + 1 + close + 1 - start,
    ))
}

fn parse_reference_link(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
) -> Option<(Link, usize)> {
    let (text, after_text) = read_bracketed(bytes, start)?;
    if bytes.get(after_text) != Some(&b'[') {
        return None;
    }
    let (label, after_label) = read_bracketed(bytes, after_text)?;
    let ref_label = if label.is_empty() {
        text.clone()
    } else {
        label
    };
    Some((
        Link {
            attrs: None,
            href: String::new(),
            title: None,
            children: parse_inline_with_options(&text, options),
            ref_label: Some(ref_label),
            raw_ref: Some(
                std::str::from_utf8(&bytes[start..after_label])
                    .ok()?
                    .to_string(),
            ),
        },
        after_label - start,
    ))
}

fn flush_text(out: &mut Vec<InlineNode>, buf: &mut String) {
    if !buf.is_empty() {
        out.push(InlineNode::Text(std::mem::take(buf)));
    }
}

fn is_escapable(b: u8) -> bool {
    matches!(
        b,
        b'\\'
            | b'`'
            | b'*'
            | b'_'
            | b'{'
            | b'}'
            | b'['
            | b']'
            | b'('
            | b')'
            | b'"'
            | b'\''
            | b'#'
            | b'+'
            | b'-'
            | b'.'
            | b'!'
            | b'~'
            | b'^'
            | b'/'
            | b'<'
            | b'>'
            | b'@'
            | b'%'
            | b'|'
            | b'='
            | b','
    )
}

fn parse_inline_code(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    // Count opening backticks
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    if open_len == 0 {
        return None;
    }
    let content_start = i;
    // Find closing run of exactly `open_len` backticks not followed by another backtick
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let close_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let close_len = i - close_start;
        if close_len == open_len {
            let raw = std::str::from_utf8(&bytes[content_start..close_start]).ok()?;
            // Per CommonMark/JS: trim one leading and one trailing space if both ends are spaces.
            let trimmed =
                if raw.starts_with(' ') && raw.ends_with(' ') && raw.trim().len() < raw.len() {
                    &raw[1..raw.len() - 1]
                } else {
                    raw
                };
            return Some((trimmed.to_string(), i - start));
        }
        // Different length closer — keep scanning past it
    }
    // No matching closer: an unclosed verbatim opener is opaque to the end of
    // the text (matches djot / carve-php / carve-js).
    let raw = std::str::from_utf8(&bytes[content_start..]).ok()?;
    let trimmed = if raw.starts_with(' ') && raw.ends_with(' ') && raw.trim().len() < raw.len() {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    Some((trimmed.to_string(), bytes.len() - start))
}

fn parse_image_at(bytes: &[u8], start: usize) -> Option<(Image, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let (alt, after_alt) = read_bracketed(bytes, start + 1)?;
    if bytes.get(after_alt) != Some(&b'(') {
        return None;
    }
    let (src, title, after_paren) = read_link_target(bytes, after_alt + 1)?;
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Image {
            attrs,
            src,
            alt,
            title,
        },
        after - start,
    ))
}

fn parse_inline_link_with_options(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
) -> Option<(Link, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let (text, after_bracket) = read_bracketed(bytes, start)?;
    if bytes.get(after_bracket) != Some(&b'(') {
        return None;
    }
    let (href, title, after_paren) = read_link_target(bytes, after_bracket + 1)?;
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Link {
            attrs,
            href,
            title,
            children: parse_inline_with_options(&text, options),
            ref_label: None,
            raw_ref: None,
        },
        after - start,
    ))
}

fn parse_inline_extension(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
) -> Option<(InlineExtension, usize)> {
    if bytes.get(start) != Some(&b':') {
        return None;
    }
    let mut i = start + 1;
    let name_start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
    {
        i += 1;
    }
    if i == name_start || bytes.get(i) != Some(&b'[') {
        return None;
    }
    let name = std::str::from_utf8(&bytes[name_start..i]).ok()?.to_string();
    let (content, after_bracket) = read_bracketed(bytes, i)?;
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        InlineExtension {
            attrs,
            name,
            children: parse_inline_with_options(&content, options),
        },
        after - start,
    ))
}

fn parse_span(bytes: &[u8], start: usize, options: &Options<'_>) -> Option<(Span, usize)> {
    let (content, after_bracket) = read_bracketed(bytes, start)?;
    if bytes.get(after_bracket) != Some(&b'{') {
        return None;
    }
    let (attrs, after_attrs) = read_attrs_at(bytes, after_bracket)
        .or_else(|| read_empty_attrs_at(bytes, after_bracket))?;
    Some((
        Span {
            attrs: Some(attrs),
            children: parse_inline_with_options(&content, options),
        },
        after_attrs - start,
    ))
}

fn read_empty_attrs_at(bytes: &[u8], start: usize) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i) == Some(&b'}') {
        Some((Attrs::default(), i + 1))
    } else {
        None
    }
}

fn parse_mention(text: &str, pos: usize) -> Option<(Mention, usize)> {
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let rest = text.get(pos + 1..)?;
    let len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        .count();
    if len == 0 {
        return None;
    }
    Some((
        Mention {
            user: rest[..len].to_string(),
        },
        len + 1,
    ))
}

fn parse_tag(text: &str, pos: usize) -> Option<(Tag, usize)> {
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let rest = text.get(pos + 1..)?;
    let mut len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-' || *b == b'.')
        .count();
    while len > 0 && rest.as_bytes()[len - 1] == b'.' {
        len -= 1;
    }
    if len == 0 {
        return None;
    }
    Some((
        Tag {
            name: rest[..len].to_string(),
        },
        len + 1,
    ))
}

fn parse_emoji(text: &str, pos: usize) -> Option<(Emoji, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(pos) != Some(&b':') {
        return None;
    }
    let rest = text.get(pos + 1..)?;
    let len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'+' || *b == b'-')
        .count();
    if len == 0 || bytes.get(pos + 1 + len) != Some(&b':') {
        return None;
    }
    Some((
        Emoji {
            name: rest[..len].to_string(),
        },
        len + 2,
    ))
}

fn parse_autolink(text: &str, pos: usize) -> Option<(AutoLink, usize)> {
    let rest = text.get(pos..)?;
    let close = rest.find('>')?;
    let target = &rest[1..close];
    let mut attrs = None;
    let mut consumed = close + 1;
    let bytes = text.as_bytes();
    if bytes.get(pos + consumed) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, pos + consumed) {
            attrs = Some(parsed_attrs);
            consumed = next - pos;
        }
    }
    if target.starts_with("http://") || target.starts_with("https://") {
        return Some((
            AutoLink {
                attrs,
                href: target.to_string(),
            },
            consumed,
        ));
    }
    if target.contains('@') && !target.contains(' ') {
        return Some((
            AutoLink {
                attrs,
                href: format!("mailto:{target}"),
            },
            consumed,
        ));
    }
    None
}

fn parse_crossref(text: &str, pos: usize) -> Option<(CrossRef, usize)> {
    let rest = text.get(pos..)?;
    let inner = rest.strip_prefix("</#")?;
    let close = inner.find('>')?;
    Some((
        CrossRef {
            target: inner[..close].to_string(),
        },
        close + 4,
    ))
}

/// Read a `[…]` span starting at `start` (which must point to `[`).
/// Returns the inner text and the index just past the closing `]`.
fn read_bracketed(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut i = start + 1;
    let content_start = i;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            b']' => {
                let text = std::str::from_utf8(&bytes[content_start..i])
                    .ok()?
                    .to_string();
                return Some((text, i + 1));
            }
            _ => i += 1,
        }
    }
    None
}

/// Read `href[ "title"])` starting at `start` (just past the opening `(`).
/// Returns (href, optional title, index just past the closing `)`).
fn read_link_target(bytes: &[u8], start: usize) -> Option<(String, Option<String>, usize)> {
    let mut i = start;
    let href_start = i;
    while i < bytes.len()
        && bytes[i] != b' '
        && bytes[i] != b')'
        && bytes[i] != b'\t'
        && bytes[i] != b'\n'
    {
        i += 1;
    }
    let href = std::str::from_utf8(&bytes[href_start..i]).ok()?.to_string();
    // Skip whitespace
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let mut title: Option<String> = None;
    if bytes.get(i) == Some(&b'"') || bytes.get(i) == Some(&b'\'') {
        let quote = bytes[i];
        i += 1;
        let title_start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        title = Some(
            std::str::from_utf8(&bytes[title_start..i])
                .ok()?
                .to_string(),
        );
        i += 1;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
    }
    if bytes.get(i) != Some(&b')') {
        return None;
    }
    Some((href, title, i + 1))
}

fn match_emphasis(bytes: &[u8], i: usize, options: &Options<'_>) -> Option<(InlineNode, usize)> {
    let c = bytes[i];

    // /*bold italic*/
    if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
        if let Some(close) = find_seq(bytes, i + 2, b"*/") {
            let inner = std::str::from_utf8(&bytes[i + 2..close]).ok()?;
            return Some((
                InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind: EmphasisKind::BoldItalic,
                    children: parse_inline_with_options(inner, options),
                }),
                close + 2 - i,
            ));
        }
    }
    // ,,sub,,
    if c == b',' && bytes.get(i + 1) == Some(&b',') {
        if bytes.get(i + 2) == Some(&b',') {
            return None;
        }
        if let Some(close) = find_seq(bytes, i + 2, b",,") {
            if bytes.get(close + 2) == Some(&b',') {
                return None;
            }
            if close > i + 2 {
                let inner_bytes = &bytes[i + 2..close];
                if !inner_bytes.is_empty()
                    && inner_bytes[0] != b' '
                    && inner_bytes[inner_bytes.len() - 1] != b' '
                {
                    let inner = std::str::from_utf8(inner_bytes).ok()?;
                    return Some((
                        InlineNode::Emphasis(Emphasis {
                            attrs: None,
                            kind: EmphasisKind::Sub,
                            children: parse_inline_with_options(inner, options),
                        }),
                        close + 2 - i,
                    ));
                }
            }
        }
    }
    // ==highlight==
    if c == b'=' && bytes.get(i + 1) == Some(&b'=') {
        if bytes.get(i + 2) == Some(&b'=') {
            return None;
        }
        if let Some(close) = find_seq(bytes, i + 2, b"==") {
            if bytes.get(close + 2) == Some(&b'=') {
                return None;
            }
            if close > i + 2 {
                let inner_bytes = &bytes[i + 2..close];
                if !inner_bytes.is_empty()
                    && inner_bytes[0] != b' '
                    && inner_bytes[inner_bytes.len() - 1] != b' '
                {
                    let inner = std::str::from_utf8(inner_bytes).ok()?;
                    return Some((
                        InlineNode::Emphasis(Emphasis {
                            attrs: None,
                            kind: EmphasisKind::Highlight,
                            children: parse_inline_with_options(inner, options),
                        }),
                        close + 2 - i,
                    ));
                }
            }
        }
    }

    let kind = match c {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'~' => EmphasisKind::Strike,
        b'^' => EmphasisKind::Super,
        _ => return None,
    };
    let delim = c;
    // Opener: next char must exist and not be space/newline/delim
    let after = bytes.get(i + 1).copied()?;
    if after == b' ' || after == b'\n' || after == delim {
        return None;
    }
    if i > 0 && bytes[i - 1] == delim {
        return None;
    }
    // For / and _, the previous char must not be alphanumeric (avoid mid-word)
    if (delim == b'/' || delim == b'_') && i > 0 {
        let prev = bytes[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let close = find_emphasis_close(bytes, i + 1, delim)?;
    let inner = std::str::from_utf8(&bytes[i + 1..close]).ok()?;
    Some((
        InlineNode::Emphasis(Emphasis {
            attrs: None,
            kind,
            children: parse_inline_with_options(inner, options),
        }),
        close + 1 - i,
    ))
}

fn try_extension_inline(text: &str, pos: usize, options: &Options<'_>) -> Option<InlineMatch> {
    if options.extensions.is_empty() {
        return None;
    }
    if !text.is_char_boundary(pos) {
        return None;
    }
    let ctx = MatcherContext::new(options);
    for ext in &options.extensions {
        if let Some(matched) = ext.match_inline(text, pos, &ctx) {
            return Some(matched);
        }
    }
    None
}

fn apply_abbreviations(doc: &mut Document) {
    let mut defs = BTreeMap::new();
    for child in &doc.children {
        if let BlockNode::AbbreviationDef(def) = child {
            defs.insert(def.abbr.clone(), def.expansion.clone());
        }
    }
    if defs.is_empty() {
        return;
    }
    doc.children
        .retain(|node| !matches!(node, BlockNode::AbbreviationDef(_)));
    for block in &mut doc.children {
        apply_abbreviations_block(block, &defs);
    }
}

fn apply_abbreviations_block(block: &mut BlockNode, defs: &BTreeMap<String, String>) {
    match block {
        BlockNode::Heading(h) => apply_abbreviations_inline(&mut h.children, defs),
        BlockNode::Paragraph(p) => apply_abbreviations_inline(&mut p.children, defs),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    apply_abbreviations_block(child, defs);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                apply_abbreviations_block(child, defs);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    apply_abbreviations_inline(&mut cell.children, defs);
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                apply_abbreviations_block(child, defs);
            }
        }
        BlockNode::Figure(f) => {
            apply_abbreviations_inline(&mut f.caption, defs);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        apply_abbreviations_block(child, defs);
                    }
                }
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            apply_abbreviations_inline(&mut cell.children, defs);
                        }
                    }
                }
                FigureTarget::Image(_) => {}
            }
        }
        _ => {}
    }
}

fn apply_abbreviations_inline(nodes: &mut Vec<InlineNode>, defs: &BTreeMap<String, String>) {
    let mut out = Vec::new();
    for node in std::mem::take(nodes) {
        match node {
            InlineNode::Text(text) => {
                let mut parts = replace_abbreviations_in_text(&text, defs);
                out.append(&mut parts);
            }
            InlineNode::Emphasis(mut e) => {
                apply_abbreviations_inline(&mut e.children, defs);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Link(mut l) => {
                apply_abbreviations_inline(&mut l.children, defs);
                out.push(InlineNode::Link(l));
            }
            InlineNode::Extension(mut e) => {
                apply_abbreviations_inline(&mut e.children, defs);
                out.push(InlineNode::Extension(e));
            }
            other => out.push(other),
        }
    }
    *nodes = out;
}

fn replace_abbreviations_in_text(text: &str, defs: &BTreeMap<String, String>) -> Vec<InlineNode> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let mut matched: Option<(&str, &str)> = None;
        for (abbr, expansion) in defs {
            if text[i..].starts_with(abbr)
                && is_word_boundary(text, i)
                && is_word_boundary(text, i + abbr.len())
            {
                matched = Some((abbr.as_str(), expansion.as_str()));
                break;
            }
        }
        if let Some((abbr, expansion)) = matched {
            out.push(InlineNode::Abbreviation(Abbreviation {
                abbr: abbr.to_string(),
                expansion: expansion.to_string(),
            }));
            i += abbr.len();
            continue;
        }
        let ch = text[i..].chars().next().unwrap();
        match out.last_mut() {
            Some(InlineNode::Text(existing)) => existing.push(ch),
            _ => out.push(InlineNode::Text(ch.to_string())),
        }
        i += ch.len_utf8();
    }
    out
}

fn is_word_boundary(text: &str, pos: usize) -> bool {
    if pos == 0 || pos >= text.len() {
        return true;
    }
    !text.as_bytes()[pos - 1].is_ascii_alphanumeric()
        || !text.as_bytes()[pos].is_ascii_alphanumeric()
}

fn resolve_crossrefs(doc: &mut Document) {
    let mut counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    collect_heading_titles(&doc.children, &mut counts, &mut titles);
    for block in &mut doc.children {
        resolve_crossrefs_block(block, &titles);
    }
}

fn resolve_reference_links(doc: &mut Document, defs: &BTreeMap<String, LinkDef>) {
    for block in &mut doc.children {
        resolve_reference_links_block(block, defs);
    }
}

fn resolve_reference_links_block(block: &mut BlockNode, defs: &BTreeMap<String, LinkDef>) {
    match block {
        BlockNode::Heading(h) => resolve_reference_links_inline(&mut h.children, defs),
        BlockNode::Paragraph(p) => resolve_reference_links_inline(&mut p.children, defs),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    resolve_reference_links_block(child, defs);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                resolve_reference_links_block(child, defs);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    resolve_reference_links_inline(&mut cell.children, defs);
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                resolve_reference_links_block(child, defs);
            }
        }
        _ => {}
    }
}

fn resolve_reference_links_inline(nodes: &mut Vec<InlineNode>, defs: &BTreeMap<String, LinkDef>) {
    let mut out = Vec::new();
    for mut node in std::mem::take(nodes) {
        match &mut node {
            InlineNode::Link(l) => {
                if let Some(label) = &l.ref_label {
                    if let Some(def) = defs.get(label) {
                        l.href = def.href.clone();
                        l.title = def.title.clone();
                        l.ref_label = None;
                        l.raw_ref = None;
                        out.push(node);
                    } else {
                        out.push(InlineNode::Text(l.raw_ref.clone().unwrap_or_default()));
                    }
                } else {
                    resolve_reference_links_inline(&mut l.children, defs);
                    out.push(node);
                }
            }
            InlineNode::Emphasis(e) => {
                resolve_reference_links_inline(&mut e.children, defs);
                out.push(node);
            }
            InlineNode::Span(s) => {
                resolve_reference_links_inline(&mut s.children, defs);
                out.push(node);
            }
            InlineNode::Extension(e) => {
                resolve_reference_links_inline(&mut e.children, defs);
                out.push(node);
            }
            _ => out.push(node),
        }
    }
    *nodes = out;
}

fn collect_heading_titles(
    blocks: &[BlockNode],
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                let title = plain_inlines_parse(&h.children);
                let base = h
                    .attrs
                    .as_ref()
                    .and_then(|attrs| attrs.id.clone())
                    .unwrap_or_else(|| slugify_parse(&title));
                let count = counts.entry(base.clone()).or_insert(0);
                *count += 1;
                let id = if *count == 1 {
                    base
                } else {
                    format!("{base}-{count}")
                };
                titles.insert(id, title);
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_heading_titles(&item.children, counts, titles);
                }
            }
            BlockNode::BlockQuote(b) => collect_heading_titles(&b.children, counts, titles),
            BlockNode::Admonition(a) => collect_heading_titles(&a.children, counts, titles),
            _ => {}
        }
    }
}

fn resolve_crossrefs_block(block: &mut BlockNode, titles: &BTreeMap<String, String>) {
    match block {
        BlockNode::Heading(h) => resolve_crossrefs_inline(&mut h.children, titles),
        BlockNode::Paragraph(p) => resolve_crossrefs_inline(&mut p.children, titles),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    resolve_crossrefs_block(child, titles);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                resolve_crossrefs_block(child, titles);
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                resolve_crossrefs_block(child, titles);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    resolve_crossrefs_inline(&mut cell.children, titles);
                }
            }
        }
        _ => {}
    }
}

fn resolve_crossrefs_inline(nodes: &mut Vec<InlineNode>, titles: &BTreeMap<String, String>) {
    for node in nodes {
        match node {
            InlineNode::CrossRef(c) => {
                if let Some(title) = titles.get(&c.target) {
                    *node = InlineNode::Link(Link {
                        attrs: None,
                        href: format!("#{}", c.target),
                        title: None,
                        children: vec![InlineNode::Text(title.clone())],
                        ref_label: None,
                        raw_ref: None,
                    });
                } else {
                    // Unknown heading id: the cross-reference stays literal text.
                    *node = InlineNode::Text(format!("</#{}>", c.target));
                }
            }
            InlineNode::Emphasis(e) => resolve_crossrefs_inline(&mut e.children, titles),
            InlineNode::Link(l) => resolve_crossrefs_inline(&mut l.children, titles),
            InlineNode::Span(s) => resolve_crossrefs_inline(&mut s.children, titles),
            InlineNode::Extension(e) => resolve_crossrefs_inline(&mut e.children, titles),
            _ => {}
        }
    }
}

fn plain_inlines_parse(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(s),
            InlineNode::Emphasis(e) => out.push_str(&plain_inlines_parse(&e.children)),
            InlineNode::Code(s, _) => out.push_str(s),
            InlineNode::Link(l) => out.push_str(&plain_inlines_parse(&l.children)),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => out.push_str(&plain_inlines_parse(&e.children)),
            InlineNode::Abbreviation(a) => out.push_str(&a.abbr),
            InlineNode::Mention(m) => out.push_str(&m.user),
            InlineNode::Tag(t) => out.push_str(&t.name),
            // A soft/hard break (multi-line heading) is a word separator, so
            // parse-time cross-reference slugs match the rendered heading id.
            InlineNode::SoftBreak | InlineNode::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

fn slugify_parse(text: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out = format!("s-{out}");
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

fn find_seq(bytes: &[u8], from: usize, marker: &[u8]) -> Option<usize> {
    if marker.is_empty() || from + marker.len() > bytes.len() {
        return None;
    }
    let mut j = from;
    while j + marker.len() <= bytes.len() {
        if &bytes[j..j + marker.len()] == marker {
            return Some(j);
        }
        j += 1;
    }
    None
}

fn find_emphasis_close(bytes: &[u8], from: usize, delim: u8) -> Option<usize> {
    let mut j = from;
    while j < bytes.len() {
        let ch = bytes[j];
        if ch == b'\\' && j + 1 < bytes.len() {
            j += 2;
            continue;
        }
        if ch == b'`' {
            // Skip a verbatim span: opener run of N backticks closes on a run
            // of exactly N. An unclosed run is opaque to end of text, so no
            // emphasis closer can follow it.
            let open_start = j;
            while j < bytes.len() && bytes[j] == b'`' {
                j += 1;
            }
            let open_len = j - open_start;
            let mut found = false;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let close_start = j;
                    while j < bytes.len() && bytes[j] == b'`' {
                        j += 1;
                    }
                    if j - close_start == open_len {
                        found = true;
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            if !found {
                return None;
            }
            continue;
        }
        if ch == delim {
            let prev = bytes.get(j.wrapping_sub(1)).copied().unwrap_or(b' ');
            if prev == b' ' || prev == b'\n' {
                j += 1;
                continue;
            }
            if delim == b'/' || delim == b'_' {
                if let Some(&next) = bytes.get(j + 1) {
                    if next.is_ascii_alphanumeric() {
                        j += 1;
                        continue;
                    }
                }
            }
            return Some(j);
        }
        j += 1;
    }
    None
}
