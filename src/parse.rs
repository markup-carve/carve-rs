//! Carve parser (MVP subset).
//!
//! Block-level reads line by line; inline does a single linear scan
//! over each block's text. No backtracking.

use crate::ast::*;
use crate::extension::{BlockMatch, InlineMatch, MatcherContext, Options};
use std::cell::Cell;
use std::collections::BTreeMap;

/// Maximum block + inline nesting depth. Pathological input (deeply nested
/// blockquotes, indented lists, bracketed inlines) recurses one stack frame
/// per level; without a cap a ~1000-deep document aborts the process with a
/// stack overflow (uncatchable -- a hard DoS for any embedder). Over the cap
/// the parser degrades gracefully (remaining block content becomes a flat
/// paragraph; inline content stays literal text) instead of recursing further.
///
/// The cap also bounds the depth of the AST the renderers walk recursively, so
/// it is chosen to keep parse + render safe on a conservative 2 MiB thread
/// stack (render alone overflows ~130-deep there); 100 levels is far beyond any
/// real document while leaving comfortable headroom. carve-php's analogous cap
/// is higher only because PHP grows its VM stack on the heap.
const MAX_NESTING_DEPTH: usize = 100;

thread_local! {
    // Plain initializer (not `const { … }`) to keep the crate's 1.75 MSRV;
    // the inline-const thread-local form clippy suggests requires Rust 1.79+,
    // so the lint is allowed here rather than followed.
    #[allow(clippy::missing_const_for_thread_local)]
    static NESTING_DEPTH: Cell<usize> = Cell::new(0);
}

