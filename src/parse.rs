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
    let children = parse_blocks_with_options(body, options);
    let mut doc = Document {
        frontmatter,
        children,
    };
    apply_abbreviations(&mut doc);
    for ext in &options.extensions {
        doc = ext.after_parse(doc);
    }
    doc
}

fn split_frontmatter(source: &str) -> (BTreeMap<String, String>, &str) {
    if !source.starts_with("---\n") {
        return (BTreeMap::new(), source);
    }
    let rest = &source[4..];
    let Some(close) = rest.find("\n---\n") else {
        return (BTreeMap::new(), source);
    };
    let frontmatter_src = &rest[..close];
    let body = &rest[close + 5..];
    let mut frontmatter = BTreeMap::new();
    for line in frontmatter_src.lines() {
        if let Some((key, value)) = line.split_once(':') {
            frontmatter.insert(key.trim().to_string(), value.trim().to_string());
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
    while !cur.eof() {
        let line = cur.peek().unwrap();
        if line.trim().is_empty() {
            cur.consume();
            continue;
        }
        if let Some(node) = parse_block(cur, options) {
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
    if let Some((level, text)) = detect_heading(line) {
        cur.consume();
        let (text, attrs) = split_trailing_attrs(text);
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
    if let Some(kind) = detect_admonition(line) {
        return Some(parse_admonition(cur, kind, options));
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
        attrs: None,
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

fn parse_blockquote(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
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

fn parse_list(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let first = cur.peek().unwrap();
    let first_marker = detect_list_marker_full(first).unwrap();
    let base_indent = first_marker.indent;
    let is_task = first_marker.checked.is_some();
    let is_ordered = first_marker.ordered;
    let mut items: Vec<ListItem> = Vec::new();
    let mut tight = true;
    let mut pending_blank = false;
    while let Some(line) = cur.peek() {
        if line.trim().is_empty() {
            tight = false;
            pending_blank = true;
            cur.consume();
            continue;
        }
        let Some(marker) = detect_list_marker_full(line) else {
            break;
        };
        if marker.indent < base_indent {
            break;
        }
        if marker.indent > base_indent {
            if let Some(last) = items.last_mut() {
                let nested = collect_indented_block(cur, base_indent);
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
        cur.consume();
        items.push(ListItem {
            attrs: None,
            checked: marker.checked,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_inline_with_options(marker.content, options),
            })],
        });
    }
    BlockNode::List(List {
        attrs: None,
        ordered: is_ordered,
        start: None,
        ol_type: None,
        tight,
        items,
    })
}

#[derive(Clone, Copy)]
struct ListMarker<'a> {
    indent: usize,
    ordered: bool,
    checked: Option<bool>,
    content: &'a str,
}

fn detect_list_marker_full(line: &str) -> Option<ListMarker<'_>> {
    let indent = leading_ws(line);
    if let Some((checked, content)) = detect_task(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: Some(checked),
            content,
        });
    }
    if let Some(content) = detect_ordered(line) {
        return Some(ListMarker {
            indent,
            ordered: true,
            checked: None,
            content,
        });
    }
    if let Some(content) = detect_unordered(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: None,
            content,
        });
    }
    None
}

fn collect_indented_block(cur: &mut LineCursor, parent_indent: usize) -> String {
    let mut lines = Vec::new();
    while let Some(line) = cur.peek() {
        if line.trim().is_empty() {
            lines.push(String::new());
            cur.consume();
            continue;
        }
        let indent = leading_ws(line);
        if indent <= parent_indent {
            break;
        }
        let strip = (parent_indent + 2).min(indent);
        lines.push(line[strip..].to_string());
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
    let joined = lines.join("\n");
    let (text, attrs) = split_trailing_attrs(&joined);
    BlockNode::Paragraph(Paragraph {
        attrs,
        children: parse_inline_with_options(text, options),
    })
}

fn is_block_start(line: &str) -> bool {
    detect_heading(line).is_some()
        || detect_fence_open(line).is_some()
        || detect_thematic_break(line)
        || line.starts_with('>')
        || is_list_marker(line)
        || is_table_start(line)
        || detect_admonition(line).is_some()
        || detect_abbreviation_def(line).is_some()
        || detect_block_image(line).is_some()
}

fn consume_caption(cur: &mut LineCursor, options: &Options<'_>) -> Option<Vec<InlineNode>> {
    let line = cur.peek()?;
    let text = line.strip_prefix("^ ")?;
    cur.consume();
    Some(parse_inline_with_options(text.trim_end(), options))
}

fn is_table_start(line: &str) -> bool {
    line.trim_start().starts_with("|=") || line.trim_start().starts_with('|')
}

fn parse_table(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let mut rows = Vec::new();
    while let Some(line) = cur.peek() {
        if !is_table_start(line) {
            break;
        }
        cur.consume();
        rows.push(parse_table_row(line, options));
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
        if ch == '|' {
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
    let text = if header { trimmed[1..].trim() } else { trimmed };
    let span = match text {
        "^" => Some(TableCellSpan::Rowspan),
        "<" => Some(TableCellSpan::Colspan),
        _ => None,
    };
    TableCell {
        header,
        span,
        align: None,
        children: if span.is_some() {
            Vec::new()
        } else {
            parse_inline_with_options(text, options)
        },
    }
}

fn detect_admonition(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(":::")?.trim();
    if rest.is_empty() {
        return None;
    }
    Some(rest.split_whitespace().next()?.to_string())
}

fn parse_admonition(cur: &mut LineCursor, kind: String, options: &Options<'_>) -> BlockNode {
    cur.consume();
    let mut inner = Vec::new();
    while let Some(line) = cur.peek() {
        if line.trim() == ":::" {
            cur.consume();
            break;
        }
        inner.push(line.to_string());
        cur.consume();
    }
    let children = parse_blocks_with_options(&inner.join("\n"), options);
    BlockNode::Admonition(Admonition {
        attrs: None,
        kind,
        title: None,
        children,
    })
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
    let Some(open) = trimmed.rfind('{') else {
        return (text, None);
    };
    if open == 0 || !trimmed[..open].ends_with(' ') {
        return (text, None);
    }
    let attrs = parse_attrs(&trimmed[open + 1..trimmed.len() - 1]);
    match attrs {
        Some(attrs) => (trimmed[..open].trim_end(), Some(attrs)),
        None => (text, None),
    }
}

fn read_attrs_at(bytes: &[u8], start: usize) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != b'}' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let inner = std::str::from_utf8(&bytes[start + 1..i]).ok()?;
    Some((parse_attrs(inner)?, i + 1))
}

fn parse_attrs(src: &str) -> Option<Attrs> {
    let mut attrs = Attrs::default();
    for token in src.split_whitespace() {
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
            attrs
                .key_values
                .insert(key.to_string(), value.trim_matches('"').to_string());
        } else {
            return None;
        }
    }
    Some(attrs)
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
            if let Some((link, consumed)) = parse_inline_link_with_options(bytes, i, options) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Link(link));
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
            if let Some((node, consumed)) = parse_inline_extension(bytes, i, options) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Extension(node));
                i += consumed;
                continue;
            }
        }

        // Bold-italic, sub, highlight, then single-char emphasis
        if let Some((node, consumed)) = match_emphasis(bytes, i, options) {
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
    Some((
        Image {
            attrs: None,
            src,
            alt,
            title,
        },
        after_paren - start,
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

fn parse_autolink(text: &str, pos: usize) -> Option<(AutoLink, usize)> {
    let rest = text.get(pos..)?;
    let close = rest.find('>')?;
    let target = &rest[1..close];
    if target.starts_with("http://") || target.starts_with("https://") {
        return Some((
            AutoLink {
                attrs: None,
                href: target.to_string(),
            },
            close + 1,
        ));
    }
    if target.contains('@') && !target.contains(' ') {
        return Some((
            AutoLink {
                attrs: None,
                href: format!("mailto:{target}"),
            },
            close + 1,
        ));
    }
    None
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
