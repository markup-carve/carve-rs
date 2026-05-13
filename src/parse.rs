//! Carve parser (MVP subset).
//!
//! Block-level reads line by line; inline does a single linear scan
//! over each block's text. No backtracking.

use crate::ast::{
    BlockNode, BlockQuote, CodeBlock, Document, Emphasis, EmphasisKind, Heading, Image, InlineNode,
    Link, List, ListItem, Paragraph,
};

pub fn parse(source: &str) -> Document {
    let mut lines: Vec<&str> = source.lines().collect();
    // `lines()` already drops a single trailing newline; nothing more to do.
    let _ = &mut lines;

    let mut cursor = LineCursor {
        lines: &lines,
        pos: 0,
    };
    let children = parse_blocks(&mut cursor);
    Document { children }
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

fn parse_blocks(cur: &mut LineCursor) -> Vec<BlockNode> {
    let mut out = Vec::new();
    while !cur.eof() {
        let line = cur.peek().unwrap();
        if line.trim().is_empty() {
            cur.consume();
            continue;
        }
        if let Some(node) = parse_block(cur) {
            out.push(node);
        }
    }
    out
}

fn parse_block(cur: &mut LineCursor) -> Option<BlockNode> {
    let line = cur.peek()?;
    if let Some(fence_marker) = detect_fence_open(line) {
        return Some(parse_fence(cur, fence_marker));
    }
    if detect_thematic_break(line) {
        cur.consume();
        return Some(BlockNode::ThematicBreak);
    }
    if let Some((level, text)) = detect_heading(line) {
        cur.consume();
        return Some(BlockNode::Heading(Heading {
            level,
            children: parse_inline(text),
        }));
    }
    if line.starts_with('>') {
        return Some(parse_blockquote(cur));
    }
    if is_list_marker(line) {
        return Some(parse_list(cur));
    }
    if let Some(img) = detect_block_image(line) {
        cur.consume();
        return Some(BlockNode::BlockImage(img));
    }
    Some(parse_paragraph(cur))
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
    let rest = line[hashes + 1..].trim_end();
    if rest.is_empty() {
        return None;
    }
    Some((hashes as u8, rest))
}

fn detect_thematic_break(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    trimmed.bytes().all(|b| b == b'-')
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
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
    {
        i += 1;
    }
    let lang_end = i;
    // Must be only whitespace after the language token
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
    let lang = if open.lang_start < open.lang_end {
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
    BlockNode::CodeBlock(CodeBlock {
        lang,
        content: content_lines.join("\n"),
    })
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

fn parse_blockquote(cur: &mut LineCursor) -> BlockNode {
    let mut inner = Vec::new();
    while let Some(line) = cur.peek() {
        if !line.starts_with('>') {
            break;
        }
        cur.consume();
        let stripped = &line[1..];
        let stripped = stripped.strip_prefix(' ').unwrap_or(stripped);
        inner.push(stripped.to_string());
    }
    let joined = inner.join("\n");
    let sub_lines: Vec<&str> = joined.lines().collect();
    let mut sub_cursor = LineCursor {
        lines: &sub_lines,
        pos: 0,
    };
    let children = parse_blocks(&mut sub_cursor);
    BlockNode::BlockQuote(BlockQuote { children })
}

fn is_list_marker(line: &str) -> bool {
    detect_task(line).is_some()
        || detect_unordered(line).is_some()
        || detect_ordered(line).is_some()
}

fn detect_unordered(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    if c != b'-' && c != b'*' && c != b'+' {
        return None;
    }
    if i + 1 >= bytes.len() || bytes[i + 1] != b' ' {
        return None;
    }
    Some(line[i + 2..].trim_end())
}

fn detect_ordered(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    if i + 1 >= bytes.len() || bytes[i] != b'.' || bytes[i + 1] != b' ' {
        return None;
    }
    Some(line[i + 2..].trim_end())
}

fn detect_task(line: &str) -> Option<(bool, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    if c != b'-' && c != b'*' && c != b'+' {
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
    let checked = match marker {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };
    Some((checked, line[i + 4..].trim_end()))
}

fn parse_list(cur: &mut LineCursor) -> BlockNode {
    let first = cur.peek().unwrap();
    let is_task = detect_task(first).is_some();
    let is_ordered = !is_task && detect_ordered(first).is_some();
    let mut items = Vec::new();
    while let Some(line) = cur.peek() {
        let (checked, content) = if is_task {
            match detect_task(line) {
                Some((c, t)) => (Some(c), t),
                None => break,
            }
        } else if is_ordered {
            match detect_ordered(line) {
                Some(t) => (None, t),
                None => break,
            }
        } else {
            if detect_task(line).is_some() || detect_ordered(line).is_some() {
                break;
            }
            match detect_unordered(line) {
                Some(t) => (None, t),
                None => break,
            }
        };
        cur.consume();
        items.push(ListItem {
            checked,
            children: vec![BlockNode::Paragraph(Paragraph {
                children: parse_inline(content),
            })],
        });
    }
    BlockNode::List(List {
        ordered: is_ordered,
        items,
    })
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

fn parse_paragraph(cur: &mut LineCursor) -> BlockNode {
    let mut lines = Vec::new();
    while let Some(line) = cur.peek() {
        if line.trim().is_empty() {
            break;
        }
        if is_block_start(line) {
            break;
        }
        cur.consume();
        lines.push(line);
    }
    BlockNode::Paragraph(Paragraph {
        children: parse_inline(&lines.join("\n")),
    })
}

fn is_block_start(line: &str) -> bool {
    detect_heading(line).is_some()
        || detect_fence_open(line).is_some()
        || detect_thematic_break(line)
        || line.starts_with('>')
        || is_list_marker(line)
        || detect_block_image(line).is_some()
}

// ============================================================================
// Inline parsing
// ============================================================================

pub fn parse_inline(text: &str) -> Vec<InlineNode> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];

        // Backslash escapes
        if c == b'\\' && i + 1 < bytes.len() {
            let nxt = bytes[i + 1];
            if is_escapable(nxt) {
                buf.push(nxt as char);
                i += 2;
                continue;
            }
        }

        // Inline code spans
        if c == b'`' {
            if let Some((value, consumed)) = parse_inline_code(bytes, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Code(value));
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
            if let Some((link, consumed)) = parse_inline_link(bytes, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Link(link));
                i += consumed;
                continue;
            }
        }

        // Bold-italic, sub, highlight, then single-char emphasis
        if let Some((node, consumed)) = match_emphasis(bytes, i) {
            flush_text(&mut out, &mut buf);
            out.push(node);
            i += consumed;
            continue;
        }

        // Soft break
        if c == b'\n' {
            flush_text(&mut out, &mut buf);
            out.push(InlineNode::SoftBreak);
            i += 1;
            continue;
        }

        buf.push(c as char);
        i += 1;
    }
    flush_text(&mut out, &mut buf);
    out
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
    None
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
    Some((Image { src, alt, title }, after_paren - start))
}

fn parse_inline_link(bytes: &[u8], start: usize) -> Option<(Link, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let (text, after_bracket) = read_bracketed(bytes, start)?;
    if bytes.get(after_bracket) != Some(&b'(') {
        return None;
    }
    let (href, title, after_paren) = read_link_target(bytes, after_bracket + 1)?;
    Some((
        Link {
            href,
            title,
            children: parse_inline(&text),
        },
        after_paren - start,
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
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
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
    if bytes.get(i) == Some(&b'"') {
        i += 1;
        let title_start = i;
        while i < bytes.len() && bytes[i] != b'"' {
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

fn match_emphasis(bytes: &[u8], i: usize) -> Option<(InlineNode, usize)> {
    let c = bytes[i];

    // /*bold italic*/
    if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
        if let Some(close) = find_seq(bytes, i + 2, b"*/") {
            let inner = std::str::from_utf8(&bytes[i + 2..close]).ok()?;
            return Some((
                InlineNode::Emphasis(Emphasis {
                    kind: EmphasisKind::BoldItalic,
                    children: parse_inline(inner),
                }),
                close + 2 - i,
            ));
        }
    }
    // ,,sub,,
    if c == b',' && bytes.get(i + 1) == Some(&b',') {
        if let Some(close) = find_seq(bytes, i + 2, b",,") {
            if close > i + 2 {
                let inner_bytes = &bytes[i + 2..close];
                if !inner_bytes.is_empty()
                    && inner_bytes[0] != b' '
                    && inner_bytes[inner_bytes.len() - 1] != b' '
                {
                    let inner = std::str::from_utf8(inner_bytes).ok()?;
                    return Some((
                        InlineNode::Emphasis(Emphasis {
                            kind: EmphasisKind::Sub,
                            children: parse_inline(inner),
                        }),
                        close + 2 - i,
                    ));
                }
            }
        }
    }
    // ==highlight==
    if c == b'=' && bytes.get(i + 1) == Some(&b'=') {
        if let Some(close) = find_seq(bytes, i + 2, b"==") {
            if close > i + 2 {
                let inner_bytes = &bytes[i + 2..close];
                if !inner_bytes.is_empty()
                    && inner_bytes[0] != b' '
                    && inner_bytes[inner_bytes.len() - 1] != b' '
                {
                    let inner = std::str::from_utf8(inner_bytes).ok()?;
                    return Some((
                        InlineNode::Emphasis(Emphasis {
                            kind: EmphasisKind::Highlight,
                            children: parse_inline(inner),
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
            kind,
            children: parse_inline(inner),
        }),
        close + 1 - i,
    ))
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
            // Skip past code span
            if let Some(close) = bytes[j + 1..].iter().position(|&b| b == b'`') {
                j = j + 1 + close + 1;
                continue;
            }
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