/// RAII recursion-depth guard. `enter()` returns `None` when the cap is
/// already reached (the caller must degrade without recursing); otherwise it
/// increments the shared depth and returns a guard that decrements on drop
/// (including during panic unwind, so a normal parse always returns to 0).
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<DepthGuard> {
        NESTING_DEPTH.with(|d| {
            if d.get() >= MAX_NESTING_DEPTH {
                None
            } else {
                d.set(d.get() + 1);
                Some(DepthGuard)
            }
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        NESTING_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

pub fn parse(source: &str) -> Document {
    parse_with_options(source, &Options::default())
}

pub fn parse_with_options(source: &str, options: &Options<'_>) -> Document {
    // Normalize input up front (matching carve-js / carve-php), only allocating
    // when needed:
    //  - strip a single leading UTF-8 BOM (U+FEFF) so `﻿# T` is a heading;
    //  - collapse CRLF / CR to LF;
    //  - replace a NUL (U+0000) with the U+FFFD replacement char so a control
    //    byte never reaches output (WHATWG-style).
    let normalized;
    let source = if source.starts_with('\u{feff}') || source.contains('\r') || source.contains('\0')
    {
        let trimmed = source.strip_prefix('\u{feff}').unwrap_or(source);
        normalized = trimmed
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\0', "\u{fffd}");
        normalized.as_str()
    } else {
        source
    };
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
    resolve_crossrefs(&mut doc, options.lowercase_heading_ids);
    // Single post-resolution pass: a link may not contain another link. Runs
    // after reference and cross-reference resolution because both produce
    // `Link` nodes only at that stage; running earlier would miss the anchors
    // they create. Applied over document inline content and footnote bodies.
    enforce_no_nesting(&mut doc);
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
            if label_part.starts_with('@') {
                body.push(line);
                continue;
            }
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
    // A title needs the opening AND a distinct closing quote: a lone `"` (or
    // `'`) satisfies both starts_with and ends_with on the same byte, so guard
    // len >= 2 before `rest[1..len-1]` underflows (begin > end panic).
    let title = if rest.len() >= 2
        && ((rest.starts_with('"') && rest.ends_with('"'))
            || (rest.starts_with('\'') && rest.ends_with('\'')))
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
    // The closer is a line that is exactly `---`. It may be the FIRST line of
    // `rest` (an empty frontmatter, `---\n---`) or follow a newline.
    let (content_len, after) = if rest == "---" {
        (0, rest.len())
    } else if let Some(r) = rest.strip_prefix("---\n") {
        (0, rest.len() - r.len())
    } else if let Some(close) = rest.find("\n---\n") {
        (close, close + 5)
    } else if let Some(close) = rest.strip_suffix("\n---").map(|s| s.len()) {
        (close, rest.len())
    } else {
        return (BTreeMap::new(), source);
    };
    let frontmatter_src = &rest[..content_len];
    let body = &rest[after..];
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
    // Recursion cap (see MAX_NESTING_DEPTH). Over the cap, flatten everything
    // still in the cursor into one paragraph rather than recursing further,
    // matching the carve-php degrade behavior.
    let Some(_depth) = DepthGuard::enter() else {
        let mut rest: Vec<&str> = Vec::new();
        while let Some(line) = cur.consume() {
            rest.push(line);
        }
        let text = rest.join("\n");
        if text.trim().is_empty() {
            return Vec::new();
        }
        return vec![BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: parse_inline_with_options(&text, options),
        })];
    };
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
        let block = parse_fence(cur, fence_marker);
        // A caption immediately after a fenced code block makes it a numbered
        // LISTING: wrap it in a figure like a captioned image/table.
        if let BlockNode::CodeBlock(cb) = block {
            if let Some(caption) = consume_caption(cur, options) {
                return Some(BlockNode::Figure(Figure {
                    attrs: None,
                    target: FigureTarget::CodeBlock(cb),
                    caption,
                }));
            }
            return Some(BlockNode::CodeBlock(cb));
        }
        return Some(block);
    }
    if detect_thematic_break(line) {
        cur.consume();
        return Some(BlockNode::ThematicBreak(ThematicBreak::default()));
    }
    if let Some((level, first_text)) = detect_heading(line) {
        cur.consume();
        // Headings are multi-line: the text spills onto following lines until a
        // blank line. A continuation line may carry EXACTLY the same number of
        // `#` (stripped) or none; a different `#` count (more or fewer) starts a
        // new heading, and a caption or a fenced comment (`%%%`) ends it. A
        // block-opener (list/quote/table/fence/div/thematic break) ends it and
        // starts that block, exactly as it interrupts a paragraph (§10); only
        // plain text folds (an ordered marker folds, it never interrupts).
        let mut joined = first_text.to_string();
        while let Some(next) = cur.peek() {
            if next.trim().is_empty() {
                break;
            }
            if let Some(cont) = heading_continuation_same_level(next, level) {
                joined.push('\n');
                joined.push_str(cont);
                cur.consume();
                continue;
            }
            if is_heading_marker_line(next) || next.starts_with("^ ") || is_comment_fence_line(next)
            {
                break;
            }
            // A list marker ENDS the heading and starts a sibling list (it does
            // not fold in). Symmetric §10: a list marker does not interrupt a
            // PARAGRAPH (it folds), but a heading is ended by it -- matching djot
            // (`# T` / `- x` -> heading + list). Bullet and ordered alike.
            if is_list_marker(next) || interrupts_paragraph(next, &cur.lines[cur.pos + 1..]) {
                break;
            }
            joined.push('\n');
            joined.push_str(next);
            cur.consume();
        }
        // djot-strict (spec PART 2 headings; matches carve-js #153): a heading
        // line carries NO trailing `{...}` attribute block -- a trailing brace
        // block is ordinary inline content, and the heading id derives from
        // the full literal text. Attributes attach via a PRECEDING
        // block-attribute line (the pending-attrs loop, PART 9 §15).
        return Some(BlockNode::Heading(Heading {
            attrs: None,
            level,
            children: parse_inline_with_options(&joined, options),
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
    if let Some(fence_len) = detect_line_block_open(line) {
        // A line block, like any colon fence, opens only when a matching
        // closer exists ahead (grammar §12/§23); an unterminated `::: |`
        // stays literal instead of swallowing the rest of the document.
        let has_closer = cur.lines[cur.pos + 1..].iter().any(|l| {
            let t = l.trim();
            !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= fence_len
        });
        if has_closer {
            return Some(parse_line_block(cur, options));
        }
    }
    if let Some(open) = detect_container_open(line) {
        // A colon fence opens only when a matching closer (a line of at least
        // `fence_len` colons) exists ahead (grammar §12); an unterminated
        // `:::` / `::: note` stays literal instead of swallowing the rest of
        // the document. Matches carve-js.
        let has_closer = cur.lines[cur.pos + 1..].iter().any(|l| {
            let t = l.trim();
            !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= open.fence_len
        });
        if has_closer {
            return Some(parse_container(cur, options));
        }
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
    // A block whose sole content is a display-math span (`$$`…``) followed by a
    // caption is a numbered EQUATION. Diverted before the paragraph fallback so
    // parse_paragraph does not fold the caption line into the math paragraph.
    if line.trim_start().starts_with("$$`") {
        if let Some(eq) = parse_equation_block(cur, options) {
            return Some(eq);
        }
    }
    Some(parse_paragraph(cur, options))
}

/// Parse a standalone display-math line, wrapping it in a figure when a caption
/// follows (a numbered equation). Returns `None` when the line is not solely
/// display math, or when non-blank prose follows with no blank line (so the
/// line belongs to a normal multi-line paragraph instead).
fn parse_equation_block(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    let line = cur.peek()?;
    let inline = parse_inline_with_options(line.trim_start(), options);
    if inline.len() != 1 || !matches!(&inline[0], InlineNode::Math(m) if m.display) {
        return None;
    }
    // Non-blank, non-caption prose on the very next line: let parse_paragraph
    // fold the math and that text into one paragraph (preserve existing behavior).
    if let Some(next) = cur.lines.get(cur.pos + 1).copied() {
        if !next.trim().is_empty() && next.strip_prefix("^ ").is_none() {
            return None;
        }
    }
    // Standalone display math: consume the line, then attach a caption if one
    // follows (directly or across a single blank line).
    cur.consume();
    let target = FigureTarget::Paragraph(Paragraph {
        attrs: None,
        children: inline,
    });
    if let Some(caption) = consume_caption(cur, options) {
        return Some(BlockNode::Figure(Figure {
            attrs: None,
            target,
            caption,
        }));
    }
    match target {
        FigureTarget::Paragraph(p) => Some(BlockNode::Paragraph(p)),
        _ => unreachable!(),
    }
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

/// A heading continuation line carrying EXACTLY `level` `#` markers, a space,
/// then non-empty text. Returns the text after the markers (markers stripped),
/// as in Djot ("may be preceded by the same number of `#` characters"). A
/// different count (more or fewer) returns None, so that line starts a NEW
/// heading instead of continuing the current one.
fn heading_continuation_same_level(line: &str, level: u8) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if hashes != level as usize {
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
    // 3+ of the SAME `-`/`*`/`_`, optionally separated by spaces/tabs, with
    // nothing else on the line (`---`, `- - -`, `* * *`). Matches carve-js,
    // carve-php, and canonical djot. A mixed run (`-*-`) is not a break.
    let trimmed = line.trim();
    for marker in [b'-', b'*', b'_'] {
        let mut count = 0usize;
        let mut only_marker_and_space = true;
        for &b in trimmed.as_bytes() {
            if b == marker {
                count += 1;
            } else if b != b' ' && b != b'\t' {
                only_marker_and_space = false;
                break;
            }
        }
        if only_marker_and_space && count >= 3 {
            return true;
        }
    }
    false
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
    // Raw passthrough block: `=FORMAT` (§4.15, djot raw-block syntax) -- a
    // leading `=` immediately followed by the format name. The `=` is the block
    // parallel of the inline raw `{=format}` attribute; it is never part of a
    // language token, so this is unambiguous against an ordinary code block.
    // parse_fence recovers raw blocks by the leading `=` in this span. The `=`
    // and format name must be adjacent (`=html`); `= html` is not raw.
    if i < bytes.len() && bytes[i] == b'=' {
        i += 1;
        if i >= bytes.len() || !bytes[i].is_ascii_alphabetic() {
            return None;
        }
        i += 1;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
        {
            i += 1;
        }
        let lang_end = i;
        // Must be only whitespace after the format name
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i != bytes.len() {
            return None;
        }
        return Some(FenceOpen {
            indent,
            fence_char,
            fence_len,
            lang_start,
            lang_end,
        });
    }
    // Language token charset covers real-world tags with punctuation
    // (c++, c#, f#, asp.net, text/html); the token is still anchored (no
    // whitespace), so a multiword/quoted info string is not a fence. `/` is
    // allowed so MIME-like tags stay a single language token.
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric()
            || bytes[i] == b'_'
            || bytes[i] == b'-'
            || bytes[i] == b'+'
            || bytes[i] == b'#'
            || bytes[i] == b'.'
            || bytes[i] == b'/')
    {
        i += 1;
    }
    let lang_end = i;
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
    let raw_format = open_trim.strip_prefix('=').map(|f| f.trim().to_string());
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
                // An open paragraph requires plain paragraph text. A stripped
                // line that is itself a block-opener (heading, thematic break,
                // table row, `:::` div / line block opener) leaves NO open
                // paragraph -- so a following list marker has nothing to fold
                // into and must end the quote. Reuse `interrupts_paragraph`
                // (the §10 predicate): a line that would interrupt a paragraph
                // is, by definition, not paragraph continuation text.
                // `interrupts_paragraph` only consults the lookahead for a
                // FENCED-CODE opener (its closer probe); `:::` container openers
                // are already excluded by the detect_container_open check below.
                // Build the remaining-quoted-body slice ONLY for a fence opener,
                // so an ordinary long quote stays linear instead of O(n^2).
                let rest_stripped: Vec<&str> = if detect_fence_open(stripped).is_some() {
                    cur.lines[cur.pos..]
                        .iter()
                        .take_while(|l| l.starts_with('>'))
                        .map(|l| {
                            let s = l.strip_prefix('>').unwrap_or(l);
                            s.strip_prefix(' ').unwrap_or(s)
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                para_open = !stripped.trim().is_empty()
                    && detect_container_open(stripped).is_none()
                    && !stripped.trim_start().starts_with("%%")
                    && !interrupts_paragraph(stripped, &rest_stripped);
            }
            inner.push(stripped.to_string());
            continue;
        }
        // Continuation marker (Carve, PART 9 §17): a lone `+` at column 0 after
        // a quoted line attaches the FOLLOWING flush-left block to the quote --
        // the un-prefixed analogue of the list-item form, so a real block (list,
        // fenced code, table, ...) joins the quote without repeating `>`. Collect
        // the block's lines (up to a blank line, a `>` line, or a further `+`)
        // and splice them into the quote body behind a blank-line separator, so
        // they parse as their own block instead of folding into the quoted
        // paragraph. The marker only attaches; a blank line still ends the quote
        // and a `+` outside a container stays literal.
        if line.trim() == "+" && indent_columns(line) == 0 {
            cur.consume();
            let mut attached: Vec<String> = Vec::new();
            while let Some(&next) = cur.lines.get(cur.pos) {
                if next.trim().is_empty()
                    || next.starts_with('>')
                    || (next.trim() == "+" && indent_columns(next) == 0)
                {
                    break;
                }
                attached.push(next.to_string());
                cur.pos += 1;
            }
            if !attached.is_empty() {
                // `inner` always holds the quote's first content line, so a
                // leading blank separates the attached block from it.
                inner.push(String::new());
                inner.extend(attached);
                inner.push(String::new());
                para_open = false;
            }
            continue;
        }
        // Lazy continuation: a non-`>` line folds into an OPEN paragraph. A
        // blank line, a caption, or a line that starts a block ends the quote.
        // A list marker FOLDS into the open quoted paragraph as literal text --
        // the quoted paragraph follows the same rule as a top-level paragraph,
        // where a list marker does not interrupt (it needs a blank line before
        // it). `interrupts_paragraph` is the shared predicate for that decision,
        // and it already returns false for bullet/task/ordered markers, so we
        // simply defer to it. A heading is the sole construct a list marker
        // would otherwise end, and headings still interrupt via that predicate.
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

/// Read an attribute block abutting a list marker (`-{.c}` / `3.{#x}`):
/// the parsed attributes (`None` for an empty `{}` block) and the byte index
/// just past the closing `}`. Returns `None` when there is no closing brace
/// or the content is not a valid attribute list -- in which case the marker
/// is not a list item (the line is ordinary text, grammar `item_attributes`).
fn read_list_item_attrs(bytes: &[u8], start: usize) -> Option<(Option<Attrs>, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            quote = Some(b);
        } else if b == b'}' {
            let inner = std::str::from_utf8(&bytes[start + 1..i]).ok()?;
            let end = i + 1;
            return if inner.trim().is_empty() {
                Some((None, end))
            } else {
                Some((Some(parse_attrs(inner)?), end))
            };
        }
        i += 1;
    }
    None
}

/// The text after a list marker: an optional abutting attribute block, then
/// the marker's required single space, then the item content. Returns the
/// content (trailing whitespace trimmed) and the item attributes. `None` when
/// the required space is missing or an abutting `{...}` is not a valid
/// attribute block (so the line is not a list item). A SPACE before `{` is
/// ordinary content, not an item-attribute, so it is handled by the plain
/// space branch and the `{...}` stays in the content.
fn marker_tail(line: &str, marker_end: usize) -> Option<(&str, Option<Attrs>)> {
    let bytes = line.as_bytes();
    let (content, attrs) = match bytes.get(marker_end) {
        Some(&b' ') => (line[marker_end + 1..].trim_end(), None),
        Some(&b'{') => {
            let (attrs, end) = read_list_item_attrs(bytes, marker_end)?;
            if bytes.get(end) != Some(&b' ') {
                return None;
            }
            (line[end + 1..].trim_end(), attrs)
        }
        _ => return None,
    };
    // A marker with no same-line content is not a list item -- a bare `- `
    // (or `-{.c} `) is ordinary text (matches carve-js / carve-php; a list
    // item carries its content on the marker line).
    if content.is_empty() {
        return None;
    }
    Some((content, attrs))
}

fn detect_unordered(line: &str) -> Option<(&str, Option<Attrs>)> {
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
    marker_tail(line, i + 1)
}

fn detect_ordered(line: &str) -> Option<&str> {
    detect_ordered_full(line).map(|(content, _, _, _, _, _)| content)
}

#[allow(clippy::type_complexity)]
fn detect_ordered_full(
    line: &str,
) -> Option<(
    &str,
    Option<usize>,
    Option<OrderedListType>,
    Option<Attrs>,
    u8,
    &str,
)> {
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
    if bytes.get(i) != Some(&b'.') && bytes.get(i) != Some(&b')') {
        return None;
    }
    let delim = bytes[i];
    // The required space may be preceded by an abutting attribute block
    // (`3.{#x} item`); `marker_tail` enforces the space and rejects an
    // invalid block.
    let (content, attrs) = marker_tail(line, i + 1)?;
    let marker = &line[marker_start..i];
    if marker.bytes().all(|b| b.is_ascii_digit()) {
        return Some((
            content,
            marker.parse::<usize>().ok().filter(|n| *n != 1),
            None,
            attrs,
            delim,
            marker,
        ));
    }
    // A single letter is ALPHA by default, EXCEPT a lone `i`/`I`, which defaults
    // to roman (§11 ambiguous-letter rule; the list parser may re-classify
    // either way when a consecutive sibling disambiguates).
    if marker.len() == 1 && !marker.eq_ignore_ascii_case("i") {
        let b = marker.as_bytes()[0];
        if b.is_ascii_lowercase() {
            return Some((
                content,
                Some((b - b'a' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::LowerAlpha),
                attrs,
                delim,
                marker,
            ));
        }
        if b.is_ascii_uppercase() {
            return Some((
                content,
                Some((b - b'A' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::UpperAlpha),
                attrs,
                delim,
                marker,
            ));
        }
    }
    let roman = roman_to_int(marker)?;
    Some((
        content,
        Some(roman).filter(|n| *n != 1),
        Some(if marker.chars().all(|c| c.is_ascii_uppercase()) {
            OrderedListType::UpperRoman
        } else {
            OrderedListType::LowerRoman
        }),
        attrs,
        delim,
        marker,
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

fn detect_task(line: &str) -> Option<(bool, &str, Option<Attrs>)> {
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
    // An attribute block abuts the bullet, BEFORE the task marker:
    // `-{.c} [ ] text`. `marker_tail` consumes the optional block and the
    // bullet's required space; the task box `[x] ` then opens the content.
    let (after, attrs) = marker_tail(line, i + 1)?;
    let ab = after.as_bytes();
    if ab.len() < 4 || ab[0] != b'[' || ab[2] != b']' || ab[3] != b' ' {
        return None;
    }
    let checked = matches!(ab[1], b'x' | b'X');
    Some((checked, after[4..].trim_end(), attrs))
}

/// Lower-alpha index of a single letter (`a`=1 … `z`=26), case-insensitive.
fn alpha_index(m: &str) -> Option<usize> {
    if m.len() != 1 {
        return None;
    }
    let b = m.as_bytes()[0].to_ascii_lowercase();
    (b.is_ascii_lowercase()).then(|| (b - b'a' + 1) as usize)
}

/// Resolve the §11 ambiguous-letter tie-break for an ordered list's FIRST
/// marker, returning its `(start, ol_type)`. A single roman-letter marker
/// (i/v/x/l/c/d/m) is reclassified to ROMAN when the next sibling is the
/// consecutive roman numeral, to ALPHA when the next is the consecutive letter;
/// otherwise the detector's default stands (lone `i`/`I` roman, others alpha).
fn resolve_ordered_first(
    first: &ListMarker<'_>,
    cur: &LineCursor,
    base_indent: usize,
) -> (Option<usize>, Option<OrderedListType>) {
    if !first.ordered || !is_ambiguous_roman_letter(first.marker) {
        return (first.start, first.ol_type);
    }
    // Find the next sibling ordered marker at the same indent, skipping the
    // first item's own body (blank lines and lines indented past the base).
    let mut sibling = None;
    for l in &cur.lines[cur.pos + 1..] {
        if l.trim().is_empty() {
            continue;
        }
        if indent_columns(l) > base_indent {
            continue; // part of the first item's body
        }
        sibling = detect_list_marker_full(l).filter(|m| m.ordered && m.indent == base_indent);
        break;
    }
    let upper = first.marker.chars().all(|c| c.is_ascii_uppercase());
    let roman_type = if upper {
        OrderedListType::UpperRoman
    } else {
        OrderedListType::LowerRoman
    };
    let alpha_type = if upper {
        OrderedListType::UpperAlpha
    } else {
        OrderedListType::LowerAlpha
    };
    if let Some(sib) = sibling {
        let first_roman = roman_to_int(first.marker);
        let sib_roman = roman_to_int(sib.marker).filter(|_| !sib.marker.is_empty());
        if let (Some(fr), Some(sr)) = (first_roman, sib_roman) {
            // sibling is itself a roman-shaped marker and the consecutive value
            if sr == fr + 1 {
                return (Some(fr).filter(|n| *n != 1), Some(roman_type));
            }
        }
        if let (Some(fa), Some(sa)) = (alpha_index(first.marker), alpha_index(sib.marker)) {
            if sa == fa + 1 {
                return (Some(fa).filter(|n| *n != 1), Some(alpha_type));
            }
        }
    }
    (first.start, first.ol_type)
}

/// Parse ONE block attached by a list `+` continuation marker, bounded to the
/// lines before the next lone `+` marker at the item's base indent. The scan is
/// fence-aware -- a `+` inside a nested fenced code block is content, not a
/// boundary -- so a greedy block (e.g. a block quote's lazy continuation)
/// cannot swallow the following `+` and its block. `- a / + / >q1 / + / >q2`
/// then yields two separate quotes. Advances `cur` by the lines consumed.
fn parse_continuation_block(
    cur: &mut LineCursor,
    options: &Options<'_>,
    base_indent: usize,
) -> Option<BlockNode> {
    // A nested list manages its OWN `+` continuations -- the boundary scan
    // cannot tell a child list's `+` from the parent's, so a list is parsed
    // unbounded. (Code / colon fences are handled INSIDE the scan: a fence with
    // a matching closer is skipped so its inner `+` is content, while an
    // unterminated one is not, so a following `+` still bounds the block.)
    if let Some(line) = cur.peek() {
        if let Some(nm) = detect_list_marker_full(line) {
            // A marker indented past the base nests as a child list of THIS
            // item, so parse it unbounded (it manages its own `+`). But a
            // marker AT or BELOW the outer base column is a SIBLING of the
            // outer list, not content of this `+`-attached block: bound the
            // block to empty so the outer list takes the marker as a sibling
            // item rather than nesting it (matches carve-php for `+`-then-
            // marker, e.g. `- a / + / text / + / - b`).
            if nm.indent > base_indent {
                return parse_block(cur, options);
            }
            return None;
        }
    }
    let mut end = cur.pos;
    let mut in_fence: Option<FenceOpen> = None;
    while end < cur.lines.len() {
        let line = cur.lines[end];
        if let Some(open) = in_fence {
            if is_fence_close(line, open) {
                in_fence = None;
            }
            end += 1;
            continue;
        }
        if let Some(open) = detect_fence_open(line) {
            in_fence = Some(open);
            end += 1;
            continue;
        }
        // A colon fence (`:::` div / admonition / `::: |` line block) WITH a
        // matching closer ahead is a self-delimiting block; skip the whole
        // region so a `+` inside it is content, not the parent's boundary.
        // (An UNTERMINATED `:::` is literal -- no closer to skip to.)
        if detect_container_open(line).is_some() || detect_line_block_open(line).is_some() {
            let fence_len = line.trim_start().bytes().take_while(|b| *b == b':').count();
            let closer = (end + 1..cur.lines.len()).find(|&j| {
                let t = cur.lines[j].trim();
                !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= fence_len
            });
            if let Some(close) = closer {
                end = close + 1;
                continue;
            }
        }
        if end > cur.pos && line.trim() == "+" && indent_columns(line) == base_indent {
            break;
        }
        // A list marker at (or below) the base column is a SIBLING item of the
        // outer list, not part of this `+`-attached block. Bound the block here
        // so it is not absorbed -- now that a bullet does not interrupt, a
        // `> quote` (or other) block would otherwise swallow a following
        // `- next` as lazy continuation. Matches carve-js.
        if end > cur.pos
            && indent_columns(line) <= base_indent
            && detect_list_marker_full(line).is_some()
        {
            break;
        }
        end += 1;
    }
    let slice: Vec<&str> = cur.lines[cur.pos..end].to_vec();
    let mut sub = LineCursor {
        lines: &slice,
        pos: 0,
    };
    let block = parse_block(&mut sub, options);
    cur.pos += sub.pos;
    block
}

fn parse_list(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let first = cur.peek().unwrap();
    let first_marker = detect_list_marker_full(first).unwrap();
    let base_indent = first_marker.indent;
    let is_task = first_marker.checked.is_some();
    let is_ordered = first_marker.ordered;
    // §11 ambiguous-letter tie-break: a single roman-letter first marker is
    // roman or alpha depending on its consecutive sibling. Resolve it against
    // the next sibling marker before fixing the list's type.
    let (start, ol_type) = resolve_ordered_first(&first_marker, cur, base_indent);
    let first_delim = first_marker.delim;
    let first_dialect = ol_dialect(ol_type);
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
            if let Some(block) = parse_continuation_block(cur, options, base_indent) {
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
                    let mut nested = collect_indented_block(cur, base_indent, content_col);
                    // A heading folds its trailing plain text as continuation
                    // (PART 2 headings). When the indented block ends in a
                    // heading and the next lines are flush-left lazy text, pull
                    // them in so the heading parser folds them into the heading
                    // rather than the list ending and the text floating to the
                    // top level (matches carve-php). Only headings fold this
                    // way: a code block or table keeps its trailing text as a
                    // separate top-level block, so the guard is heading-only.
                    if !pending_blank && nested_ends_with_heading(&nested, options) {
                        collect_trailing_lazy(cur, &mut nested);
                    } else if !pending_blank && nested_ends_with_open_paragraph(&nested, options) {
                        // CommonMark lazy continuation: the dedented non-blank
                        // line folds into the nested block's deepest open
                        // paragraph (e.g. a block quote's trailing paragraph) so
                        // it stays INSIDE the item. The recursive block parse
                        // (block quote lazy continuation) absorbs it.
                        collect_trailing_lazy(cur, &mut nested);
                    }
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
                let sub_indent = marker.indent;
                let mut nested = collect_indented_block(cur, base_indent, content_col);
                // A column-0 lazy-continuation line folds into the sub-list's
                // last open paragraph (e.g. `inner` / `lazy`). It must NOT close
                // the sub-list: a following sibling marker at the sub-list's own
                // column (`2. sibling`) resumes the SAME list. Loop folding the
                // lazy line, then resume collecting the sub-list continuation, so
                // the sibling joins the open list rather than starting a new one
                // (corpus 05-lists-17, matches carve-php / carve-js).
                loop {
                    // Cheap peek first: is the next line a flush-left line that
                    // collect_trailing_lazy could actually fold? If not, stop --
                    // WITHOUT reparsing `nested` (the open-paragraph check below
                    // reparses the whole subtree, so it must run only when there
                    // is a lazy line pending, else deeply nested lists blow up).
                    let has_lazy = cur.peek().is_some_and(|line| {
                        !line.trim().is_empty()
                            && indent_columns(line) == 0
                            && !is_list_marker(line)
                            && !interrupts_paragraph(line, &cur.lines[cur.pos + 1..])
                    });
                    if !has_lazy {
                        break;
                    }
                    // Only fold the column-0 lazy line when the collected content
                    // still ends in an OPEN paragraph. After a CLOSED block
                    // (fenced code, table, div) there is none, so the dedented
                    // line ends the item -> top-level (family-D rule).
                    if !nested_ends_with_open_paragraph(&nested, options) {
                        break;
                    }
                    let before = cur.pos;
                    collect_trailing_lazy(cur, &mut nested);
                    if cur.pos == before {
                        break;
                    }
                    let resumed = collect_indented_block(cur, sub_indent - 1, content_col);
                    if !resumed.is_empty() {
                        nested.push('\n');
                        nested.push_str(&resumed);
                    }
                }
                let nested_children = parse_blocks_with_options(&nested, options);
                last.children.extend(nested_children);
                continue;
            }
            break;
        }
        if marker.ordered != is_ordered || marker.checked.is_some() != is_task {
            break;
        }
        // §11: an ordered item whose delimiter (`.` vs `)`) or dialect family
        // (decimal / alpha / roman, case included) differs from the list's first
        // item starts a NEW sibling list. Skip the FIRST item: its own detected
        // dialect may differ from the list's resolved (tie-broken) dialect
        // (`v.` detects alpha but the list is roman), and it can never split
        // from itself.
        if is_ordered
            && !items.is_empty()
            && (marker.delim != first_delim || !dialect_compatible(first_dialect, &marker))
        {
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
                attrs: marker.attrs.clone(),
                checked: marker.checked,
                children: Vec::new(),
            };
            if let Some(block) = parse_continuation_block(cur, options, base_indent) {
                item.children.push(block);
            }
            items.push(item);
            continue;
        }
        // When the item's content BEGINS, on the marker line, with another list
        // marker (`- - A`, `* - A`, `1. - A`, ...), the lead is itself a
        // sub-list, not a paragraph. Parse the lead together with every
        // following dedented line as ONE block stream so the marker-line
        // sub-list behaves exactly like a sub-list opened on a *following* line:
        // following same-indent markers MERGE into it as siblings, and
        // post-blank indented blocks are ABSORBED into its items. This MATCHES
        // reference djot.js (@djot/djot 0.3.2) and CommonMark, which both treat
        // a marker-line sub-list as a normal nested list. It corrects Carve's
        // prior line-scoping (which split the sub-list from following items and
        // leaked later indented blocks to the parent row) -- a bug inherited
        // from djot-php, whose marker-line handling deviates from reference
        // djot. The combined stream reuses the normal nested-list/absorption
        // logic (collect_indented_block + recursive parse) -- no separate path.
        if detect_list_marker_full(marker.content).is_some() {
            let mut stream = marker.content.to_string();
            let following = collect_indented_block(cur, base_indent, content_col);
            if !following.is_empty() {
                stream.push('\n');
                stream.push_str(&following);
            }
            // A column-0 lazy-continuation line following the marker-line
            // sub-list folds into its last open paragraph (`- - b` / `lazy` ->
            // `<li>b\nlazy</li>`), and a following sibling marker at the
            // sub-list's column resumes the SAME list. This is the same
            // lazy-fold / resume loop the following-line nested-list path runs
            // above; reused here so the marker-line sub-list behaves identically.
            loop {
                let has_lazy = cur.peek().is_some_and(|line| {
                    !line.trim().is_empty()
                        && indent_columns(line) == 0
                        && !is_list_marker(line)
                        && !interrupts_paragraph(line, &cur.lines[cur.pos + 1..])
                });
                if !has_lazy {
                    break;
                }
                if !nested_ends_with_open_paragraph(&stream, options) {
                    break;
                }
                let before = cur.pos;
                collect_trailing_lazy(cur, &mut stream);
                if cur.pos == before {
                    break;
                }
                let resumed = collect_indented_block(cur, content_col - 1, content_col);
                if !resumed.is_empty() {
                    stream.push('\n');
                    stream.push_str(&resumed);
                }
            }
            let children = parse_blocks_with_options(&stream, options);
            items.push(ListItem {
                attrs: marker.attrs.clone(),
                checked: marker.checked,
                children,
            });
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
                // A marker indented past the base but BELOW this item's content
                // column is lazy continuation, not a sub-list: under symmetric
                // §10 no list marker (bullet, task, or ordered) interrupts a
                // paragraph, so fold it. A marker AT or ABOVE the content column
                // nests; one at the base column is a sibling (ends the paragraph).
                let folds = nm.indent > base_indent && nm.indent < content_col;
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
            attrs: marker.attrs.clone(),
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

#[derive(Clone)]
struct ListMarker<'a> {
    indent: usize,
    ordered: bool,
    checked: Option<bool>,
    start: Option<usize>,
    ol_type: Option<OrderedListType>,
    content: &'a str,
    attrs: Option<Attrs>,
    /// Ordered-marker delimiter (`.` or `)`); `None` for bullets/tasks. A change
    /// in delimiter starts a new sibling list (§11).
    delim: Option<u8>,
    /// The raw ordered marker text (`i`, `iv`, `3`, `b`); used to re-classify an
    /// ambiguous single roman-letter via its sibling (§11 tie-break).
    marker: &'a str,
}

/// Coarse ordered-list dialect family for the §11 same-list test: decimal,
/// alphabetic, or roman (case included). A change splits the list.
#[derive(PartialEq, Eq, Clone, Copy)]
enum OlDialect {
    Decimal,
    Alpha(bool),
    Roman(bool),
}

fn ol_dialect(ol_type: Option<OrderedListType>) -> OlDialect {
    match ol_type {
        None => OlDialect::Decimal,
        Some(OrderedListType::LowerAlpha) => OlDialect::Alpha(false),
        Some(OrderedListType::UpperAlpha) => OlDialect::Alpha(true),
        Some(OrderedListType::LowerRoman) => OlDialect::Roman(false),
        Some(OrderedListType::UpperRoman) => OlDialect::Roman(true),
    }
}

/// Does an ordered `marker` keep the list's dialect (no §11 dialect split)? A
/// non-ambiguous marker must match the family exactly; an ambiguous single
/// roman-letter is compatible with EITHER a roman or an alpha list of the same
/// case (it continues as that dialect), but never a decimal list.
fn dialect_compatible(first: OlDialect, marker: &ListMarker<'_>) -> bool {
    if is_ambiguous_roman_letter(marker.marker) {
        let upper = marker.marker.chars().all(|c| c.is_ascii_uppercase());
        match first {
            OlDialect::Roman(u) | OlDialect::Alpha(u) => u == upper,
            OlDialect::Decimal => false,
        }
    } else {
        ol_dialect(marker.ol_type) == first
    }
}

/// Is `m` a single roman-letter marker (i/v/x/l/c/d/m, either case)? Such a
/// marker is dialect-AMBIGUOUS: roman or alpha depending on its sibling (§11).
fn is_ambiguous_roman_letter(m: &str) -> bool {
    m.len() == 1
        && matches!(
            m.to_ascii_lowercase().as_str(),
            "i" | "v" | "x" | "l" | "c" | "d" | "m"
        )
}

fn detect_list_marker_full(line: &str) -> Option<ListMarker<'_>> {
    let indent = indent_columns(line);
    if let Some((checked, content, attrs)) = detect_task(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: Some(checked),
            start: None,
            ol_type: None,
            content,
            attrs,
            delim: None,
            marker: "",
        });
    }
    if let Some((content, start, ol_type, attrs, delim, marker)) = detect_ordered_full(line) {
        return Some(ListMarker {
            indent,
            ordered: true,
            checked: None,
            start,
            ol_type,
            content,
            attrs,
            delim: Some(delim),
            marker,
        });
    }
    if let Some((content, attrs)) = detect_unordered(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: None,
            start: None,
            ol_type: None,
            content,
            attrs,
            delim: None,
            marker: "",
        });
    }
    None
}

/// After a nested block is collected for a list item, pull any immediately
/// following column-0 lazy-continuation lines into it (plain text only -- not a
/// blank line, a list marker, or a block-opener). Appended at column 0 so the
/// recursive parse folds them into the DEEPEST open item, matching carve-js and
/// carve-php (`- a` / `  - b` / `lazy` -> `<li>b lazy</li>`).
/// Whether the collected nested block ends in a heading. Used to decide if
/// flush-left lazy text following an indented heading-in-item should fold into
/// the heading (heading continuation) rather than ending the item.
fn nested_ends_with_heading(nested: &str, options: &Options<'_>) -> bool {
    matches!(
        parse_blocks_with_options(nested, options).last(),
        Some(BlockNode::Heading(_))
    )
}

/// Whether the collected nested block ends in an OPEN paragraph -- i.e. its
/// last block is a paragraph, or a container (block quote / div / admonition)
/// whose last child recursively ends in a paragraph. CommonMark lazy
/// continuation folds a following dedented non-blank line into the deepest open
/// paragraph: when a list item's last block is a block quote whose trailing
/// block is a paragraph, the dedented line is the quote's own lazy continuation
/// and must stay INSIDE the item rather than ending it. A code block or table
/// has no open paragraph, so it does NOT fold (the dedented line ends the item).
fn nested_ends_with_open_paragraph(nested: &str, options: &Options<'_>) -> bool {
    block_ends_with_open_paragraph(parse_blocks_with_options(nested, options).last())
}

fn block_ends_with_open_paragraph(block: Option<&BlockNode>) -> bool {
    match block {
        Some(BlockNode::Paragraph(_)) => true,
        // A blockquote has no explicit closer: lazy continuation keeps its
        // trailing paragraph open, so a dedented line folds into it.
        Some(BlockNode::BlockQuote(q)) => block_ends_with_open_paragraph(q.children.last()),
        // A list's last item can hold an open paragraph (the deepest open
        // paragraph a dedented line continues, e.g. a sub-list item's text).
        Some(BlockNode::List(l)) => {
            block_ends_with_open_paragraph(l.items.last().and_then(|it| it.children.last()))
        }
        // A div / admonition is closed by its `:::` fence -- a complete block
        // with no open paragraph -- so a dedented line after it ends the item
        // (like code/table). Matches carve-js.
        _ => false,
    }
}

fn collect_trailing_lazy(cur: &mut LineCursor, nested: &mut String) {
    while let Some(line) = cur.peek() {
        if line.trim().is_empty()
            || indent_columns(line) > 0
            || is_list_marker(line)
            || interrupts_paragraph(line, &cur.lines[cur.pos + 1..])
        {
            break;
        }
        nested.push('\n');
        nested.push_str(line.trim_start());
        cur.consume();
    }
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
    // A paragraph never carries its OWN trailing attribute block: a standalone
    // `{...}` line floats forward (handled via interrupts_paragraph + the
    // pending-attrs loop), and a trailing same-line `{...}` with no abutting
    // host stays literal inline content (§14). Paragraph attributes come only
    // from a preceding block-attribute line (§15), applied by the caller.
    // CommonMark / Djot: trailing whitespace at the very END of a paragraph's
    // final line is not significant and is stripped (`abc ` -> `<p>abc</p>`,
    // `# ` -> `<p>#</p>`). Only the paragraph's final trailing whitespace is
    // removed -- whitespace before a MID-paragraph newline is untouched, so a
    // two-space (`a  \nb`) or backslash (`a\<newline>b`) line break is
    // preserved. `trim_end` here acts on the joined buffer, i.e. only the end.
    let joined = lines.join("\n");
    let joined = joined.trim_end_matches([' ', '\t']);
    BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: parse_inline_with_options(joined, options),
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
    // A standalone block-attribute line floats forward to the next block (or is
    // dropped when none follows, §15), so it interrupts the paragraph rather
    // than folding in as literal text.
    if parse_standalone_attrs(line).is_some() {
        return true;
    }
    // Symmetric §10: a list marker (bullet OR task OR ordered) does NOT
    // interrupt a paragraph -- a list needs a blank line before it. Only the
    // other visible blocks interrupt.
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || line.starts_with('>')
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
    // A `::: |` line block interrupts like any colon-fence block, with the same
    // matching-closer lookahead.
    if let Some(len) = detect_line_block_open(line) {
        if rest.iter().any(|l| {
            let t = l.trim();
            !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= len
        }) {
            return true;
        }
    }
    false
}

/// A `- ` / `* ` bullet, including the attributed form `-{.c} ` (NOT `+`, the
/// continuation marker; not ordered).
///
/// Delegates to `detect_unordered` so an attributed bullet interrupts a
/// paragraph just like a plain one (and an attributed task already does via
/// `detect_task`). Leading tabs are skipped as well as spaces: a bullet opens
/// a list at any indentation (Rule B), so a tab-indented bullet interrupts a
/// paragraph too.
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
    Some(parse_caption_inline_with_options(text.trim_end(), options))
}

fn is_table_start(line: &str) -> bool {
    // A standard table row opens AND closes with `|` (grammar standard_row; a
    // `|=` cell is a header cell). A stray leading `|` with no closing `|`
    // (`| a`) is ordinary paragraph text, not a table. (`+` multi-line-cell
    // continuations are consumed inside parse_table; a `+` line never starts a
    // table, #80.)
    //
    // A row may also carry a `{...}` attribute block glued to its closing pipe
    // (`| a |{.x}` -> <tr class="x">); split_row_attrs validates it, so a line
    // ending in a valid row-attribute block also opens a table.
    let trimmed = line.trim();
    if trimmed.len() < 2 || !trimmed.starts_with('|') {
        return false;
    }
    trimmed.ends_with('|') || split_row_attrs(trimmed).0.is_some()
}

/// A `{...}` attribute block GLUED to the row's closing `|` sets the row's
/// `<tr>` attributes -- the row-level twin of a cell's opening-pipe block. The
/// whole payload must be a valid attribute block running to end of line;
/// otherwise the `{` is ordinary content. Returns the parsed attributes and the
/// line body up to and including the closing pipe (with the block removed).
fn split_row_attrs(content: &str) -> (Option<Attrs>, &str) {
    if let Some(idx) = content.rfind('|') {
        let bytes = content.as_bytes();
        if bytes.get(idx + 1) == Some(&b'{') {
            if let Some((attrs, next)) = read_attrs_at(bytes, idx + 1) {
                if next == content.len() {
                    return (Some(attrs), &content[..=idx]);
                }
            }
        }
    }
    (None, content)
}

fn parse_table(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let mut rows = Vec::new();
    // GFM-style header separator: a delimiter row directly after the first row
    // turns that row into a header and sets per-column alignment for the whole
    // column. The colons are read here and applied to every body row that
    // follows. The first row must not itself be a delimiter row.
    let mut first_is_delim = false;
    let mut column_aligns: Vec<Option<TableAlign>> = Vec::new();
    let mut saw_separator = false;
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
            continue;
        }
        if rows.is_empty() {
            first_is_delim = is_delim_row(line);
        } else if rows.len() == 1 && !saw_separator && !first_is_delim && is_delim_row(line) {
            // The separator row: make the first row the header, drop the row.
            saw_separator = true;
            column_aligns = parse_delim_aligns(line);
            for cell in &mut rows[0].cells {
                cell.header = true;
            }
            apply_column_aligns(&mut rows[0], &column_aligns);
            continue;
        }
        let mut row = parse_table_row(line, options);
        if saw_separator {
            apply_column_aligns(&mut row, &column_aligns);
        }
        rows.push(row);
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

/// A GFM delimiter cell: an optional leading colon, one or more dashes, an
/// optional trailing colon, and nothing else.
fn is_delim_cell(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b':' {
        i += 1;
    }
    let dash_start = i;
    while i < b.len() && b[i] == b'-' {
        i += 1;
    }
    if i == dash_start {
        return false; // need at least one dash
    }
    if i < b.len() && b[i] == b':' {
        i += 1;
    }
    i == b.len()
}

/// A delimiter row: every cell is a delimiter cell (and there is at least one).
fn is_delim_row(line: &str) -> bool {
    let mut content = line.trim();
    content = content.strip_prefix('|').unwrap_or(content);
    content = content.strip_suffix('|').unwrap_or(content);
    let cells = split_table_cells(content);
    !cells.is_empty() && cells.iter().all(|c| is_delim_cell(c.trim()))
}

/// Per-column alignment from a delimiter row's colons.
fn parse_delim_aligns(line: &str) -> Vec<Option<TableAlign>> {
    let mut content = line.trim();
    content = content.strip_prefix('|').unwrap_or(content);
    content = content.strip_suffix('|').unwrap_or(content);
    split_table_cells(content)
        .iter()
        .map(|c| {
            let t = c.trim();
            match (t.starts_with(':'), t.ends_with(':')) {
                (true, true) => Some(TableAlign::Center),
                (false, true) => Some(TableAlign::Right),
                (true, false) => Some(TableAlign::Left),
                (false, false) => None,
            }
        })
        .collect()
}

/// Apply a column default alignment to each cell that has no alignment of its
/// own (a native `|<` marker wins over the column default).
fn apply_column_aligns(row: &mut TableRow, aligns: &[Option<TableAlign>]) {
    for (i, cell) in row.cells.iter_mut().enumerate() {
        if cell.align.is_none() {
            if let Some(a) = aligns.get(i).copied().flatten() {
                cell.align = Some(a);
            }
        }
    }
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
    let (attrs, body) = split_row_attrs(content);
    content = body;
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
    TableRow { cells, attrs }
}

fn split_table_cells(content: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut buf = String::new();
    let mut code_ticks = 0usize;
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' {
            code_ticks ^= 1;
            buf.push(ch);
            continue;
        }
        if ch == '\\' {
            // Only an escaped PIPE is resolved here (so it does not split the
            // row); every other backslash escape is PRESERVED for the inline
            // parser to resolve. That keeps a leading `\{` literal rather than
            // looking like a cell attribute block. Matches carve-js.
            if chars.peek() == Some(&'|') {
                buf.push('|');
                chars.next();
            } else {
                buf.push('\\');
            }
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
    // A `{...}` attribute block GLUED to the opening pipe (no leading space)
    // sets the cell's attributes; the rest, after optional whitespace, is the
    // content. `read_attrs_at` is quote-aware and validates the whole payload,
    // so a partially-invalid or empty block reads as None and the `{` stays
    // content. A space before the brace (`| {.x}`) is also ordinary content.
    // An attributed cell is never a bare span marker -- its content is literal.
    if cell.as_bytes().first() == Some(&b'{') {
        if let Some((attrs, next)) = read_attrs_at(cell.as_bytes(), 0) {
            return TableCell {
                header: false,
                span: None,
                align: None,
                attrs: Some(attrs),
                children: parse_inline_with_options(cell[next..].trim(), options),
            };
        }
    }

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
        attrs: None,
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
    // STRICT (djot): the opener is the colon fence, an optional type word,
    // and an optional quoted title -- and NOTHING else. A trailing `{...}`
    // (or any other non-title text) makes the line an ordinary paragraph,
    // not a fence; attributes attach via a preceding block-attribute line.
    if rest.is_empty() {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            attrs: None,
        });
    }
    // A type word is a grammar identifier: `(letter | '_'), {letter | digit
    // | '_' | '-'}`. It must START with a letter or underscore, so a
    // digit-first token (`123`) or a non-identifier opener (`::: {.x}`,
    // `:::{k=v}`) is not a fence -- the line is an ordinary paragraph.
    if !rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let id_end = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    let kind = rest[..id_end].to_string();
    let after = rest[id_end..].trim();
    // After the type, only a quoted title may follow (with nothing after
    // it); anything else (a `{...}` block, unquoted text) is not a fence.
    let title = if after.is_empty() {
        None
    } else {
        let inner = after.strip_prefix('"')?;
        let close = inner.find('"')?;
        if !inner[close + 1..].trim().is_empty() {
            return None;
        }
        Some(inner[..close].to_string())
    };
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

/// A `::: |` line-block (verse) opener: a colon fence (3+) then a bare pipe and
/// nothing else (grammar PART 9 §23). Returns the fence length.
fn detect_line_block_open(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    // grammar: `line_block_open = colon_fence, space, "|"` -- a space (or tab)
    // between the fence and the pipe is REQUIRED, so `:::|` is not a line block.
    let after = &trimmed[fence_len..];
    let trimmed_after = after.trim_start_matches([' ', '\t']);
    if trimmed_after.len() == after.len() {
        return None; // no whitespace before the pipe
    }
    if trimmed_after.trim_end() == "|" {
        Some(fence_len)
    } else {
        None
    }
}

/// Count a line's leading whitespace in visual columns (tab = next 4-stop).
fn leading_ws_columns(line: &str) -> usize {
    let mut columns = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => columns += 1,
            '\t' => columns += 4 - (columns % 4),
            _ => break,
        }
    }
    columns
}

/// Remove up to `cols` columns of leading whitespace (tab-aware). When a tab
/// straddles the boundary its unconsumed columns are re-inserted as spaces, so
/// a verse line's relative indentation is preserved exactly (the residual-aware
/// dedent Carve uses on indentation it must keep).
fn strip_leading_columns(line: &str, cols: usize) -> String {
    let mut columns = 0usize;
    for (i, ch) in line.char_indices() {
        if columns >= cols {
            return line[i..].to_string();
        }
        match ch {
            ' ' => columns += 1,
            '\t' => {
                let next = columns + (4 - columns % 4);
                if next > cols {
                    // Tab crosses the reference column: keep the leftover columns.
                    return " ".repeat(next - cols) + &line[i + 1..];
                }
                columns = next;
            }
            _ => return line[i..].to_string(),
        }
    }
    String::new()
}

/// Expand a line's LEADING whitespace to non-breaking spaces (the U+00A0 the
/// renderers emit as `&nbsp;`) so a verse line's indentation survives; tabs
/// advance to the next 4-column stop. The rest of the line is left untouched.
fn expand_line_block_leading_ws(line: &str) -> String {
    let mut columns = 0usize;
    let mut idx = 0usize;
    for (i, ch) in line.char_indices() {
        match ch {
            ' ' => columns += 1,
            '\t' => columns += 4 - (columns % 4),
            _ => {
                idx = i;
                break;
            }
        }
        idx = i + ch.len_utf8();
    }
    format!("{}{}", "\u{00a0}".repeat(columns), &line[idx..])
}

/// Parse a `::: |` line block into a `<div class="line-block">`: each stanza
/// (blank-line-separated run) is a paragraph whose soft breaks become hard
/// breaks and whose per-line leading whitespace is preserved (grammar §23).
fn parse_line_block(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let opener = cur.peek().unwrap();
    let fence_len = detect_line_block_open(opener).unwrap();
    // Verse indentation is measured RELATIVE TO THE FENCE (grammar §23
    // REFERENCE COLUMN): strip the opener's own structural indent from each
    // body line before preserving the author's intra-verse whitespace.
    let base_indent = leading_ws_columns(opener);
    cur.consume();
    let mut stanzas: Vec<Vec<String>> = Vec::new();
    let mut stanza: Vec<String> = Vec::new();
    while let Some(line) = cur.peek() {
        let t = line.trim();
        if !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= fence_len {
            cur.consume();
            break;
        }
        cur.consume();
        if line.trim().is_empty() {
            if !stanza.is_empty() {
                stanzas.push(std::mem::take(&mut stanza));
            }
            continue;
        }
        let stripped = strip_leading_columns(line, base_indent);
        stanza.push(expand_line_block_leading_ws(&stripped));
    }
    if !stanza.is_empty() {
        stanzas.push(stanza);
    }

    let children = stanzas
        .into_iter()
        .map(|lines| {
            let inlines = parse_inline_with_options(&lines.join("\n"), options)
                .into_iter()
                .map(|n| match n {
                    InlineNode::SoftBreak => InlineNode::HardBreak,
                    other => other,
                })
                .collect();
            BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: inlines,
            })
        })
        .collect();

    // No inline opener attributes (strict djot); a preceding block-attribute
    // line merges onto this div in parse_blocks.
    BlockNode::Div(Div {
        attrs: Some(Attrs {
            id: None,
            classes: vec!["line-block".to_string()],
            key_values: BTreeMap::new(),
            order: vec![AttrSlot::Class],
        }),
        children,
    })
}

fn detect_abbreviation_def(line: &str) -> Option<AbbreviationDef> {
    let rest = line.strip_prefix("*[")?;
    let (abbr, expansion) = rest.split_once("]:")?;
    if abbr.is_empty() || !abbr.chars().all(char::is_alphanumeric) {
        return None;
    }
    Some(AbbreviationDef {
        abbr: abbr.to_string(),
        expansion: expansion.trim().to_string(),
    })
}

fn read_attrs_at(bytes: &[u8], start: usize) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        // An inline attribute block is single-line (grammar): a newline before
        // the closing `}` means this is not an inline attr -- the `{` stays
        // literal (`[x]{.a\n.b}` is text). Matches carve-js. Block-attribute
        // lines, which may span lines, are read by a separate path.
        if bytes[i] == b'\n' {
            return None;
        }
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

/// An attribute name (id, class, key) is a grammar identifier: it must start
/// with a letter or underscore (not a digit -- a `class="123"` / `id="1"` is
/// also invalid CSS). A name that fails this (including an empty one) makes
/// the whole block invalid, so it stays literal (§14). A digit after the
/// first character is fine. Stricter than djot (jgm/djot#399).
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_attrs(src: &str) -> Option<Attrs> {
    if src.trim().is_empty() {
        return None;
    }
    let mut attrs = Attrs::default();
    for token in attr_tokens(src) {
        if let Some(id) = token.strip_prefix('#') {
            if !is_identifier(id) {
                return None;
            }
            if attrs.id.is_none() {
                attrs.order.push(AttrSlot::Id);
            }
            attrs.id = Some(id.to_string());
        } else if let Some(class) = token.strip_prefix('.') {
            if !is_identifier(class) {
                return None;
            }
            if attrs.classes.is_empty() {
                attrs.order.push(AttrSlot::Class);
            }
            attrs.classes.push(class.to_string());
        } else if let Some((key, value)) = token.split_once('=') {
            if !is_identifier(key) {
                return None;
            }
            let value = value
                .trim_matches('"')
                .trim_matches('\'')
                .replace("\\\"", "\"")
                .replace("\\'", "'");
            if key == "id" {
                // `id=value` is the same attribute as `#id`: it feeds the id
                // slot, last-wins (§15), instead of emitting a second `id="…"`
                // (invalid HTML). `id` never enters key_values, so a bare `id`
                // boolean (below) cannot leave a stale duplicate. Matches
                // carve-php.
                if attrs.id.is_none() {
                    attrs.order.push(AttrSlot::Id);
                }
                attrs.id = Some(value);
            } else {
                if !attrs.key_values.contains_key(key) {
                    attrs.order.push(AttrSlot::Key(key.to_string()));
                }
                attrs.key_values.insert(key.to_string(), value);
            }
        } else if is_identifier(&token) {
            if token == "id" {
                // A bare boolean `id` also feeds the id slot (value ""), last-wins
                // and single -- `{id id=j}` -> `id="j"`, `{id}` -> `id=""`.
                if attrs.id.is_none() {
                    attrs.order.push(AttrSlot::Id);
                }
                attrs.id = Some(String::new());
            } else {
                // Boolean attribute: a bare word with no value, rendered name="".
                // (Matched last so `k=v` is a key/value, not a bare `k`.)
                if !attrs.key_values.contains_key(&token) {
                    attrs.order.push(AttrSlot::Key(token.clone()));
                }
                attrs.key_values.insert(token, String::new());
            }
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

/// Merge a leading block-attribute line onto a node that may already carry
/// its own opener attributes. Leading attrs are earlier in source, so their
/// classes precede the opener's and the opener wins on id/key conflict (§15).
fn merge_leading_attrs(target: &mut Option<Attrs>, leading: Attrs) {
    match target.take() {
        None => *target = Some(leading),
        Some(own) => {
            *target = Some(leading);
            merge_attrs(target, own);
        }
    }
}

fn apply_attrs_to_block(node: &mut BlockNode, attrs: Attrs) {
    match node {
        BlockNode::Heading(n) => n.attrs = Some(attrs),
        BlockNode::Paragraph(n) => n.attrs = Some(attrs),
        BlockNode::ThematicBreak(n) => n.attrs = Some(attrs),
        BlockNode::CodeBlock(n) => n.attrs = Some(attrs),
        BlockNode::List(n) => n.attrs = Some(attrs),
        BlockNode::BlockQuote(n) => n.attrs = Some(attrs),
        BlockNode::Table(n) => n.attrs = Some(attrs),
        // A typed colon-fence opener may already carry its own attribute
        // block (`::: note {.x}`); a leading block-attribute line is earlier
        // in source, so its classes come first and the opener's win on
        // id/key conflict (§15) -- merge instead of clobbering.
        BlockNode::Admonition(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::Div(n) => merge_leading_attrs(&mut n.attrs, attrs),
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

/// Merge an attribute block onto an inline node, accumulating classes (§15)
/// instead of overwriting -- used for chained blocks (`[x]{.a}{.b}`).
fn merge_attrs_into_inline(node: &mut InlineNode, attrs: Attrs) {
    match node {
        InlineNode::Emphasis(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Link(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Image(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Span(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Math(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::AutoLink(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Extension(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Code(_, a) => merge_attrs(a, attrs),
        InlineNode::Footnote(n) => merge_attrs(&mut n.attrs, attrs),
        _ => {}
    }
}

/// Whether an inline node can carry an attribute block (so a following `{...}`
/// attaches rather than staying literal). Text/raw nodes cannot.
fn inline_is_attributable(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Emphasis(_)
            | InlineNode::Link(_)
            | InlineNode::Image(_)
            | InlineNode::Span(_)
            | InlineNode::Math(_)
            | InlineNode::AutoLink(_)
            | InlineNode::Extension(_)
            | InlineNode::Code(_, _)
            | InlineNode::Footnote(_)
    )
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
    parse_inline_context(text, options, false, false)
}

fn parse_caption_inline_with_options(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    parse_inline_context(text, options, true, false)
}

fn parse_inline_context(
    text: &str,
    options: &Options<'_>,
    mut caption_number_allowed: bool,
    in_footnote: bool,
) -> Vec<InlineNode> {
    // Recursion cap (see MAX_NESTING_DEPTH). Nested links/spans/emphasis recurse
    // through here one frame per level; over the cap, keep the remaining text
    // literal rather than recursing further (prevents a stack-overflow abort on
    // input like `[[[[[…x]]]]]`). Shares the depth counter with block parsing.
    let Some(_depth) = DepthGuard::enter() else {
        return vec![InlineNode::Text(text.to_string())];
    };
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
                if caption_number_allowed && nxt == b'#' {
                    buf.push('#');
                } else {
                    buf.push('\\');
                    buf.push(nxt as char);
                }
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
            if let Some((critic, consumed)) = parse_critic_markup(bytes, i, options, in_footnote) {
                flush_text(&mut out, &mut buf);
                out.push(critic);
                i += consumed;
                continue;
            }
            // Forced intraword emphasis `{X…X}` — tried before inline attribute
            // blocks, matching the reference scan order.
            if let Some((mut node, consumed)) =
                parse_forced_emphasis(bytes, i, options, in_footnote)
            {
                let mut consumed = consumed;
                // A trailing `{...}` attribute block attaches to the forced span,
                // exactly like a bare span (`{*x*}{.c}` -> <strong class="c">x</strong>).
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
            // A standalone attribute block merges into the immediately preceding
            // inline node, so adjacent blocks chain (`[x]{.a}{.b}`,
            // `*x*{.a}{.b}` -> merged classes, §15). It must be GLUED: a
            // non-empty `buf` means text (e.g. a space) sits between the node
            // and the `{`, so the block stays literal. An empty/invalid `{...}`
            // also stays literal. Matches carve-php / carve-js.
            if buf.is_empty() && out.last().is_some_and(inline_is_attributable) {
                if let Some((attrs, next)) = read_attrs_at(bytes, i) {
                    let last = out.last_mut().unwrap();
                    // A reference link carries `raw_ref` (its literal source) in
                    // case it stays unresolved. Merge the block into the link's
                    // attrs (used when it resolves) AND append the block's
                    // literal text to `raw_ref` (used when it reverts), so a
                    // resolved `[t][r]{.a}{.b}` gets class="a b" while an
                    // unresolved `[t][missing]{.a}{.b}` keeps both blocks literal.
                    if let InlineNode::Link(l) = last {
                        if let Some(raw) = l.raw_ref.as_mut() {
                            if let Ok(lit) = std::str::from_utf8(&bytes[i..next]) {
                                raw.push_str(lit);
                            }
                        }
                    }
                    merge_attrs_into_inline(last, attrs);
                    i = next;
                    continue;
                }
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
            if !in_footnote {
                if let Some((footnote, consumed)) = parse_footnote_ref(bytes, i) {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Footnote(footnote));
                    i += consumed;
                    continue;
                }
            }
            if has_link_trigger {
                if let Some((link, consumed)) =
                    parse_inline_link_with_options(bytes, i, options, in_footnote)
                {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Link(link));
                    i += consumed;
                    continue;
                }
                if let Some((link, consumed)) = parse_reference_link(bytes, i, options, in_footnote)
                {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Link(link));
                    i += consumed;
                    continue;
                }
                if let Some((span, consumed)) = parse_span(bytes, i, options, in_footnote) {
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
            if caption_number_allowed && !bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::CaptionNumber(CaptionNumber { number: None }));
                caption_number_allowed = false;
                i += 1;
                continue;
            }
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
            if let Some((node, consumed)) = parse_inline_extension(bytes, i, options, in_footnote) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Extension(node));
                i += consumed;
                continue;
            }
        }

        // Inline footnote `^[content]`, ranked above superscript.
        if !in_footnote
            && c == b'^'
            && bytes.get(i + 1) == Some(&b'[')
            && (i == 0 || bytes[i - 1] != b'^')
        {
            if let Some((footnote, consumed)) = parse_inline_footnote(bytes, i, options) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Footnote(footnote));
                i += consumed;
                continue;
            }
        }

        // Bold-italic, sub, highlight, then single-char emphasis
        if let Some((mut node, consumed)) = match_emphasis(bytes, i, options, in_footnote) {
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
            // `end` must land on a char boundary or `text[i..]`/slicing panics;
            // a misbehaving extension matcher must not be able to crash the core.
            if end > i && end <= text.len() && text.is_char_boundary(end) {
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
    in_footnote: bool,
) -> Option<(InlineNode, usize)> {
    let rest = std::str::from_utf8(&bytes[start..]).ok()?;
    if let Some(inner) = rest.strip_prefix("{+") {
        let end = inner.find("+}")?;
        return Some((
            InlineNode::CriticInsert(CriticInsert {
                children: parse_inline_context(&inner[..end], options, false, in_footnote),
            }),
            end + 4,
        ));
    }
    if let Some(inner) = rest.strip_prefix("{-") {
        let end = inner.find("-}")?;
        return Some((
            InlineNode::CriticDelete(CriticDelete {
                children: parse_inline_context(&inner[..end], options, false, in_footnote),
            }),
            end + 4,
        ));
    }
    if let Some(inner) = rest.strip_prefix("{~") {
        // A critic substitution is `{~old~>new~}`: the `~>` separator must sit
        // within this `{~ … ~}`. Without it (`{~view~}`), this is not critic
        // markup -- it falls through to forced strike emphasis.
        let end = inner.find("~}")?;
        let sep = inner[..end].find("~>")?;
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
    let mut attrs = None;
    let mut after = i + 1;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Footnote {
            attrs,
            id: Some(id),
            inline: None,
            number: None,
            ref_id: None,
        },
        after - start,
    ))
}

fn parse_inline_footnote(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'^') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let (content, after_bracket) = read_bracketed(bytes, start + 1)?;
    if content.trim().is_empty() {
        return None;
    }
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    let children = parse_inline_context(&content, options, false, true);
    Some((
        Footnote {
            attrs,
            id: None,
            inline: Some(children),
            number: None,
            ref_id: None,
        },
        after - start,
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
    let end = tick + 1 + close + 1;
    // A trailing attribute block attaches to the math span (math reuses the
    // code-span attribute slot), EXCEPT `{=format}`, the raw-inline form,
    // which is code-span-only and not inherited by math -- leave it literal.
    let mut attrs = None;
    let mut after = end;
    if bytes.get(end) == Some(&b'{') && bytes.get(end + 1) != Some(&b'=') {
        if let Some((parsed, next)) = read_attrs_at(bytes, end) {
            attrs = Some(parsed);
            after = next;
        }
    }
    Some((
        Math {
            attrs,
            display,
            content: rest[..close].to_string(),
        },
        after - start,
    ))
}

fn parse_reference_link(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
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
    // A trailing attribute block attaches to the resolved link, the same
    // slot an inline link uses (`[t][x]{.c}`).
    let mut attrs = None;
    let mut after = after_label;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Link {
            attrs,
            href: String::new(),
            title: None,
            children: parse_inline_context(&text, options, false, in_footnote),
            ref_label: Some(ref_label),
            // `raw_ref` is the literal source emitted only when the
            // reference does not resolve; it must include the consumed
            // attribute block so an unresolved `[t][x]{.c}` stays fully
            // literal rather than silently dropping the `{.c}`. A resolved
            // reference ignores `raw_ref` and applies `attrs` instead.
            raw_ref: Some(std::str::from_utf8(&bytes[start..after]).ok()?.to_string()),
        },
        after - start,
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
    in_footnote: bool,
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
            children: parse_inline_context(&text, options, false, in_footnote),
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
    in_footnote: bool,
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
            children: parse_inline_context(&content, options, false, in_footnote),
        },
        after - start,
    ))
}

fn parse_span(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
) -> Option<(Span, usize)> {
    let (content, after_bracket) = read_bracketed(bytes, start)?;
    if bytes.get(after_bracket) != Some(&b'{') {
        return None;
    }
    let (attrs, after_attrs) = read_attrs_at(bytes, after_bracket)
        .or_else(|| read_empty_attrs_at(bytes, after_bracket))?;
    // Absorb a CHAIN of adjacent attribute blocks (`[x]{.a}{.b}` ->
    // class="a b"), accumulating classes (§15). A non-attribute `{...}` (e.g.
    // an empty `{}`) reads as None and is left literal, so `[x]{}{}` keeps the
    // trailing `{}` -- matching carve-php / carve-js.
    let mut attrs = Some(attrs);
    let mut after_attrs = after_attrs;
    while bytes.get(after_attrs) == Some(&b'{') {
        match read_attrs_at(bytes, after_attrs) {
            Some((more, next)) => {
                merge_attrs(&mut attrs, more);
                after_attrs = next;
            }
            None => break,
        }
    }
    Some((
        Span {
            attrs,
            children: parse_inline_context(&content, options, false, in_footnote),
        },
        after_attrs - start,
    ))
}

fn read_empty_attrs_at(bytes: &[u8], start: usize) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    // Only space/tab between the braces -- a newline means it is not a
    // single-line inline attribute block, so `[x]{\n}` stays literal (matching
    // the read_attrs_at newline-bail and carve-js).
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if bytes.get(i) == Some(&b'}') {
        Some((Attrs::default(), i + 1))
    } else {
        None
    }
}

/// Length of a mention/tag name = name_word ('.' name_word)*, where
/// name_word = (letter | digit | '_' | '-')+ (grammar PART 9 §7). A `.` is
/// INTERIOR only -- it must sit between two name_words, so `a..b` yields `a`
/// (the run stops before the doubled dot) and `markus.` yields `markus`.
fn name_run_len(s: &str) -> usize {
    let b = s.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    let mut i = 0;
    while i < b.len() && is_word(b[i]) {
        i += 1;
    }
    if i == 0 {
        return 0;
    }
    while b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(|&c| is_word(c)) {
        i += 1; // the interior dot
        while i < b.len() && is_word(b[i]) {
            i += 1;
        }
    }
    i
}

fn parse_mention(text: &str, pos: usize) -> Option<(Mention, usize)> {
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let rest = text.get(pos + 1..)?;
    let len = name_run_len(rest);
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
    let len = name_run_len(rest);
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
    if is_url_autolink_target(target) {
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

fn is_url_autolink_target(target: &str) -> bool {
    let Some((scheme, url)) = target.split_once(':') else {
        return false;
    };
    let Some(first) = scheme.bytes().next() else {
        return false;
    };
    if url.is_empty() || !first.is_ascii_alphabetic() {
        return false;
    }
    scheme
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        && url.bytes().all(is_url_autolink_char)
}

fn is_url_autolink_char(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b'%'
        )
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
            b'`' => {
                // An unclosed verbatim span is opaque to end of text, so no `]`
                // after it can close the bracket: the construct is not balanced.
                i = skip_code_span(bytes, i)?;
            }
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

/// Skip a verbatim (code) span opening at `start` (a backtick run). Returns the
/// index just past the equal-length closing run, or `None` when the span is
/// unclosed (opaque to end of text) — mirroring the reference bracket scanner.
fn skip_code_span(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let close_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if i - close_start == open_len {
            return Some(i);
        }
    }
    None
}

/// Read `href[ "title"])` starting at `start` (just past the opening `(`).
/// Returns (href, optional title, index just past the closing `)`).
/// Resolve backslash escapes in a link/image title: `\X` becomes `X` when X is
/// ASCII punctuation (so `\"` is a literal quote), otherwise the backslash is
/// kept. Mirrors carve-js's unescapeAttrValue.
fn unescape_title(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_punctuation() {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

fn read_link_target(bytes: &[u8], start: usize) -> Option<(String, Option<String>, usize)> {
    let mut i = start;
    let href_start = i;
    // Per the grammar, an inline link destination ends at the first whitespace
    // or first `)` (no balanced-paren or escape rule). A `)` that needs to live
    // in a URL comes via a reference definition; the markdown renderer
    // percent-encodes it on the way out.
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
        // A backslash escapes the next byte, so `\"` is a literal quote inside
        // the title rather than its terminator (matches carve-php / carve-js).
        while i < bytes.len() && bytes[i] != quote {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
            } else {
                i += 1;
            }
        }
        if i >= bytes.len() {
            return None;
        }
        title = Some(unescape_title(
            std::str::from_utf8(&bytes[title_start..i]).ok()?,
        ));
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

fn match_emphasis(
    bytes: &[u8],
    i: usize,
    options: &Options<'_>,
    in_footnote: bool,
) -> Option<(InlineNode, usize)> {
    let c = bytes[i];

    // /*bold italic*/
    if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
        if let Some(close) = find_seq(bytes, i + 2, b"*/") {
            let inner = std::str::from_utf8(&bytes[i + 2..close]).ok()?;
            return Some((
                InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind: EmphasisKind::BoldItalic,
                    children: parse_inline_context(inner, options, false, in_footnote),
                }),
                close + 2 - i,
            ));
        }
    }
    // Single-char delimiters. Highlight `=` and subscript `,` are single-char
    // like the rest; a doubled `==`/`,,` is therefore literal by same-delimiter
    // adjacency (checked below), exactly like `**x**`.
    let kind = match c {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'~' => EmphasisKind::Strike,
        b'^' => EmphasisKind::Super,
        b'=' => EmphasisKind::Highlight,
        b',' => EmphasisKind::Sub,
        _ => return None,
    };
    let delim = c;
    // Opener: next char must exist and not be space/newline/delim
    let after = bytes.get(i + 1).copied()?;
    if after == b' ' || after == b'\n' || after == delim {
        return None;
    }
    // A `=` that is part of a multi-char smart-typography operator is consumed
    // by that operator, not as a highlight opener (grammar PART 8 / §8): it
    // begins `=>` or trails `<=` / `>=` / `!=`. (The spaced forms like `x <= y`
    // already fail the opener test -- their `=` is followed by whitespace -- but
    // compact forms like `a <=b` would otherwise open a stray `<mark>`.)
    if delim == b'=' {
        if after == b'>' {
            return None;
        }
        if i > 0 && matches!(bytes[i - 1], b'<' | b'>' | b'!') {
            return None;
        }
    }
    if i > 0 {
        let prev = bytes[i - 1];
        // No same-type nesting: a delimiter adjacent to the same delimiter does
        // not open, so a doubled delimiter (`**x**`, `==x==`, `,,x,,`) is literal.
        if prev == delim {
            return None;
        }
        // Word-boundary opener (spec §9): no bare delimiter opens after an
        // alphanumeric or `_`, keeping paths/identifiers/numbers literal
        // (`a/b/c`, `foo*bar*baz`, `snake_case`, `x = 5`, `key=value`, `1,2,3`).
        // Use the forced `{X…X}` family for deliberate intraword emphasis.
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
        // Italic/underline additionally can't open after `/` (path protection,
        // e.g. `snake_/case/`).
        if (delim == b'/' || delim == b'_') && prev == b'/' {
            return None;
        }
    }
    let close = find_emphasis_close(bytes, i + 1, delim)?;
    let inner = std::str::from_utf8(&bytes[i + 1..close]).ok()?;
    Some((
        InlineNode::Emphasis(Emphasis {
            attrs: None,
            kind,
            children: parse_inline_context(inner, options, false, in_footnote),
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
    let index = abbreviation_index(&defs);
    for block in &mut doc.children {
        apply_abbreviations_block(block, &index);
    }
}

type AbbreviationIndex<'a> = BTreeMap<char, Vec<(&'a str, &'a str)>>;

fn abbreviation_index(defs: &BTreeMap<String, String>) -> AbbreviationIndex<'_> {
    let mut index: AbbreviationIndex<'_> = BTreeMap::new();
    for (abbr, expansion) in defs {
        if let Some(first) = abbr.chars().next() {
            index
                .entry(first)
                .or_default()
                .push((abbr.as_str(), expansion.as_str()));
        }
    }
    index
}

fn apply_abbreviations_block(block: &mut BlockNode, index: &AbbreviationIndex<'_>) {
    match block {
        BlockNode::Heading(h) => apply_abbreviations_inline(&mut h.children, index),
        BlockNode::Paragraph(p) => apply_abbreviations_inline(&mut p.children, index),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    apply_abbreviations_block(child, index);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    apply_abbreviations_inline(&mut cell.children, index);
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::Figure(f) => {
            apply_abbreviations_inline(&mut f.caption, index);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        apply_abbreviations_block(child, index);
                    }
                }
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            apply_abbreviations_inline(&mut cell.children, index);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            }
        }
        _ => {}
    }
}

fn apply_abbreviations_inline(nodes: &mut Vec<InlineNode>, index: &AbbreviationIndex<'_>) {
    let mut out = Vec::new();
    for node in std::mem::take(nodes) {
        match node {
            InlineNode::Text(text) => {
                let mut parts = replace_abbreviations_in_text(&text, index);
                out.append(&mut parts);
            }
            InlineNode::Emphasis(mut e) => {
                apply_abbreviations_inline(&mut e.children, index);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Link(mut l) => {
                apply_abbreviations_inline(&mut l.children, index);
                out.push(InlineNode::Link(l));
            }
            InlineNode::Extension(mut e) => {
                apply_abbreviations_inline(&mut e.children, index);
                out.push(InlineNode::Extension(e));
            }
            InlineNode::CitationGroup(mut g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        apply_abbreviations_inline(prefix, index);
                    }
                    if let Some(locator) = &mut item.locator {
                        apply_abbreviations_inline(locator, index);
                    }
                }
                out.push(InlineNode::CitationGroup(g));
            }
            other => out.push(other),
        }
    }
    *nodes = out;
}

fn replace_abbreviations_in_text(text: &str, index: &AbbreviationIndex<'_>) -> Vec<InlineNode> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let mut matched: Option<(&str, &str)> = None;
        let ch = text[i..].chars().next().unwrap();
        if let Some(candidates) = index.get(&ch) {
            for (abbr, expansion) in candidates {
                if text[i..].starts_with(abbr)
                    && is_word_boundary(text, i)
                    && is_word_boundary(text, i + abbr.len())
                {
                    matched = Some((*abbr, *expansion));
                    break;
                }
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

fn resolve_crossrefs(doc: &mut Document, lowercase_ids: bool) {
    let mut counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    collect_heading_titles(&doc.children, &mut counts, &mut titles, lowercase_ids);
    let mut caption_counts = BTreeMap::new();
    number_captioned_blocks(&mut doc.children, &mut caption_counts, &mut titles);
    // Case-folded index of known ids -> actual (case-preserved) id. First
    // occurrence wins, so a duplicate that only differs in case does not shadow
    // the earlier heading. Used as a fallback when an exact id match fails, so a
    // lowercase `</#getting-started>` resolves to a `Getting-Started` heading
    // (and the emitted href uses the ACTUAL id).
    let mut folded: BTreeMap<String, String> = BTreeMap::new();
    for id in titles.keys() {
        folded.entry(case_fold(id)).or_insert_with(|| id.clone());
    }
    let index = CrossrefIndex { titles, folded };
    for block in &mut doc.children {
        resolve_crossrefs_block(block, &index);
    }
}

/// Heading-id lookup table for `</#id>` cross-references: exact id -> title,
/// plus a case-folded fallback (folded id -> actual case-preserved id) so a
/// lowercase reference resolves to a case-preserved heading.
struct CrossrefIndex {
    titles: BTreeMap<String, String>,
    folded: BTreeMap<String, String>,
}

impl CrossrefIndex {
    /// Resolve a cross-reference target to its `(actual_id, title)`. Tries an
    /// exact match first, then a case-folded fallback (first-occurrence wins).
    fn resolve(&self, target: &str) -> Option<(&str, &str)> {
        if let Some((id, title)) = self.titles.get_key_value(target) {
            return Some((id.as_str(), title.as_str()));
        }
        let id = self.folded.get(&case_fold(target))?;
        let title = self.titles.get(id)?;
        Some((id.as_str(), title.as_str()))
    }
}

/// Per-code-point lowercase fold, used for case-insensitive `</#id>` lookup.
/// Matches the `lowercase` transform in `slugify_parse` (no context mappings).
fn case_fold(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        for lc in ch.to_lowercase() {
            out.push(lc);
        }
    }
    out
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
            if let Some(caption) = &mut t.caption {
                resolve_reference_links_inline(caption, defs);
            }
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
        BlockNode::Div(d) => {
            for child in &mut d.children {
                resolve_reference_links_block(child, defs);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    resolve_reference_links_inline(term, defs);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        resolve_reference_links_block(child, defs);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            resolve_reference_links_inline(&mut f.caption, defs);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        resolve_reference_links_block(child, defs);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        resolve_reference_links_inline(caption, defs);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            resolve_reference_links_inline(&mut cell.children, defs);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
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
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        resolve_reference_links_inline(prefix, defs);
                    }
                    if let Some(locator) = &mut item.locator {
                        resolve_reference_links_inline(locator, defs);
                    }
                }
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
    lowercase_ids: bool,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                let title = plain_inlines_parse(&h.children);
                let base = h
                    .attrs
                    .as_ref()
                    .and_then(|attrs| attrs.id.clone())
                    .unwrap_or_else(|| slugify_parse(&title, lowercase_ids));
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
                    collect_heading_titles(&item.children, counts, titles, lowercase_ids);
                }
            }
            BlockNode::BlockQuote(b) => {
                collect_heading_titles(&b.children, counts, titles, lowercase_ids)
            }
            BlockNode::Admonition(a) => {
                collect_heading_titles(&a.children, counts, titles, lowercase_ids)
            }
            BlockNode::Div(d) => collect_heading_titles(&d.children, counts, titles, lowercase_ids),
            BlockNode::DefinitionList(d) => {
                for item in &d.items {
                    for definition in &item.definitions {
                        collect_heading_titles(definition, counts, titles, lowercase_ids);
                    }
                }
            }
            BlockNode::Figure(f) => match &f.target {
                FigureTarget::BlockQuote(b) => {
                    collect_heading_titles(&b.children, counts, titles, lowercase_ids)
                }
                FigureTarget::Table(_)
                | FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            },
            _ => {}
        }
    }
}

fn number_captioned_blocks(
    blocks: &mut [BlockNode],
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    for block in blocks {
        match block {
            BlockNode::Table(t) => number_table_caption(t, counts, titles),
            BlockNode::Figure(f) => {
                number_caption(&mut f.caption, f.attrs.as_ref(), counts, titles);
                match &mut f.target {
                    FigureTarget::BlockQuote(b) => {
                        number_captioned_blocks(&mut b.children, counts, titles);
                    }
                    FigureTarget::Table(t) => number_table_caption(t, counts, titles),
                    FigureTarget::Image(_)
                    | FigureTarget::CodeBlock(_)
                    | FigureTarget::Paragraph(_) => {}
                }
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    number_captioned_blocks(&mut item.children, counts, titles);
                }
            }
            BlockNode::BlockQuote(b) => number_captioned_blocks(&mut b.children, counts, titles),
            BlockNode::Admonition(a) => number_captioned_blocks(&mut a.children, counts, titles),
            BlockNode::Div(d) => number_captioned_blocks(&mut d.children, counts, titles),
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for definition in &mut item.definitions {
                        number_captioned_blocks(definition, counts, titles);
                    }
                }
            }
            _ => {}
        }
    }
}

fn number_table_caption(
    table: &mut Table,
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    if let Some(caption) = &mut table.caption {
        number_caption(caption, table.attrs.as_ref(), counts, titles);
    }
}

fn number_caption(
    caption: &mut [InlineNode],
    attrs: Option<&Attrs>,
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    let Some(idx) = caption
        .iter()
        .position(|node| matches!(node, InlineNode::CaptionNumber(_)))
    else {
        return;
    };
    let label = plain_inlines_parse(&caption[..idx])
        .trim_end_matches(char::is_whitespace)
        .to_string();
    let next = counts.entry(label.clone()).or_insert(0);
    *next += 1;
    let number = *next;
    if let InlineNode::CaptionNumber(caption_number) = &mut caption[idx] {
        caption_number.number = Some(number);
    }
    if let Some(id) = attrs.and_then(|attrs| attrs.id.as_ref()) {
        titles
            .entry(id.clone())
            .or_insert_with(|| format!("{label} {number}"));
    }
}

fn resolve_crossrefs_block(block: &mut BlockNode, index: &CrossrefIndex) {
    match block {
        BlockNode::Heading(h) => resolve_crossrefs_inline(&mut h.children, index),
        BlockNode::Paragraph(p) => resolve_crossrefs_inline(&mut p.children, index),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    resolve_crossrefs_block(child, index);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                resolve_crossrefs_block(child, index);
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                resolve_crossrefs_block(child, index);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                resolve_crossrefs_block(child, index);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    resolve_crossrefs_inline(term, index);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        resolve_crossrefs_block(child, index);
                    }
                }
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                resolve_crossrefs_inline(caption, index);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    resolve_crossrefs_inline(&mut cell.children, index);
                }
            }
        }
        BlockNode::Figure(f) => {
            resolve_crossrefs_inline(&mut f.caption, index);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        resolve_crossrefs_block(child, index);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        resolve_crossrefs_inline(caption, index);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            resolve_crossrefs_inline(&mut cell.children, index);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            }
        }
        _ => {}
    }
}

fn resolve_crossrefs_inline(nodes: &mut Vec<InlineNode>, index: &CrossrefIndex) {
    for node in nodes {
        match node {
            InlineNode::CrossRef(c) => {
                if let Some((actual_id, title)) = index.resolve(&c.target) {
                    // The href uses the ACTUAL (case-preserved) heading id, even
                    // when the reference matched only via the case-fold fallback.
                    *node = InlineNode::Link(Link {
                        attrs: None,
                        href: format!("#{actual_id}"),
                        title: None,
                        children: vec![InlineNode::Text(title.to_string())],
                        ref_label: None,
                        raw_ref: None,
                    });
                } else {
                    // Unknown heading id: the cross-reference stays literal text.
                    *node = InlineNode::Text(format!("</#{}>", c.target));
                }
            }
            InlineNode::Emphasis(e) => resolve_crossrefs_inline(&mut e.children, index),
            InlineNode::Link(l) => resolve_crossrefs_inline(&mut l.children, index),
            InlineNode::Span(s) => resolve_crossrefs_inline(&mut s.children, index),
            InlineNode::Extension(e) => resolve_crossrefs_inline(&mut e.children, index),
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        resolve_crossrefs_inline(prefix, index);
                    }
                    if let Some(locator) = &mut item.locator {
                        resolve_crossrefs_inline(locator, index);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Enforce "links never nest" (CommonMark: a link may not contain another
/// link). This is a single post-resolution pass: it runs AFTER reference-link
/// and cross-reference resolution because both turn into `Link` nodes only at
/// that stage, so a `</#id>` cross-reference or a resolved reference inside a
/// link's text would otherwise survive as a nested anchor. A link found inside
/// another link is unwrapped to its (recursively cleaned) text, so only the
/// outermost destination applies; an autolink inside a link becomes plain text
/// (the display form the renderer would emit, with a leading `mailto:` scheme
/// stripped). A footnote body renders in the endnotes section, outside any
/// anchor, so its links are not nested -- the walk re-enters a footnote body
/// with `inside_link = false`.
fn enforce_no_nesting(doc: &mut Document) {
    for block in &mut doc.children {
        enforce_no_nesting_block(block);
    }
    for body in doc.footnote_defs.values_mut() {
        for block in body {
            enforce_no_nesting_block(block);
        }
    }
}

fn enforce_no_nesting_block(block: &mut BlockNode) {
    match block {
        BlockNode::Heading(h) => apply_no_nesting(&mut h.children),
        BlockNode::Paragraph(p) => apply_no_nesting(&mut p.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    enforce_no_nesting_block(child);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            if let Some(attribution) = &mut b.attribution {
                apply_no_nesting(attribution);
            }
            for child in &mut b.children {
                enforce_no_nesting_block(child);
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                apply_no_nesting(title);
            }
            for child in &mut a.children {
                enforce_no_nesting_block(child);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                enforce_no_nesting_block(child);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    apply_no_nesting(term);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        enforce_no_nesting_block(child);
                    }
                }
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                apply_no_nesting(caption);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    apply_no_nesting(&mut cell.children);
                }
            }
        }
        BlockNode::Figure(f) => {
            apply_no_nesting(&mut f.caption);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    if let Some(attribution) = &mut b.attribution {
                        apply_no_nesting(attribution);
                    }
                    for child in &mut b.children {
                        enforce_no_nesting_block(child);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        apply_no_nesting(caption);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            apply_no_nesting(&mut cell.children);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            }
        }
        _ => {}
    }
}

fn apply_no_nesting(nodes: &mut Vec<InlineNode>) {
    let taken = std::mem::take(nodes);
    *nodes = enforce_no_nesting_inline(taken, false);
}

fn enforce_no_nesting_inline(nodes: Vec<InlineNode>, inside_link: bool) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            InlineNode::Link(mut link) => {
                let children = enforce_no_nesting_inline(link.children, true);
                if inside_link {
                    // A nested link is dropped; only its (cleaned) text remains
                    // because the outermost destination already applies.
                    out.extend(children);
                } else {
                    link.children = children;
                    out.push(InlineNode::Link(link));
                }
            }
            InlineNode::AutoLink(a) => {
                if inside_link {
                    let display = a
                        .href
                        .strip_prefix("mailto:")
                        .unwrap_or(&a.href)
                        .to_string();
                    out.push(InlineNode::Text(display));
                } else {
                    out.push(InlineNode::AutoLink(a));
                }
            }
            InlineNode::Footnote(mut f) => {
                // A footnote body renders outside the anchor, so its links are
                // not nested: re-enter with inside_link = false.
                if let Some(inline) = f.inline.take() {
                    f.inline = Some(enforce_no_nesting_inline(inline, false));
                }
                out.push(InlineNode::Footnote(f));
            }
            InlineNode::Emphasis(mut e) => {
                e.children = enforce_no_nesting_inline(e.children, inside_link);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Span(mut s) => {
                s.children = enforce_no_nesting_inline(s.children, inside_link);
                out.push(InlineNode::Span(s));
            }
            InlineNode::Extension(mut ext) => {
                ext.children = enforce_no_nesting_inline(ext.children, inside_link);
                out.push(InlineNode::Extension(ext));
            }
            InlineNode::CriticInsert(mut c) => {
                c.children = enforce_no_nesting_inline(c.children, inside_link);
                out.push(InlineNode::CriticInsert(c));
            }
            InlineNode::CriticDelete(mut c) => {
                c.children = enforce_no_nesting_inline(c.children, inside_link);
                out.push(InlineNode::CriticDelete(c));
            }
            other => out.push(other),
        }
    }
    out
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
            InlineNode::CitationGroup(g) => out.push_str(&g.raw),
            InlineNode::Abbreviation(a) => out.push_str(&a.abbr),
            InlineNode::Mention(m) => out.push_str(&m.user),
            InlineNode::Tag(t) => out.push_str(&t.name),
            InlineNode::CaptionNumber(n) => {
                if let Some(number) = n.number {
                    out.push_str(&number.to_string());
                }
            }
            // A soft/hard break (multi-line heading) is a word separator, so
            // parse-time cross-reference slugs match the rendered heading id.
            InlineNode::SoftBreak | InlineNode::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

/// Carve "Automatic Identifiers" slug (spec #73). The single canonical
/// implementation, shared by the HTML and Markdown renderers so all id
/// derivation in carve-rs stays byte-identical (and identical to carve-js /
/// carve-php).
/// Reverse smart-typography substitutions to their ASCII source, so a heading
/// id never depends on presentational typography. The inverse of the parser's
/// smart tokens plus smart quotes and dashes; the recovered ASCII punctuation
/// then collapses in the slug run. Kept byte-identical to carve-js / carve-php.
fn de_typography(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '↔' => out.push_str("<->"),
            '™' => out.push_str("(tm)"),
            '…' => out.push_str("..."),
            '→' => out.push_str("->"),
            '←' => out.push_str("<-"),
            '⇒' => out.push_str("=>"),
            '≤' => out.push_str("<="),
            '≥' => out.push_str(">="),
            '≠' => out.push_str("!="),
            '±' => out.push_str("+-"),
            '©' => out.push_str("(c)"),
            '®' => out.push_str("(r)"),
            '–' | '—' => out.push('-'),
            '‘' | '’' => out.push('\''),
            '“' | '”' => out.push('"'),
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn slugify_parse(text: &str, lowercase: bool) -> String {
    // Carve "Automatic Identifiers" (spec #73), kept byte-identical to
    // carve-js / carve-php:
    //   - keep ASCII alphanumerics AND every non-ASCII code point (>= U+0080)
    //     verbatim; replace each maximal run of ASCII non-alphanumerics with a
    //     single '-' and trim. (Do NOT filter by Unicode is_alphanumeric: the
    //     spec keeps non-ASCII symbols, marks, and punctuation, e.g. a CJK
    //     comma or a bullet, just like the `[^0-9A-Za-z\x80-\x10FFFF]+` rule.)
    //   - smart-typography output is first reversed to its ASCII source (see
    //     de_typography) so an id never depends on presentational typography.
    //   - the DEFAULT is CASE-PRESERVING: kept characters are emitted verbatim
    //     (`# Getting Started` -> `Getting-Started`, `# Über uns` -> `Über-uns`).
    //   - when `lowercase` is set, fold kept characters per code point
    //     (`char::to_lowercase`). Per-code-point folding avoids context mappings
    //     (Greek final-sigma) so the result is portable and matches the other
    //     impls regardless of stdlib whole-string casing behavior. carve-rs has
    //     no ASCII transliterator, so ascii-folding is intentionally not offered
    //     here -- `lowercase` is the only transform.
    let detyped = de_typography(text);
    let mut out = String::new();
    let mut last_dash = false;
    for ch in detyped.chars() {
        if ch.is_ascii_alphanumeric() || ch as u32 >= 0x80 {
            if lowercase {
                for lc in ch.to_lowercase() {
                    out.push(lc);
                }
            } else {
                out.push(ch);
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
    // A leading Unicode number (\p{N}: Nd/Nl/No) is a valid HTML id but not a
    // bare CSS selector, so prefix 's-'. Empty -> 's'. Matches carve-js/php.
    if out.chars().next().is_some_and(char::is_numeric) {
        out = format!("s-{out}");
    }
    if out.is_empty() {
        "s".to_string()
    } else {
        out
    }
}

/// Forced intraword emphasis `{X…X}` (spec §22): emits the same node as the bare
/// delimiter X, but with no word-boundary condition. X is one of `/ * _ ^ , ~ =`.
/// The closing `X}` is the first one after at least one content byte, mirroring
/// the non-greedy `^\{(X)([\s\S]+?)\1\}` match. `{=html}` (no trailing `=`) does
/// not match, so raw-format attribute blocks are unaffected.
fn parse_forced_emphasis(
    bytes: &[u8],
    i: usize,
    options: &Options<'_>,
    in_footnote: bool,
) -> Option<(InlineNode, usize)> {
    let delim = bytes.get(i + 1).copied()?;
    let kind = match delim {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'^' => EmphasisKind::Super,
        b',' => EmphasisKind::Sub,
        b'~' => EmphasisKind::Strike,
        b'=' => EmphasisKind::Highlight,
        _ => return None,
    };
    let content_start = i + 2;
    let mut j = content_start;
    while j + 1 < bytes.len() {
        if bytes[j] == delim && bytes[j + 1] == b'}' {
            if j == content_start {
                return None; // empty content: `+?` requires at least one byte
            }
            let inner = std::str::from_utf8(&bytes[content_start..j]).ok()?;
            return Some((
                InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind,
                    children: parse_inline_context(inner, options, false, in_footnote),
                }),
                j + 2 - i,
            ));
        }
        j += 1;
    }
    None
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
            // Word-boundary closer (spec §9): no bare delimiter closes when
            // followed by an alphanumeric. Applies to every delimiter.
            // NOTE: a `=` is NOT excluded here when it abuts a smart operator
            // (`=b=>`): both reference impls (carve-js, carve-php) let the
            // highlight close there, so rs matches them rather than being the
            // lone grammar-pedantic outlier on this unpinned corner. The
            // operator exclusion applies only to the OPENER (the corpus-pinned
            // `a => b` case).
            if let Some(&next) = bytes.get(j + 1) {
                if next.is_ascii_alphanumeric() {
                    j += 1;
                    continue;
                }
            }
            return Some(j);
        }
        j += 1;
    }
    None
}
