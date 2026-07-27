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
/// it bounds the depth of the AST the renderers walk recursively.
///
/// The cap is 200, applied UNIFORMLY to blockquote, list, div, and admonition
/// nesting, matching carve-js (`MAX_NESTING_DEPTH = 200`) and carve-php so the
/// three implementations degrade at the same depth. Deeply nested input
/// degrades gracefully at the cap (remaining block content becomes a flat
/// paragraph; inline content stays literal) instead of recursing further, so
/// the AST depth is bounded by this constant. The recursive-descent parser and
/// the renderers use one native stack frame per level; in a release build 200
/// levels fit comfortably in a default 2 MiB thread stack (a debug build's
/// larger frames need more, which is why the worst-case-depth robustness tests
/// run on a generous worker stack). carve-php's analogous cap relies on PHP
/// growing its VM stack on the heap.
const MAX_NESTING_DEPTH: usize = 200;

fn trim_ascii_start(s: &str) -> &str {
    s.trim_start_matches([' ', '\t'])
}

fn trim_ascii_end(s: &str) -> &str {
    s.trim_end_matches([' ', '\t'])
}

fn trim_ascii(s: &str) -> &str {
    trim_ascii_end(trim_ascii_start(s))
}

fn is_blank_line(s: &str) -> bool {
    trim_ascii(s).is_empty()
}

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
    parse_with_options_mode(source, options, ParseMode::Html)
}

pub(crate) fn parse_for_carve(source: &str) -> Document {
    parse_with_options_mode(source, &Options::default(), ParseMode::Carve)
}

#[derive(Clone, Copy)]
enum ParseMode {
    Html,
    Carve,
}

fn parse_with_options_mode(source: &str, options: &Options<'_>, mode: ParseMode) -> Document {
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
    let body_start_line = source
        [..(body.as_ptr() as usize).saturating_sub(source.as_ptr() as usize)]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    let (body, footnote_defs_src) = extract_footnote_defs(body, body_start_line);
    let (body_source, link_defs) = extract_link_defs(&body.source);
    let body = remap_source(body_source, &body);
    let footnote_defs = footnote_defs_src
        .into_iter()
        .map(|(label, source)| (label, parse_mapped_source(&source, options)))
        .collect();
    let children = parse_mapped_source(&body, options);
    let mut doc = Document {
        frontmatter,
        footnote_defs,
        children,
        source_len: source.len(),
    };
    let heading_index = heading_index(
        &doc.children,
        &doc.footnote_defs,
        options.lowercase_heading_ids,
    );
    resolve_reference_links(
        &mut doc,
        &link_defs,
        &heading_index,
        matches!(mode, ParseMode::Carve),
    );
    if matches!(mode, ParseMode::Html) {
        apply_abbreviations(&mut doc);
        resolve_crossrefs(&mut doc, options.lowercase_heading_ids);
        // A resolved reference image lands as a one-image paragraph (the
        // syntactic block-image check ran before resolution); promote it to a
        // block image like a standalone direct image, matching carve-php.
        promote_block_images(&mut doc.children, false);
        for blocks in doc.footnote_defs.values_mut() {
            promote_block_images(blocks, false);
        }
        // Single post-resolution pass: a link may not contain another link. Runs
        // after reference and cross-reference resolution because both produce
        // `Link` nodes only at that stage; running earlier would miss the anchors
        // they create. Applied over document inline content and footnote bodies.
        enforce_no_nesting(&mut doc);
    } else {
        // Carve/fmt mode: promote image+caption paragraphs to figures too, so a
        // caption serializes as an unescaped `^ …` line -- portable and
        // round-tripping in every implementation. Without this the caption would
        // stay a paragraph `[Image, SoftBreak, "^ …"]` and the leading `^` would
        // be escaped to `\^`, which only carve-js's lenient parser reads back as
        // a caption (carve-rs / carve-php read it as literal text, losing the
        // figure). Reference-link resolution already ran above.
        promote_block_images(&mut doc.children, true);
        for blocks in doc.footnote_defs.values_mut() {
            promote_block_images(blocks, true);
        }
    }
    for ext in &options.extensions {
        doc = ext.after_parse(doc);
    }
    doc
}

fn remap_source(source: String, original: &MappedSource) -> MappedSource {
    let source_line_count = source.lines().count();
    if source_line_count <= original.line_map.len() {
        return MappedSource {
            source,
            line_map: original.line_map[..source_line_count].to_vec(),
        };
    }
    MappedSource {
        line_map: (1..=source_line_count).map(Some).collect(),
        source,
    }
}

fn extract_footnote_defs(
    source: &str,
    first_source_line: usize,
) -> (MappedSource, BTreeMap<String, MappedSource>) {
    let lines: Vec<&str> = source.lines().collect();
    let mut body = Vec::new();
    let mut body_line_map = Vec::new();
    let mut defs = BTreeMap::new();
    let mut in_fence: Option<FenceOpen> = None;
    let mut i = 0;
    while i < lines.len() {
        // A footnote definition is collected at the top level AND from inside a
        // blockquote / bullet-list container: `> [^a]: body` and `- [^a]: body`
        // both stash the def and leave the container empty, matching carve-js
        // (which recognizes the def inside the container's sub-lexer). Strip the
        // container prefix first, then test the bare content (corpus 115).
        let stripped = strip_container_prefixes(lines[i]);
        let in_container = !stripped.structural.is_empty();
        // A footnote definition is NEVER collected from inside a fenced code
        // block: a `[^x]: ...` line there is literal content. The prepass has
        // only a prefix-stripped line, not the block parser's container-column
        // context, so it recognizes fences only with no residual indentation.
        // This can collect a def from a container-nested fence body, but avoids
        // opening a fence the block parser never opens and swallowing every
        // later definition in the document.
        let fence_line = stripped.bare;
        if let Some(open) = in_fence {
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            if is_fence_close(fence_line, open) {
                in_fence = None;
            }
            i += 1;
            continue;
        }
        if let Some(open) = detect_fence_open(fence_line) {
            in_fence = Some(open);
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
            continue;
        }
        if let Some((label, first)) = parse_footnote_def_line(stripped.bare) {
            let def_start_line = first_source_line + i;
            i += 1;
            let mut def_lines = vec![first.to_string()];
            let mut def_line_map = vec![Some(def_start_line)];
            // Multi-line continuation (indented >= 2) is only gathered for a
            // TOP-LEVEL definition. A container-nested def is single-line here:
            // its continuation would carry the container prefix and is left to
            // normal block parsing, which the spec corpus does not pin.
            if !in_container {
                while i < lines.len() {
                    let line = lines[i];
                    if parse_footnote_def_line(line).is_some() {
                        break;
                    }
                    if is_blank_line(line) {
                        // A footnote body extends to following lines indented by
                        // >= 2 spaces (grammar PART 9 §16); single blank lines
                        // are allowed between chunks. A `+` continuation marker
                        // also keeps the body open (PART 9 §17).
                        if i + 1 < lines.len()
                            && (leading_ws(lines[i + 1]) >= 2 || is_plus_marker(lines[i + 1]))
                        {
                            def_lines.push(String::new());
                            def_line_map.push(Some(first_source_line + i));
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    // Form B: a lone `+` attaches the following flush-left block
                    // to the note with no indentation (the same continuation
                    // marker lists, block quotes and definition bodies use); the
                    // attached block ends at a blank line, another `+`, or the
                    // next footnote definition.
                    if is_plus_marker(line) {
                        i += 1;
                        let mut attached: Vec<String> = Vec::new();
                        let attached_start = i;
                        while i < lines.len() {
                            let a = lines[i];
                            if is_blank_line(a)
                                || is_plus_marker(a)
                                || parse_footnote_def_line(a).is_some()
                            {
                                break;
                            }
                            attached.push(a.to_string());
                            i += 1;
                        }
                        if !attached.is_empty() {
                            def_lines.push(String::new());
                            def_line_map.push(None);
                            let attached_len = attached.len();
                            def_lines.extend(attached);
                            def_line_map.extend(
                                (attached_start..attached_start + attached_len)
                                    .map(|line_idx| Some(first_source_line + line_idx)),
                            );
                        }
                        continue;
                    }
                    if leading_ws(line) >= 2 {
                        def_lines.push(trim_ascii_start(line).to_string());
                        def_line_map.push(Some(first_source_line + i));
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            // First definition for a label wins (later duplicates are ignored).
            defs.entry(label.to_string())
                .or_insert_with(|| MappedSource {
                    source: def_lines.join("\n"),
                    line_map: def_line_map,
                });
            // Leave the container's structural prefix (or a blank line at top
            // level) where the invisible definition was, so the container still
            // renders and the line still acts as a block boundary -- a following
            // paragraph or a lazy blockquote continuation does not absorb across
            // it.
            body.push(stripped.replacement());
            body_line_map.push(Some(def_start_line));
        } else {
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
        }
    }
    (
        MappedSource {
            source: body.join("\n"),
            line_map: body_line_map,
        },
        defs,
    )
}

fn parse_footnote_def_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("[^")?;
    let (label, body) = rest.split_once("]: ")?;
    Some((label, trim_ascii_start(body)))
}

#[derive(Clone)]
struct LinkDef {
    href: String,
    title: Option<String>,
}

fn extract_link_defs(source: &str) -> (String, BTreeMap<String, LinkDef>) {
    let mut body: Vec<String> = Vec::new();
    let mut defs = BTreeMap::new();
    let mut in_fence: Option<FenceOpen> = None;
    // Track enclosing list item content columns so the strict fence test can be
    // re-based to the item's content column. This remains a line-based
    // approximation: tab-vs-space marker alignment is char-counted, the
    // post-blank baseIndent+2 continuation rule is not modeled, and lists
    // nested inside blockquotes are not fully modeled. Those residual cases can
    // still produce a spurious link, not content loss; the sound fix is
    // collecting definitions during block parsing.
    let mut list_cols: Vec<usize> = Vec::new();
    let mut prev_blank = true;
    for line in source.lines() {
        let stripped = strip_container_prefixes(line);
        let was_prev_blank = prev_blank;
        prev_blank = trim_ascii(line).is_empty();
        if in_fence.is_none() {
            let indent = leading_ws(line);
            let raw_trimmed = trim_ascii(line);
            let starts_block = is_heading_marker_line(raw_trimmed)
                || raw_trimmed.starts_with('>')
                || detect_fence_open(raw_trimmed).is_some()
                || detect_thematic_break(raw_trimmed);
            if let Some((marker_indent, marker_width)) = detect_prepass_list_marker(line) {
                while list_cols.last().is_some_and(|col| *col > marker_indent) {
                    list_cols.pop();
                }
                list_cols.push(marker_width);
            } else if !raw_trimmed.is_empty() && (was_prev_blank || starts_block) {
                while list_cols.last().is_some_and(|col| *col > indent) {
                    list_cols.pop();
                }
            }
        }
        let content_col = list_cols.last().copied().unwrap_or(0);
        let raw_is_quoted = prepass_line_is_quoted(line);
        if let Some(open) = in_fence {
            body.push(line.to_string());
            // CLOSER: strip a blockquote prefix only when the fence was opened
            // quoted, and NEVER a list marker. A fence closer is a continuation
            // line of pure indentation, so a literal marker line inside a
            // document-level code sample stays content.
            let close_kept = if open.quoted {
                strip_prepass_blockquote_prefix(line).unwrap_or(line)
            } else {
                line
            };
            let close_indent = leading_ws(close_kept);
            let close_line = if close_indent >= open.content_col {
                &close_kept[open.content_col..]
            } else {
                close_kept
            };
            if is_fence_close(close_line, open) {
                in_fence = None;
            }
            continue;
        }
        // OPENER: strip container prefixes (blockquote AND list marker), then
        // re-base to the current list-item content column. This recognizes a
        // fence on the marker line (`- ````) and on continuation lines.
        let opener_kept;
        let fence_line = if content_col == 0 {
            stripped.bare
        } else {
            opener_kept = strip_container_prefixes_keep_indent(line);
            let kept_indent = leading_ws(&opener_kept);
            if kept_indent >= content_col {
                &opener_kept[content_col..]
            } else {
                opener_kept.as_str()
            }
        };
        if let Some(mut open) = detect_fence_open(fence_line) {
            open.content_col = content_col;
            open.quoted = raw_is_quoted;
            in_fence = Some(open);
            body.push(line.to_string());
            continue;
        }
        if let Some((label_part, target_part)) = parse_link_def_line(stripped.bare) {
            // A reference definition needs a non-empty destination (carve-js
            // `RE_LINK_DEF` requires `(\S+)` after the colon). An empty target
            // (`[r]:` + only whitespace) is NOT a definition -- the line stays
            // literal text. (corpus 34-reference-link-9)
            if label_part.starts_with('@') || target_part.trim().is_empty() {
                body.push(line.to_string());
                continue;
            }
            defs.insert(
                label_part.to_string(),
                parse_link_def_target(target_part.trim()),
            );
            // Leave a blank line in place of the (invisible) definition so it
            // still acts as a block boundary (matches carve-js, where a
            // definition interrupts a paragraph / ends a lazy blockquote).
            body.push(stripped.replacement());
        } else {
            body.push(line.to_string());
        }
    }
    (body.join("\n"), defs)
}

fn prepass_line_is_quoted(line: &str) -> bool {
    strip_prepass_blockquote_prefix(line).is_some()
        || detect_list_marker_full(line)
            .is_some_and(|marker| strip_prepass_blockquote_prefix(marker.content).is_some())
}

fn strip_prepass_blockquote_prefix(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'>') {
        return None;
    }
    i += 1;
    if bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    Some(&line[i..])
}

fn detect_prepass_list_marker(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let marker_indent = i;
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'-' | b'*' => i += 1,
        b'0'..=b'9' => {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if !matches!(bytes.get(i), Some(b'.' | b')')) {
                return None;
            }
            i += 1;
        }
        b'a'..=b'z' | b'A'..=b'Z' => {
            let marker_start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let marker = &line[marker_start..i];
            let ordered = marker.len() == 1
                || marker.bytes().all(|b| {
                    matches!(
                        b,
                        b'i' | b'v'
                            | b'x'
                            | b'l'
                            | b'c'
                            | b'd'
                            | b'm'
                            | b'I'
                            | b'V'
                            | b'X'
                            | b'L'
                            | b'C'
                            | b'D'
                            | b'M'
                    )
                });
            if !ordered || !matches!(bytes.get(i), Some(b'.' | b')')) {
                return None;
            }
            i += 1;
        }
        _ => return None,
    }
    if bytes.get(i) == Some(&b'{') {
        while i < bytes.len() && bytes[i] != b'}' {
            i += 1;
        }
        if bytes.get(i) != Some(&b'}') {
            return None;
        }
        i += 1;
    }
    let spaces_start = i;
    while bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    if i == spaces_start || !bytes.get(i).is_some_and(|b| !b.is_ascii_whitespace()) {
        return None;
    }
    Some((marker_indent, i))
}

struct StrippedContainerLine<'a> {
    structural: &'a str,
    bare: &'a str,
    needs_empty_list_content: bool,
}

impl StrippedContainerLine<'_> {
    fn replacement(&self) -> String {
        let mut replacement = self.structural.to_string();
        if self.needs_empty_list_content {
            replacement.push_str("%%");
        }
        replacement
    }
}

fn parse_link_def_line(line: &str) -> Option<(&str, &str)> {
    line.strip_prefix('[').and_then(|s| s.split_once("]: "))
}

fn strip_container_prefixes(mut line: &str) -> StrippedContainerLine<'_> {
    let original = line;
    let mut needs_empty_list_content = false;
    loop {
        let before = line;
        while let Some(rest) = strip_blockquote_prefix(line) {
            line = rest;
            needs_empty_list_content = false;
        }
        // Only bullets and DECIMAL-ordered markers (ol_type == None) carry a
        // collected definition, matching carve-js and the spec corpus. An
        // alpha/roman ordered marker is left intact (carve-js does not collect
        // those either; byte-parity there is moot as js is itself inconsistent).
        if let Some(marker) = detect_list_marker_full(line) {
            if marker.ol_type.is_none() {
                line = marker.content;
                needs_empty_list_content = true;
            }
        }
        if line.len() == before.len() {
            break;
        }
    }
    // Compute the structural prefix length from the BYTE OFFSET of `line`
    // within `original`, not by length subtraction. `marker_tail` trims trailing
    // ASCII whitespace off the END of the collected content, so `line` can be
    // shorter than its true offset; a length-difference cut would then land
    // inside a leading multibyte content char and panic (`- ́ ` repro). `line`
    // is always a subslice of `original` whose START pointer is preserved by an
    // end-trim, so pointer subtraction yields the correct, char-boundary length.
    let structural_len = line.as_ptr() as usize - original.as_ptr() as usize;
    StrippedContainerLine {
        structural: &original[..structural_len],
        bare: line,
        needs_empty_list_content,
    }
}

fn strip_container_prefixes_keep_indent(mut line: &str) -> String {
    let mut out = String::new();
    loop {
        let before = line;
        while let Some(rest) = strip_blockquote_prefix(line) {
            line = rest;
        }
        if let Some(marker) = detect_list_marker_full(line) {
            let marker_width = marker.content.as_ptr() as usize - line.as_ptr() as usize;
            out.extend(std::iter::repeat(' ').take(marker_width));
            line = marker.content;
        }
        if line.len() == before.len() {
            break;
        }
    }
    out.push_str(line);
    out
}

fn strip_blockquote_prefix(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
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
        // A backslash-escaped quote (or any escaped ASCII punctuation) inside
        // the title is unescaped, matching inline-link titles and carve-js
        // `unescapeAttrValue` (`[y]: /u "a\"b\"c"` -> title `a"b"c`).
        Some(unescape_title(&rest[1..rest.len() - 1]))
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

    let line_map: Vec<Option<usize>> = if options.source_lines {
        (1..=lines.len()).map(Some).collect()
    } else {
        Vec::new()
    };
    let mut cursor = LineCursor::new(&lines, options.source_lines.then_some(&line_map));
    parse_blocks(&mut cursor, options)
}

struct LineCursor<'a> {
    lines: &'a [&'a str],
    line_map: Option<&'a [Option<usize>]>,
    pos: usize,
    /// Lazily built suffix-maximum of each line's colon-closer length: entry `i`
    /// holds the largest all-colon line length at any index `>= i` (0 if none).
    /// A closer for a fence of length `k` is any all-colon line of length `>= k`,
    /// so "a closer of length `>= k` exists at or after `start`" is exactly
    /// `colon_closer_suffix_max[start] >= k` -- independent of the exact fence
    /// length. This defeats the distinct-increasing-fence-length cache miss that
    /// turned a per-fence-length cache into an O(N^2) full rescan per line.
    colon_closer_suffix_max: Option<Vec<usize>>,
}

impl<'a> LineCursor<'a> {
    fn new(lines: &'a [&'a str], line_map: Option<&'a [Option<usize>]>) -> Self {
        LineCursor {
            lines,
            line_map,
            pos: 0,
            colon_closer_suffix_max: None,
        }
    }

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
    fn source_line(&self, pos: usize) -> Option<usize> {
        self.line_map
            .and_then(|map| map.get(pos).copied().flatten())
    }

    fn has_colon_closer_after(&mut self, start: usize, fence_len: usize) -> bool {
        if self.colon_closer_suffix_max.is_none() {
            self.colon_closer_suffix_max = Some(build_colon_closer_suffix_max(self.lines));
        }
        let suffix_max = self.colon_closer_suffix_max.as_ref().unwrap();
        // `start` may sit one past the end (opener is the last line).
        suffix_max.get(start).copied().unwrap_or(0) >= fence_len
    }
}

#[derive(Default)]
struct LineBuffer {
    lines: Vec<String>,
    line_map: Vec<Option<usize>>,
}

impl LineBuffer {
    fn push(&mut self, line: String, source_line: Option<usize>) {
        self.lines.push(line);
        if source_line.is_some() || !self.line_map.is_empty() {
            self.line_map.push(source_line);
        }
    }

    fn push_synthetic_blank(&mut self) {
        self.push(String::new(), None);
    }

    fn into_source(self) -> MappedSource {
        MappedSource {
            source: self.lines.join("\n"),
            line_map: self.line_map,
        }
    }
}

struct MappedSource {
    source: String,
    line_map: Vec<Option<usize>>,
}

impl MappedSource {
    fn new_line(line: String, source_line: Option<usize>) -> Self {
        MappedSource {
            source: line,
            line_map: source_line.into_iter().map(Some).collect(),
        }
    }

    fn push_newline(&mut self, line: String, source_line: Option<usize>) {
        if !self.source.is_empty() {
            self.source.push('\n');
        }
        self.source.push_str(&line);
        if source_line.is_some() || !self.line_map.is_empty() {
            self.line_map.push(source_line);
        }
    }

    fn append(&mut self, other: MappedSource) {
        if other.source.is_empty() {
            return;
        }
        if !self.source.is_empty() {
            self.source.push('\n');
        }
        self.source.push_str(&other.source);
        self.line_map.extend(other.line_map);
    }
}

fn parse_mapped_source(source: &MappedSource, options: &Options<'_>) -> Vec<BlockNode> {
    if !options.source_lines {
        return parse_blocks_with_options(&source.source, options);
    }
    let lines: Vec<&str> = source.source.lines().collect();
    let mut cursor = LineCursor::new(&lines, Some(&source.line_map));
    parse_blocks(&mut cursor, options)
}

/// Build the suffix-maximum of each line's colon-closer length (see
/// `LineCursor::colon_closer_suffix_max`). A line contributes its trimmed length
/// when it is a non-empty all-colon line, else 0. Single O(N) right-to-left pass.
fn build_colon_closer_suffix_max(lines: &[&str]) -> Vec<usize> {
    let mut suffix_max = vec![0usize; lines.len()];
    let mut running = 0usize;
    for idx in (0..lines.len()).rev() {
        let t = trim_ascii(lines[idx]);
        let len = if !t.is_empty() && t.bytes().all(|b| b == b':') {
            t.len()
        } else {
            0
        };
        running = running.max(len);
        suffix_max[idx] = running;
    }
    suffix_max
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
        // Check line-wise: `is_blank_line` only trims spaces/tabs, so a joined
        // multi-line all-blank tail (which contains newlines) must be tested
        // per line, not on the joined string.
        if rest.iter().all(|line| is_blank_line(line)) {
            return Vec::new();
        }
        return vec![BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: parse_inline_with_options(&text, options),
            ..Default::default()
        })];
    };
    let mut out = Vec::new();
    let mut pending_attrs: Option<Attrs> = None;
    while !cur.eof() {
        let line = cur.peek().unwrap();
        // A standalone `{attr}` block opener fires only at the container's
        // content column (flush-left here, since the caller has dedented to that
        // column). An INDENTED `{attr}` line does NOT attach to the following
        // block; it folds as literal paragraph text (strict column-0 rule,
        // docs/divergence-from-djot.md §11), matching carve-php / carve-js.
        let line_flush = !line.starts_with([' ', '\t']);
        if is_blank_line(line) {
            cur.consume();
            continue;
        }
        if is_comment_fence_line(line) {
            let mut content = Vec::new();
            cur.consume();
            while let Some(line) = cur.peek() {
                cur.consume();
                if is_comment_fence_line(line) {
                    break;
                }
                content.push(line.to_string());
            }
            out.push(BlockNode::Comment(Comment {
                block: true,
                content: content.join("\n"),
            }));
            continue;
        }
        if trim_ascii_start(line).starts_with("%%") {
            let content = trim_ascii_start(line)
                .strip_prefix("%%")
                .unwrap_or_default()
                .trim_start()
                .to_string();
            cur.consume();
            out.push(BlockNode::Comment(Comment {
                block: false,
                content,
            }));
            continue;
        }
        if line_flush {
            if let Some(attrs) = parse_standalone_attrs_block(cur) {
                merge_attrs(&mut pending_attrs, attrs);
                continue;
            }
        }
        let start_line = cur.pos;
        if let Some(node) = parse_block(cur, options) {
            let mut node = node;
            if let Some(attrs) = pending_attrs.take() {
                apply_attrs_to_block(&mut node, attrs);
            }
            // Resolve a code fence's opener title to the `title` attribute (after
            // the preceding {title=...} line was applied, so that line wins), so
            // the title lives on the node attrs and survives every consumer: the
            // core renderer, a caption Figure, and a FencedRender extension that
            // rewrites the block (it clones the code block's attrs).
            resolve_code_title(&mut node);
            // Stamp blocks with their 1-based original source line for editor
            // preview scroll-sync. Synthetic extracted lines carry no map entry.
            if options.source_lines {
                if let Some(line) = cur.source_line(start_line) {
                    stamp_source_line(&mut node, line);
                }
            }
            out.push(node);
        }
    }
    out
}

/// True if `attrs` already carries a `title` key (case-insensitive, since HTML
/// attribute names are case-insensitive).
fn attrs_have_title(attrs: &Option<Attrs>) -> bool {
    attrs
        .as_ref()
        .is_some_and(|a| a.key_values.keys().any(|k| k.eq_ignore_ascii_case("title")))
}

/// Copy a code fence's opener `title` to the `title` attribute (unless a
/// preceding `{title=...}` line already set one, which wins). The `title` field
/// is left in place as the source of truth for non-HTML renderers.
fn copy_title_to_attr(cb: &mut CodeBlock) {
    let Some(title) = cb.title.clone() else {
        return;
    };
    if attrs_have_title(&cb.attrs) {
        return;
    }
    let attrs = cb.attrs.get_or_insert_with(Attrs::default);
    attrs.key_values.insert("title".to_string(), title);
    attrs.order.push(AttrSlot::Key("title".to_string()));
}

/// Resolve a code fence's opener title onto the node attrs so it renders
/// uniformly and survives a caption Figure and a FencedRender extension (which
/// clones the code block's attrs). For a captioned block a `{title=...}` line
/// attaches to the figure and wins, so the inner block's title is dropped.
fn resolve_code_title(node: &mut BlockNode) {
    match node {
        BlockNode::CodeBlock(cb) => copy_title_to_attr(cb),
        BlockNode::Figure(f) => {
            if let FigureTarget::CodeBlock(cb) = &mut f.target {
                if attrs_have_title(&f.attrs) {
                    cb.title = None;
                } else {
                    copy_title_to_attr(cb);
                }
            }
        }
        _ => {}
    }
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
            if is_blank_line(next) {
                break;
            }
            if let Some(cont) = heading_continuation_same_level(next, level) {
                joined.push('\n');
                joined.push_str(cont);
                cur.consume();
                continue;
            }
            // A bare same-level marker line continues the heading, contributing
            // no content (checked before is_heading_marker_line, which would
            // otherwise treat it as the start of a new heading).
            if is_bare_same_level_marker(next, level) {
                cur.consume();
                continue;
            }
            if is_heading_marker_line(next)
                || caption_content(next).is_some()
                || is_comment_fence_line(next)
            {
                break;
            }
            // A list marker ENDS the heading and starts a sibling list (it does
            // not fold in). Symmetric §10: a list marker does not interrupt a
            // PARAGRAPH (it folds), but a heading is ended by it -- matching djot
            // (`# T` / `- x` -> heading + list). Bullet and ordered alike.
            let next_owned = next.to_string();
            if is_list_marker(next) || interrupts_paragraph(cur, &next_owned) {
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
        // §756 (NORMATIVE): strip the FINAL line's trailing whitespace only
        // (trim_ascii_end -- ASCII whitespace, so a trailing NBSP survives; an
        // interior trailing run before a soft break is preserved).
        return Some(BlockNode::Heading(Heading {
            attrs: None,
            level,
            children: parse_inline_with_options(trim_ascii_end(&joined), options),
        }));
    }
    if line.starts_with('>') {
        return Some(parse_blockquote(cur, options));
    }
    if is_list_marker(line) {
        return Some(parse_list(cur, options));
    }
    // A table row opens a table only when FLUSH-LEFT (like a heading, quote or
    // `:: ` def-list term). `is_table_start` trims leading whitespace, so an
    // INDENTED row (`  |a|`) would otherwise wrongly open a table where the
    // reference renders a paragraph; a genuine table sits at its container's
    // content column and is already dedented to column 0 here.
    if !line.starts_with([' ', '\t']) && is_table_start(line) {
        return Some(parse_table(cur, options));
    }
    if is_definition_list_start(line) {
        return Some(parse_definition_list(cur, options));
    }
    // A `::: |` line block or `::: \` hard-break block opens ONLY flush-left
    // (at its container's content column), exactly like the div / admonition
    // container check below. `detect_line_block_open` and
    // `detect_hardbreaks_block_open` trim leading whitespace, so an INDENTED
    // colon fence (above the content column) would otherwise still open; the
    // strict column-0 rule (docs/divergence-from-djot.md §11) requires it to
    // fold as literal paragraph text instead. `line` is already dedented to
    // the content column here, so a fence sitting AT that column still opens.
    if !line.starts_with([' ', '\t']) {
        if let Some(fence_len) = detect_line_block_open(line) {
            // A line block, like any colon fence, opens only when a matching
            // closer exists ahead (grammar §12/§23); an unterminated `::: |`
            // stays literal instead of swallowing the rest of the document.
            let has_closer = cur.has_colon_closer_after(cur.pos + 1, fence_len);
            if has_closer {
                return Some(parse_line_block(cur, options));
            }
        }
        if let Some(fence_len) = detect_hardbreaks_block_open(line) {
            // Like a line block, a `::: \` opens only when a matching closer
            // exists ahead (grammar §12/§23); an unterminated opener stays literal.
            let has_closer = cur.has_colon_closer_after(cur.pos + 1, fence_len);
            if has_closer {
                return Some(parse_hardbreaks_block(cur, options));
            }
        }
    }
    // FLUSH-LEFT only: `detect_container_open` trims leading whitespace, so an
    // indented `::: note` below/above a list item's content column must fold as
    // lazy paragraph text (§24 C3), not open a nested container -- uniform with
    // the quote/heading/table checks. `line` is already dedented to the content
    // column here, so a `:::` at the content column opens.
    if !line.starts_with([' ', '\t']) {
        if let Some(open) = detect_container_open(line) {
            // A colon fence opens only when a matching closer (a line of at least
            // `fence_len` colons) exists ahead (grammar §12); an unterminated
            // `:::` / `::: note` stays literal instead of swallowing the rest of
            // the document. Matches carve-js.
            let has_closer = cur.has_colon_closer_after(cur.pos + 1, open.fence_len);
            if has_closer {
                return Some(parse_container(cur, options));
            }
        }
    }
    if let Some(abbr) = detect_abbreviation_def(line) {
        cur.consume();
        return Some(BlockNode::AbbreviationDef(abbr));
    }
    if let Some(img) = detect_block_image(line) {
        if image_is_block(cur) {
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
        // Not standalone: the image folds into a paragraph with the following
        // content (parse_paragraph below); a sole-image paragraph is still
        // promoted to a bare block image afterwards.
    }
    if let Some(matched) = try_extension_block(cur, options) {
        return Some(matched);
    }
    // A block whose sole content is a display-math span (`$$`…``) followed by a
    // caption is a numbered EQUATION. Diverted before the paragraph fallback so
    // parse_paragraph does not fold the caption line into the math paragraph.
    if trim_ascii_start(line).starts_with("$$`") {
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
    let inline = parse_inline_with_options(trim_ascii_start(line), options);
    if inline.len() != 1 || !matches!(&inline[0], InlineNode::Math(m) if m.display) {
        return None;
    }
    // Non-blank, non-caption prose on the very next line: let parse_paragraph
    // fold the math and that text into one paragraph (preserve existing behavior).
    if let Some(next) = cur.lines.get(cur.pos + 1).copied() {
        if !is_blank_line(next) && caption_content(next).is_none() {
            return None;
        }
    }
    // Standalone display math: consume the line, then attach a caption if one
    // follows (directly or across a single blank line).
    cur.consume();
    let target = FigureTarget::Paragraph(Paragraph {
        attrs: None,
        children: inline,
        ..Default::default()
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
    // Return the content VERBATIM (leading tab kept, trailing kept): first-line
    // trailing is interior once continuation lines fold in, so it is stripped
    // only from the final assembled content (§756). The empty gate still tests a
    // trailing-stripped view so `# `, `#  `, `# \t` are not headings.
    let rest = &line[start..];
    if trim_ascii_end(rest).is_empty() {
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
    // Verbatim content (see detect_heading): a continuation line's trailing is
    // interior, so only the final assembled content is trailing-stripped (§756).
    let rest = &line[start..];
    if trim_ascii_end(rest).is_empty() {
        return None;
    }
    Some(rest)
}

/// A bare SAME-level marker line (`#` / `# ` for a level-1 heading): exactly
/// `level` `#`s followed by only spaces. It continues the heading but adds no
/// content, so the surrounding marker lines join with a single newline (djot;
/// "same number of `#` ... or none"). A DIFFERENT count is left to
/// is_heading_marker_line, which ends the heading and starts a new one.
fn is_bare_same_level_marker(line: &str, level: u8) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    hashes == level as usize && bytes[hashes..].iter().all(|&b| b == b' ')
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

/// A line that opens an ATX heading WITH content: `#`..`######`, then a single
/// space, then at least one non-whitespace character. Used to decide whether a
/// list item's marker-line remainder (`- # H`) opens a heading block. Bare (`#`)
/// or whitespace-only (`# `, `#  `) remainders and a tab separator (`#\tH`) are
/// NOT headings here -- they stay inline text, matching carve-js / carve-php on
/// the settled cases (the all-whitespace remainder is tracked separately).
fn heading_content_starts(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if !(1..=6).contains(&hashes) || bytes.get(hashes) != Some(&b' ') {
        return false;
    }
    line[hashes + 1..].bytes().any(|b| b != b' ' && b != b'\t')
}

/// A fenced-comment opener line (a run of 3+ `%`, nothing else) — ends a heading.
fn is_comment_fence_line(line: &str) -> bool {
    let t = trim_ascii_end(line);
    t.len() >= 3 && t.bytes().all(|b| b == b'%')
}

fn detect_thematic_break(line: &str) -> bool {
    // Grammar (spec §262): a col-0 run of 3+ of the SAME `-`/`*`/`_`,
    // CONTIGUOUS (no internal spaces), followed only by trailing whitespace.
    // No leading indent. So `***`/`----`/`___` are breaks, but `* * *` (spaces)
    // and ` ***` (indented) fall through to list/paragraph. A mixed run (`-*-`)
    // is not a break either.
    let bytes = line.as_bytes();
    let marker = match bytes.first() {
        Some(&b @ (b'-' | b'*' | b'_')) => b,
        _ => return false,
    };
    let mut count = 0usize;
    let mut i = 0;
    while i < bytes.len() && bytes[i] == marker {
        count += 1;
        i += 1;
    }
    if count < 3 {
        return false;
    }
    // Only trailing whitespace may follow the contiguous marker run.
    bytes[i..].iter().all(|&b| b == b' ' || b == b'\t')
}

#[derive(Debug, Clone, Copy)]
struct FenceOpen {
    fence_char: u8,
    fence_len: usize,
    content_col: usize,
    quoted: bool,
    lang_start: usize,
    lang_end: usize,
    title_start: Option<usize>,
    title_end: Option<usize>,
    label_start: Option<usize>,
    label_end: Option<usize>,
}

fn detect_fence_open(line: &str) -> Option<FenceOpen> {
    let bytes = line.as_bytes();
    let mut i = 0;
    if bytes.is_empty() {
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
    // Optional whitespace then info string:
    //   [language] ["header"] [[label]]
    // in that fixed order. With no language, a header or label may sit
    // directly against the fence; after a language each following token must
    // be whitespace-separated.
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
            fence_char,
            fence_len,
            content_col: 0,
            quoted: false,
            lang_start,
            lang_end,
            title_start: None,
            title_end: None,
            label_start: None,
            label_end: None,
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
    let has_lang = lang_start < lang_end;
    let mut title_start = None;
    let mut title_end = None;
    let mut label_start = None;
    let mut label_end = None;
    let mut separated = false;
    while i < bytes.len() && bytes[i] == b' ' {
        separated = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'"' {
        if has_lang && !separated {
            return None;
        }
        i += 1;
        let start = i;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                title_start = Some(start);
                title_end = Some(i);
                i += 1;
                break;
            }
            i += 1;
        }
        title_start?;
        separated = false;
        while i < bytes.len() && bytes[i] == b' ' {
            separated = true;
            i += 1;
        }
    }
    if i < bytes.len() && bytes[i] == b'[' {
        if (has_lang || title_start.is_some()) && !separated {
            return None;
        }
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != b']' {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        label_start = Some(start);
        label_end = Some(i);
        i += 1;
    }
    // Must be only whitespace after the info string
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i != bytes.len() {
        return None;
    }
    Some(FenceOpen {
        fence_char,
        fence_len,
        content_col: 0,
        quoted: false,
        lang_start,
        lang_end,
        title_start,
        title_end,
        label_start,
        label_end,
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
    let title = open
        .title_start
        .zip(open.title_end)
        .map(|(start, end)| unescape_quoted_header(&open_line[start..end]));
    let label = open
        .label_start
        .zip(open.label_end)
        .map(|(start, end)| open_line[start..end].to_string());
    let mut content_lines: Vec<String> = Vec::new();
    while let Some(line) = cur.peek() {
        if is_fence_close(line, open) {
            cur.consume();
            break;
        }
        cur.consume();
        content_lines.push(line.to_string());
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
            title,
            label,
            content: content_lines.join("\n"),
        })
    }
}

fn unescape_quoted_header(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_fence_close(line: &str, open: FenceOpen) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
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
    let mut inner = LineBuffer::default();
    let mut para_open = false;
    let mut in_fence: Option<FenceOpen> = None;
    while let Some(line) = cur.peek() {
        if let Some(rest) = line.strip_prefix('>') {
            let source_line = cur.source_line(cur.pos);
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
                // Only a FLUSH-LEFT `:::` container closes the quoted paragraph;
                // an INDENTED `::: note` / `:::` (above the quote's content
                // column) is literal paragraph text, so it keeps the paragraph
                // open and lazy continuation stays in the quote (strict column-0
                // rule, docs/divergence-from-djot.md §11) -- uniform with the
                // opener paths in parse_block / interrupts_paragraph.
                para_open = !is_blank_line(stripped)
                    && (stripped.starts_with([' ', '\t'])
                        || detect_container_open(stripped).is_none())
                    && !trim_ascii_start(stripped).starts_with("%%")
                    && !interrupts_paragraph_with_rest(stripped, &rest_stripped);
            }
            inner.push(stripped.to_string(), source_line);
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
        if trim_ascii(line) == "+" && indent_columns(line) == 0 {
            cur.consume();
            let mut attached = LineBuffer::default();
            while let Some(&next) = cur.lines.get(cur.pos) {
                if is_blank_line(next)
                    || next.starts_with('>')
                    || (trim_ascii(next) == "+" && indent_columns(next) == 0)
                {
                    break;
                }
                attached.push(next.to_string(), cur.source_line(cur.pos));
                cur.pos += 1;
            }
            if !attached.lines.is_empty() {
                // `inner` always holds the quote's first content line, so a
                // leading blank separates the attached block from it.
                inner.push_synthetic_blank();
                inner.lines.extend(attached.lines);
                inner.line_map.extend(attached.line_map);
                inner.push_synthetic_blank();
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
        if !para_open || is_blank_line(line) || caption_content(line).is_some() || {
            let line_owned = line.to_string();
            interrupts_lazy_continuation(cur, &line_owned)
        } {
            break;
        }
        let source_line = cur.source_line(cur.pos);
        cur.consume();
        inner.push(line.to_string(), source_line);
    }
    let inner = inner.into_source();
    let children = parse_mapped_source(&inner, options);
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
        Some(&b' ') => {
            let mut content_start = marker_end;
            while matches!(bytes.get(content_start), Some(b' ' | b'\t')) {
                content_start += 1;
            }
            (trim_ascii_end(&line[content_start..]), None)
        }
        Some(&b'{') => {
            let (attrs, end) = read_list_item_attrs(bytes, marker_end)?;
            if bytes.get(end) != Some(&b' ') {
                return None;
            }
            let mut content_start = end;
            while matches!(bytes.get(content_start), Some(b' ' | b'\t')) {
                content_start += 1;
            }
            (trim_ascii_end(&line[content_start..]), attrs)
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

fn detect_unordered(line: &str) -> Option<(&str, Option<Attrs>, &str)> {
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
    let (content, attrs) = marker_tail(line, i + 1)?;
    Some((content, attrs, &line[i..i + 1]))
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

fn detect_task(line: &str) -> Option<(bool, &str, Option<Attrs>, &str)> {
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
    Some((checked, trim_ascii_end(&after[4..]), attrs, &line[i..i + 1]))
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
        if is_blank_line(l) {
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
        if detect_container_open(line).is_some()
            || detect_line_block_open(line).is_some()
            || detect_hardbreaks_block_open(line).is_some()
        {
            let fence_len = trim_ascii_start(line)
                .bytes()
                .take_while(|b| *b == b':')
                .count();
            let closer = (end + 1..cur.lines.len()).find(|&j| {
                let t = trim_ascii(cur.lines[j]);
                !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= fence_len
            });
            if let Some(close) = closer {
                end = close + 1;
                continue;
            }
        }
        if end > cur.pos && trim_ascii(line) == "+" && indent_columns(line) == base_indent {
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
    let line_map: Vec<Option<usize>> = cur
        .line_map
        .map(|map| map[cur.pos..end].to_vec())
        .unwrap_or_default();
    let mut sub = LineCursor::new(
        &slice,
        cur.line_map.is_some().then_some(line_map.as_slice()),
    );
    let mut block = parse_block(&mut sub, options);
    if options.source_lines {
        if let Some(block) = &mut block {
            if let Some(line) = line_map.first().copied().flatten() {
                stamp_source_line(block, line);
            }
        }
    }
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
        if is_blank_line(line) {
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
        if trim_ascii(line) == "+" && indent_columns(line) == base_indent {
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
                // must reach the item's content column to keep belonging to it
                // (PART 9 §24 C3). A line BELOW content_column ends the list and
                // parses at document level (corpus 81-list-lazy-5). content_col
                // is the item's true column (`- `=2, `1. `=3, `10. `=4), NOT a
                // fixed base+2 -- an ordered item's body sits deeper.
                if pending_blank && indent < content_col {
                    break;
                }
                if let Some(last) = items.last_mut() {
                    let mut nested =
                        collect_item_continuation_block_mapped(cur, base_indent, content_col);
                    // A heading folds its trailing plain text as continuation
                    // (PART 2 headings). When the indented block ends in a
                    // heading and the next lines are flush-left lazy text, pull
                    // them in so the heading parser folds them into the heading
                    // rather than the list ending and the text floating to the
                    // top level (matches carve-php). Only headings fold this
                    // way: a code block or table keeps its trailing text as a
                    // separate top-level block, so the guard is heading-only.
                    if !pending_blank && nested_ends_with_heading(&nested.source, options) {
                        collect_trailing_lazy(cur, &mut nested);
                    } else if !pending_blank
                        && nested_ends_with_open_paragraph(&nested.source, options)
                    {
                        // CommonMark lazy continuation: the dedented non-blank
                        // line folds into the nested block's deepest open
                        // paragraph (e.g. a block quote's trailing paragraph) so
                        // it stays INSIDE the item. The recursive block parse
                        // (block quote lazy continuation) absorbs it.
                        collect_trailing_lazy(cur, &mut nested);
                    }
                    let nested_children = parse_mapped_source(&nested, options);
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
            if pending_blank && marker.indent < content_col {
                break;
            }
            if let Some(last) = items.last_mut() {
                let sub_indent = marker.indent;
                let mut nested = collect_indented_block_mapped(cur, base_indent, content_col);
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
                    let has_lazy = if let Some(line) = cur.peek() {
                        let line = line.to_string();
                        !is_blank_line(&line)
                            && indent_columns(&line) == 0
                            && !is_list_marker(&line)
                            && !interrupts_paragraph(cur, &line)
                    } else {
                        false
                    };
                    if !has_lazy {
                        break;
                    }
                    // Only fold the column-0 lazy line when the collected content
                    // still ends in an OPEN paragraph. After a CLOSED block
                    // (fenced code, table, div) there is none, so the dedented
                    // line ends the item -> top-level (family-D rule).
                    if !nested_ends_with_open_paragraph(&nested.source, options) {
                        break;
                    }
                    let before = cur.pos;
                    collect_trailing_lazy(cur, &mut nested);
                    if cur.pos == before {
                        break;
                    }
                    nested.append(collect_indented_block_mapped(
                        cur,
                        sub_indent - 1,
                        content_col,
                    ));
                }
                let nested_children = parse_mapped_source(&nested, options);
                // A blank line INSIDE the outer item -- swallowed into the nested
                // source by the collection above -- that directly separates the
                // sub-list from a following PARAGRAPH still attached to the outer
                // item makes the OUTER item loose. This is the same paragraph-only
                // rule the plain-continuation branch applies via `pending_blank`
                // (matches carve-js). The check is precise: the blank must
                // directly precede outer-item content (not inner-item content or a
                // sibling marker -- corpus 142: nested looseness does not
                // propagate) and that content must begin a paragraph (a `<hr>`,
                // block quote, or other block opener does not loosen).
                if sublist_source_loosens_outer_item(&nested.source) {
                    tight = false;
                }
                last.children.extend(nested_children);
                continue;
            }
            break;
        }
        if marker.ordered != is_ordered || marker.checked.is_some() != is_task {
            break;
        }
        if !is_ordered && marker.marker != first_marker.marker {
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
        let item_source_line = cur.source_line(cur.pos);
        cur.consume();
        let item_attrs = source_line_attrs(marker.attrs.clone(), item_source_line, options);
        // First-block form `- +` (grammar §17): a lone `+` as the marker
        // content means the item's first block is the following flush-left
        // block (no inline paragraph).
        if trim_ascii(marker.content) == "+" {
            let mut item = ListItem {
                attrs: item_attrs,
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
        if marker.content.starts_with('>') {
            let mut stream =
                MappedSource::new_line(marker.content.to_string(), cur.source_line(cur.pos - 1));
            stream.append(collect_indented_block_mapped(cur, base_indent, content_col));
            let children = parse_mapped_source(&stream, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children,
            });
            continue;
        }
        if detect_list_marker_full(marker.content).is_some() {
            let mut stream =
                MappedSource::new_line(marker.content.to_string(), cur.source_line(cur.pos - 1));
            stream.append(collect_indented_block_mapped(cur, base_indent, content_col));
            // A column-0 lazy-continuation line following the marker-line
            // sub-list folds into its last open paragraph (`- - b` / `lazy` ->
            // `<li>b\nlazy</li>`), and a following sibling marker at the
            // sub-list's column resumes the SAME list. This is the same
            // lazy-fold / resume loop the following-line nested-list path runs
            // above; reused here so the marker-line sub-list behaves identically.
            loop {
                let has_lazy = if let Some(line) = cur.peek() {
                    let line = line.to_string();
                    !is_blank_line(&line)
                        && indent_columns(&line) == 0
                        && !is_list_marker(&line)
                        && !interrupts_paragraph(cur, &line)
                } else {
                    false
                };
                if !has_lazy {
                    break;
                }
                if !nested_ends_with_open_paragraph(&stream.source, options) {
                    break;
                }
                let before = cur.pos;
                collect_trailing_lazy(cur, &mut stream);
                if cur.pos == before {
                    break;
                }
                stream.append(collect_indented_block_mapped(
                    cur,
                    content_col - 1,
                    content_col,
                ));
            }
            let children = parse_mapped_source(&stream, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children,
            });
            continue;
        }
        if marker_content_starts_block(marker.content, cur, content_col) {
            let mut stream =
                MappedSource::new_line(marker.content.to_string(), cur.source_line(cur.pos - 1));
            let before_block = cur.pos;
            stream.append(collect_indented_block_mapped(cur, base_indent, content_col));
            // A heading on the marker line (`- # H`) folds its trailing
            // flush-left plain text as heading continuation (`- # H\nlazy` ->
            // one `<h1>H\nlazy</h1>` inside the item), matching carve-js /
            // carve-php and the indented-heading-in-item path above. Only a
            // heading folds this way; the other marker-line block openers
            // (fence, table, thematic, container) keep their trailing text as a
            // separate block, so the guard is heading-only. A blank line closes
            // the heading (§heading rule 2), so skip the fold once one was
            // consumed while collecting -- `- # H\n\nsep` keeps `sep` as its own
            // top-level block.
            // True only when the block collection ended by swallowing a TRAILING
            // blank separator -- the single-line marker-line block case (heading,
            // thematic break), where the last consumed line is that blank. A
            // blank INSIDE a multiline block (e.g. a fenced code block with an
            // interior blank) is NOT a separator: there the last consumed line is
            // block content, and any real trailing separator is left for the
            // outer loop, so this stays false and tightness is unaffected.
            let swallowed_blank_separator =
                cur.pos > before_block && is_blank_line(cur.lines[cur.pos - 1]);
            if !swallowed_blank_separator && nested_ends_with_heading(&stream.source, options) {
                collect_trailing_lazy(cur, &mut stream);
            }
            let children = parse_mapped_source(&stream, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children,
            });
            // A single-line marker-line block (heading, thematic break) leaves
            // no indented continuation, so collect_indented_block_mapped above
            // swallows the trailing blank separator before the outer loop can
            // see it. Re-raise pending_blank so a following sibling item still
            // loosens the list (`- # H\n\n- b` / `- ---\n\n- b` render `<li>` as
            // loose), matching carve-js / carve-php. A multi-line block (`:::`
            // container) leaves the blank for the outer loop, so
            // swallowed_blank_separator is false there and this does not
            // double-loosen.
            if swallowed_blank_separator {
                pending_blank = true;
            }
            continue;
        }
        // The item's first paragraph is the marker content plus any
        // immediately-following indented prose lines (lazy continuation).
        // It stops at a blank line or a list marker: a nested sublist still
        // interrupts (the one Carve deviation, grammar §10). Block openers that
        // begin ON the marker line are handled above by
        // marker_content_starts_block (heading, fence, thematic, container,
        // table); a would-be opener on a LATER lazy-continuation line stays
        // paragraph text.
        let mut para_lines = vec![marker.content.to_string()];
        let literal_colon_opener = detect_container_open(marker.content)
            .map(|open| open.fence_len)
            .or_else(|| detect_line_block_open(marker.content))
            .or_else(|| detect_hardbreaks_block_open(marker.content));
        while let Some(next) = cur.peek() {
            if is_blank_line(next) || trim_ascii(next) == "+" {
                break;
            }
            if let Some(fence_len) = literal_colon_opener {
                let trimmed = trim_ascii(next);
                if indent_columns(next) <= base_indent
                    && !trimmed.is_empty()
                    && trimmed.bytes().all(|b| b == b':')
                    && trimmed.len() >= fence_len
                {
                    break;
                }
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
            if indent < content_col {
                // BELOW content_column (§24 C3): a line -- flush-left OR indented
                // short of the content column -- lazily continues the item's open
                // paragraph and folds in as text. A block opener here is NOT a
                // block opener (the residual/absent indent disqualifies it), so it
                // never interrupts; only a genuine lazy-continuation interrupt
                // (blank, sibling marker, ...) ends it.
                let next_owned = next.to_string();
                if interrupts_lazy_continuation(cur, &next_owned) {
                    break;
                }
                para_lines.push(trim_ascii_start(next).to_string());
                cur.consume();
                continue;
            }
            if indent > content_col {
                // ABOVE content_column (§24 C3): the line is lazy paragraph text,
                // never a block opener. Fully strip its indent and fold it into
                // the lead paragraph (inline-parsed, so a would-be opener like
                // `> q` renders as literal text, and no residual indent leaks).
                // A sibling/nesting marker above the content column was already
                // handled above; only a genuine lazy interrupt ends the fold.
                let next_owned = next.to_string();
                if interrupts_lazy_continuation(cur, &next_owned) {
                    break;
                }
                para_lines.push(trim_ascii_start(next).to_string());
                cur.consume();
                continue;
            }
            // AT content_column: a block opener interrupts the lead paragraph and
            // nests as a child block; plain text dedents to the body's column 0.
            let dedented = slice_columns(next, content_col, false);
            if interrupts_paragraph(cur, &dedented) {
                break;
            }
            para_lines.push(dedented);
            cur.consume();
        }
        let para_text = para_lines.join("\n");
        let para_text = para_text.trim_end_matches([' ', '\t']);
        let mut paragraph = BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: parse_inline_with_options(para_text, options),
            ..Default::default()
        });
        if options.source_lines {
            if let Some(line) = item_source_line {
                stamp_source_line(&mut paragraph, line);
            }
        }
        items.push(ListItem {
            attrs: item_attrs,
            checked: marker.checked,
            children: vec![paragraph],
        });
    }
    BlockNode::List(List {
        attrs: None,
        ordered: is_ordered,
        start,
        ol_type,
        delim: first_delim.map(char::from),
        bullet_char: if is_ordered {
            None
        } else {
            first_marker
                .marker
                .chars()
                .next()
                .filter(|c| *c == '-' || *c == '*')
        },
        tight,
        items,
    })
}

fn marker_content_starts_block(content: &str, cur: &LineCursor<'_>, content_col: usize) -> bool {
    // A thematic break as the marker-line content is a block (`1. ---` ->
    // <li><hr></li>), not inline text -- otherwise smart punctuation turns
    // `---` into an em-dash. Matches carve-js / carve-php.
    if detect_thematic_break(content) {
        return true;
    }
    // A heading WITH content as the marker-line first block (`- # H` ->
    // <li><h1>H</h1></li>), matching carve-js / carve-php. A heading is a single
    // line, so no multi-line close scan is needed. Bare `#`, a `# ` with no
    // content, or a tab (not the required space) stay inline text.
    if heading_content_starts(content) {
        return true;
    }
    if let Some(open) = detect_fence_open(content) {
        return cur.lines[cur.pos..]
            .iter()
            .map(|line| slice_columns(line, content_col.min(indent_columns(line)), false))
            .any(|line| is_fence_close(&line, open));
    }
    let colon_fence_len = detect_container_open(content)
        .map(|open| open.fence_len)
        .or_else(|| detect_line_block_open(content))
        .or_else(|| detect_hardbreaks_block_open(content));
    if let Some(fence_len) = colon_fence_len {
        return cur.lines[cur.pos..].iter().enumerate().any(|(idx, line)| {
            let indent = indent_columns(line);
            if idx > 0 && indent < content_col {
                return false;
            }
            let line = slice_columns(line, content_col.min(indent), false);
            let trimmed = trim_ascii(&line);
            !trimmed.is_empty() && trimmed.bytes().all(|b| b == b':') && trimmed.len() >= fence_len
        });
    }
    if is_table_start(content) {
        return cur.lines.get(cur.pos).is_some_and(|line| {
            let indent = indent_columns(line);
            indent >= content_col && is_table_start(&slice_columns(line, content_col, false))
        });
    }
    false
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

/// Visual column (tab-aware) at which a list ITEM's continuation content begins,
/// mirroring `parse_list` exactly: for ordered/unordered it is where the marker
/// content begins (`- ` -> 2, `1. ` -> 3, `10. ` -> 4); for a TASK the checkbox
/// counts as content, not marker, so the column is the bullet width (`- ` -> 2).
/// Returns `None` when `line` is not a list marker.
fn marker_content_col(line: &str) -> Option<usize> {
    let m = detect_list_marker_full(line)?;
    if m.checked.is_some() {
        return Some(m.indent + 2);
    }
    let content_off = (m.content.as_ptr() as usize).saturating_sub(line.as_ptr() as usize);
    Some(indent_columns(line) + content_off.saturating_sub(leading_ws(line)))
}

/// Whether `line` (in the dedented sub-list coordinate space) begins a plain
/// PARAGRAPH rather than a block opener. Any indented line is paragraph text
/// under the strict column-0 rule; a flush-left line is a paragraph only when it
/// matches none of the block openers.
fn line_starts_paragraph(line: &str) -> bool {
    if is_blank_line(line) {
        return false;
    }
    if line.starts_with([' ', '\t']) {
        return true;
    }
    detect_heading(line).is_none()
        && !detect_thematic_break(line)
        && !line.starts_with('>')
        && detect_fence_open(line).is_none()
        && detect_container_open(line).is_none()
        && detect_line_block_open(line).is_none()
        && detect_hardbreaks_block_open(line).is_none()
        && detect_abbreviation_def(line).is_none()
        && !is_table_start(line)
        && !is_definition_list_start(line)
        && detect_list_marker_full(line).is_none()
        && !trim_ascii_start(line).starts_with("%%")
}

/// True when the dedented sub-list source carries a blank line that DIRECTLY
/// precedes a PARAGRAPH attached to the OUTER item -- i.e. below the sub-list's
/// own content column and plain paragraph text. That blank is internal to the
/// outer item and loosens it. A blank that precedes inner-item content (reaching
/// the sub-list's content column), a sibling marker, or a non-paragraph outer
/// block (e.g. a `<hr>` or block quote) does NOT loosen the outer item (corpus
/// 142: looseness does not propagate; and the paragraph-only rule the plain
/// continuation branch applies via `pending_blank`).
fn sublist_source_loosens_outer_item(source: &str) -> bool {
    let lines: Vec<&str> = source.split('\n').collect();
    let Some(inner_content_col) = lines
        .iter()
        .find(|l| !is_blank_line(l))
        .and_then(|l| marker_content_col(l))
    else {
        return false;
    };
    let mut prev_blank = false;
    for line in &lines {
        if is_blank_line(line) {
            prev_blank = true;
            continue;
        }
        if prev_blank && indent_columns(line) < inner_content_col && line_starts_paragraph(line) {
            return true;
        }
        prev_blank = false;
    }
    false
}

fn detect_list_marker_full(line: &str) -> Option<ListMarker<'_>> {
    let indent = indent_columns(line);
    if let Some((checked, content, attrs, marker)) = detect_task(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: Some(checked),
            start: None,
            ol_type: None,
            content,
            attrs,
            delim: None,
            marker,
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
    if let Some((content, attrs, marker)) = detect_unordered(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: None,
            start: None,
            ol_type: None,
            content,
            attrs,
            delim: None,
            marker,
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
        // A definition list has no explicit closer either: its last item stays
        // open -- a term still awaiting its `:  ` definition, or a definition
        // whose body ends in a paragraph. A following flush-left `:  ` line (at
        // any column at or below the term) attaches as a `<dd>`, and lazy body
        // text folds into the open definition. This is the lenient def-attach
        // rule shared with carve-php / carve-js: a definition marker is not
        // subject to the column-0-exits rule that ends a list item.
        Some(BlockNode::DefinitionList(dl)) => match dl.items.last() {
            None => false,
            // Bare term, no definition yet: open (awaiting `:  def`).
            Some(item) if item.definitions.is_empty() => true,
            // Otherwise the last definition's body must end in an open paragraph.
            Some(item) => block_ends_with_open_paragraph(
                item.definitions.last().and_then(|d| d.children.last()),
            ),
        },
        // A div / admonition is closed by its `:::` fence -- a complete block
        // with no open paragraph -- so a dedented line after it ends the item
        // (like code/table). Matches carve-js.
        _ => false,
    }
}

fn collect_trailing_lazy(cur: &mut LineCursor, nested: &mut MappedSource) {
    while let Some(line) = cur.peek() {
        if is_blank_line(line) || indent_columns(line) > 0 || is_list_marker(line) || {
            let line_owned = line.to_string();
            interrupts_lazy_continuation(cur, &line_owned)
        } {
            break;
        }
        nested.push_newline(trim_ascii_start(line).to_string(), cur.source_line(cur.pos));
        cur.consume();
    }
}

fn collect_item_continuation_block_mapped(
    cur: &mut LineCursor,
    parent_indent: usize,
    content_col: usize,
) -> MappedSource {
    collect_indented_block_mapped_with(cur, parent_indent, content_col, true)
}

fn collect_indented_block_mapped(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
) -> MappedSource {
    collect_indented_block_mapped_with(cur, parent_indent, strip_cols, false)
}

fn collect_indented_block_mapped_with(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    stop_at_content_column_marker: bool,
) -> MappedSource {
    if cur.line_map.is_none() {
        return MappedSource {
            source: collect_indented_block_plain_with(
                cur,
                parent_indent,
                strip_cols,
                stop_at_content_column_marker,
            ),
            line_map: Vec::new(),
        };
    }
    let mut lines = Vec::new();
    let mut line_map = Vec::new();
    let mut block_indent: Option<usize> = None;
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            // Lazy continuation does not cross a blank line: after a blank, only
            // keep collecting if the next non-blank line is still indented to the
            // block's own level. A shallower line (e.g. a dedent landing below a
            // sublist) ends the block and is left for the caller, so it can close
            // the list rather than fold in (grammar §10, corpus 81-list-lazy-5).
            if let Some(bi) = block_indent {
                let mut k = cur.pos + 1;
                while k < cur.lines.len() && is_blank_line(cur.lines[k]) {
                    k += 1;
                }
                let continues = k < cur.lines.len() && indent_columns(cur.lines[k]) >= bi;
                if !continues {
                    break;
                }
            }
            lines.push(String::new());
            if cur.line_map.is_some() {
                line_map.push(cur.source_line(cur.pos));
            }
            cur.consume();
            continue;
        }
        let indent = indent_columns(line);
        if indent <= parent_indent {
            break;
        }
        let is_marker = detect_list_marker_full(line).is_some();
        if stop_at_content_column_marker && is_marker && indent >= strip_cols {
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
        lines.push(slice_columns(line, strip_cols.min(indent), is_marker));
        if cur.line_map.is_some() {
            line_map.push(cur.source_line(cur.pos));
        }
        cur.consume();
    }
    MappedSource {
        source: lines.join("\n"),
        line_map,
    }
}

fn collect_indented_block_plain_with(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    stop_at_content_column_marker: bool,
) -> String {
    let mut lines = Vec::new();
    let mut block_indent: Option<usize> = None;
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            if let Some(bi) = block_indent {
                let mut k = cur.pos + 1;
                while k < cur.lines.len() && is_blank_line(cur.lines[k]) {
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
        let is_marker = detect_list_marker_full(line).is_some();
        if stop_at_content_column_marker && is_marker && indent >= strip_cols {
            break;
        }
        if block_indent.is_none() {
            block_indent = Some(indent);
        }
        lines.push(slice_columns(line, strip_cols.min(indent), is_marker));
        cur.consume();
    }
    lines.join("\n")
}

fn detect_block_image(line: &str) -> Option<Image> {
    if !line.starts_with("![") {
        return None;
    }
    let bytes = line.as_bytes();
    let bracket_matches = compute_bracket_matches(bytes);
    // Block-image detection runs once on a single line (not in a per-position
    // loop), so full-slice last-occurrence scans are fine here.
    let bounds = InlineBounds {
        matches: &bracket_matches,
        last_close_paren: bytes.iter().rposition(|&b| b == b')'),
        last_close_brace: bytes.iter().rposition(|&b| b == b'}'),
        last_close_bracket: bytes.iter().rposition(|&b| b == b']'),
        last_gt: bytes.iter().rposition(|&b| b == b'>'),
        delim_brace: [None; DELIM_BRACE_SLOTS],
    };
    let (img, consumed) = parse_image_at(bytes, 0, &bounds)?;
    let after = &line[consumed..];
    if !after.trim().is_empty() {
        return None;
    }
    Some(img)
}

fn parse_paragraph(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    // Whether the first line sits at the container's content column (flush-left
    // here, since the caller has dedented to that column). Only a content-column
    // image + `^ caption` promotes to a `<figure>` later; an indented one stays
    // literal (strict column-0 rule).
    let at_content_column = cur.peek().is_some_and(|l| !l.starts_with([' ', '\t']));
    let mut lines: Vec<&str> = Vec::new();
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            break;
        }
        // First line is always part of the paragraph; from the second on, a
        // visible block opener interrupts (§10).
        let line_owned = line.to_string();
        if !lines.is_empty() && interrupts_paragraph(cur, &line_owned) {
            break;
        }
        cur.consume();
        // Leading indentation is not significant in a paragraph (djot has no
        // indented code blocks); strip it so an indented line like ` c` renders
        // as `<p>c</p>`, matching list-item continuation handling.
        lines.push(trim_ascii_start(line));
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
        at_content_column,
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
fn interrupts_paragraph(cur: &mut LineCursor<'_>, line: &str) -> bool {
    // §10 (post-Markdown default): a VISIBLE block interrupts an open paragraph
    // with no blank line. Invisible constructs (comments, abbreviation defs)
    // interrupt too. Ordered lists do NOT interrupt, `+` is the continuation
    // marker not a bullet, and a bare image stays inline.
    if trim_ascii_start(line).starts_with("%%") || detect_abbreviation_def(line).is_some() {
        return true;
    }
    // A standalone block-attribute line floats forward to the next block (or is
    // dropped when none follows, §15), so it interrupts the paragraph rather
    // than folding in as literal text -- but only FLUSH-LEFT, like the
    // quote/heading/table checks below. `parse_standalone_attrs` trims leading
    // whitespace, so without this guard an INDENTED `{...}` line would interrupt
    // where an indented `> q` / `# h` does not; an indented attr line is lazy
    // paragraph text under the strict column-0 rule (§24 C3), not a floater.
    if !line.starts_with([' ', '\t']) && parse_standalone_attrs(line).is_some() {
        return true;
    }
    // Symmetric §10: a list marker (bullet OR task OR ordered) does NOT
    // interrupt a paragraph -- a list needs a blank line before it. Only the
    // other visible blocks interrupt.
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || line.starts_with('>')
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    // Fenced code / `:::` interrupt only with a matching closer ahead. The
    // opener `line` has been dedented to its container's content column by the
    // caller (a list item's lead paragraph dedents by that column), but the
    // closer probe runs over the RAW remaining lines -- so dedent each by the
    // same amount before the column-exact `is_fence_close`, or a closer that
    // carries the container indent is missed and the fence never interrupts.
    // For a flush (column-0) opener the strip is 0, so top-level fences are
    // unaffected; a strict opener only matches when `line` is flush, so the
    // strip comes from the raw current line's own indentation.
    if let Some(open) = detect_fence_open(line) {
        let strip = leading_ws(cur.lines[cur.pos]);
        let rest = &cur.lines[cur.pos + 1..];
        if rest
            .iter()
            .any(|l| is_fence_close(&l[leading_ws(l).min(strip)..], open))
        {
            return true;
        }
    }
    // A colon fence interrupts only when FLUSH-LEFT, like the quote/heading/
    // table checks above -- `detect_container_open` trims leading whitespace, so
    // without this guard an INDENTED `::: note` would interrupt where an indented
    // `> q` / `# h` does not. A `:::` below/above a list item's content column is
    // lazy paragraph text (§24 C3), not a nested container. `line` is already
    // dedented to the container's content column, so at-content-column opens.
    if !line.starts_with([' ', '\t']) {
        if let Some(open) = detect_container_open(line) {
            if cur.has_colon_closer_after(cur.pos + 1, open.fence_len) {
                return true;
            }
        }
    }
    // A `::: |` line block or `::: \` hard-break block interrupts like any
    // colon-fence block, with the same matching-closer lookahead -- but only
    // FLUSH-LEFT, matching the `detect_container_open` guard above and the
    // opener path in `parse_block`. An INDENTED colon fence (the detectors trim
    // leading whitespace) folds as lazy paragraph text instead of splitting the
    // paragraph (strict column-0 rule, docs/divergence-from-djot.md §11).
    if !line.starts_with([' ', '\t']) {
        if let Some(len) =
            detect_line_block_open(line).or_else(|| detect_hardbreaks_block_open(line))
        {
            if cur.has_colon_closer_after(cur.pos + 1, len) {
                return true;
            }
        }
    }
    false
}

fn interrupts_lazy_continuation(cur: &mut LineCursor<'_>, line: &str) -> bool {
    interrupts_paragraph(cur, line) || is_colon_fence_opener_shape(line)
}

fn is_colon_fence_opener_shape(line: &str) -> bool {
    // Only a FLUSH-LEFT colon fence ends lazy continuation regardless of a
    // closer (grammar PART 9 §10). An INDENTED colon-shaped line (the detectors
    // trim leading whitespace) is still within the container; it keeps the
    // normal closer-lookahead via interrupts_paragraph, so an unterminated
    // indented `  ::: note` folds as lazy text instead of escaping the container.
    if line.starts_with(' ') || line.starts_with('\t') {
        return false;
    }
    detect_container_open(line).is_some()
        || detect_line_block_open(line).is_some()
        || detect_hardbreaks_block_open(line).is_some()
}

fn interrupts_paragraph_with_rest(line: &str, rest: &[&str]) -> bool {
    if trim_ascii_start(line).starts_with("%%") || detect_abbreviation_def(line).is_some() {
        return true;
    }
    // Flush-left only (see interrupts_paragraph): an indented `{...}` line is
    // lazy paragraph text under the strict column-0 rule, not a floating attr.
    if !line.starts_with([' ', '\t']) && parse_standalone_attrs(line).is_some() {
        return true;
    }
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || line.starts_with('>')
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    if let Some(open) = detect_fence_open(line) {
        if rest.iter().any(|l| is_fence_close(l, open)) {
            return true;
        }
    }
    // A FLUSH-LEFT colon-fence family opener (`::: |` line block, `::: \`
    // hard-break block) interrupts blockquote lazy continuation like any block
    // opener, matching the plain `:::` div the caller already guards. Without
    // this, an unquoted line after a quoted opener is wrongly absorbed into the
    // quote. An INDENTED colon fence (above the quote's content column) is
    // literal paragraph text under the strict column-0 rule, so lazy
    // continuation stays inside the quote -- uniform with the opener and
    // interrupt paths in parse_block / interrupts_paragraph. carve-js lags on
    // the hard-break block, so the spec corpus (88-line-blocks) -- not carve-js
    // -- is the reference here (carve-rs issue 148).
    if !line.starts_with([' ', '\t'])
        && (detect_line_block_open(line).is_some() || detect_hardbreaks_block_open(line).is_some())
    {
        return true;
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
    line.strip_prefix(":: ")
        .is_some_and(|term| !is_blank_line(term))
}

fn parse_definition_list(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let mut items = Vec::new();
    while let Some(line) = cur.peek() {
        let Some(term) = line.strip_prefix(":: ") else {
            break;
        };
        if is_blank_line(term) {
            break;
        }
        let term_source_line = cur.source_line(cur.pos);
        cur.consume();
        // A term folds a following plain line like a heading (soft break), so a
        // wrapped term line does not strand the definition. A blank line, a new
        // marker (`::` / `:  `), a list marker, or a block opener ends the term.
        let mut term_text = trim_ascii_end(term).to_string();
        while let Some(next) = cur.peek() {
            if is_blank_line(next)
                || next.strip_prefix(":: ").is_some()
                || next.strip_prefix(":  ").is_some()
                || is_list_marker(next)
            {
                break;
            }
            let owned = next.to_string();
            if interrupts_paragraph(cur, &owned) {
                break;
            }
            term_text.push('\n');
            term_text.push_str(&owned);
            cur.consume();
        }
        let terms = vec![DefinitionTerm {
            attrs: source_line_attrs(None, term_source_line, options),
            children: parse_inline_with_options(&term_text, options),
        }];
        let mut defs = Vec::new();

        loop {
            // A blank line before a `:  ` definition is a separator (djot
            // parity): a definition may be separated from its term or a
            // previous definition by a blank line. A blank not followed by a
            // `:  ` definition ends the entry.
            if matches!(cur.peek(), Some(l) if is_blank_line(l)) {
                let mut look = 0usize;
                while matches!(cur.lines.get(cur.pos + look).copied(), Some(l) if is_blank_line(l))
                {
                    look += 1;
                }
                match cur.lines.get(cur.pos + look).copied() {
                    Some(after) if after.strip_prefix(":  ").is_some() => {
                        for _ in 0..look {
                            cur.consume();
                        }
                    }
                    _ => break,
                }
            }
            let Some(line) = cur.peek() else {
                break;
            };
            let Some(def) = line.strip_prefix(":  ") else {
                break;
            };
            if is_blank_line(def) {
                break;
            }
            let def_source_line = cur.source_line(cur.pos);
            cur.consume();
            let def_trimmed = trim_ascii_end(def);
            // First-block form (`:  +`, mirroring the list `- +`): when the sole
            // content is a lone `+`, seed the body with the FOLLOWING flush-left
            // block (no `+` literal), with no indentation. `:  \+` stays literal.
            let mut body = if is_plus_marker(def_trimmed) {
                let mut fb = LineBuffer::default();
                while let Some(a) = cur.peek() {
                    if is_blank_line(a)
                        || is_plus_marker(a)
                        || a.strip_prefix(":: ").is_some()
                        || a.strip_prefix(":  ").is_some()
                    {
                        break;
                    }
                    fb.push(a.to_string(), cur.source_line(cur.pos));
                    cur.consume();
                }
                fb.into_source()
            } else {
                MappedSource::new_line(def_trimmed.to_string(), def_source_line)
            };
            body.append(collect_definition_body(cur));
            defs.push(DefinitionDef {
                attrs: source_line_attrs(None, def_source_line, options),
                children: parse_mapped_source(&body, options),
            });
        }

        items.push(DefinitionItem {
            terms,
            definitions: defs,
        });

        let saved = cur.pos;
        while matches!(cur.peek(), Some(line) if is_blank_line(line)) {
            cur.consume();
        }
        if !cur.peek().is_some_and(is_definition_list_start) {
            cur.pos = saved;
            break;
        }
    }
    BlockNode::DefinitionList(DefinitionList { attrs: None, items })
}

/// A lone `+` (optionally followed by spaces/tabs) is the continuation marker
/// (PART 9 §17): it attaches the following flush-left block to the open
/// container.
fn is_plus_marker(line: &str) -> bool {
    line.strip_prefix('+')
        .is_some_and(|rest| rest.bytes().all(|b| b == b' ' || b == b'\t'))
}

/// Collect the continuation of a definition body. A definition continues like a
/// list item (PART 9 §17): form A folds an indented block in (a blank line is
/// tolerated when a later line still continues), form B attaches a lone `+`
/// pull-left flush-left block with no indentation, and a flush-left line with no
/// blank before it that does not start an interrupting block lazily continues
/// the open paragraph (matching list items, block quotes and djot). Returned
/// lines carry blank separators so the block sub-parse yields multiple paragraphs.
fn collect_definition_body(cur: &mut LineCursor) -> MappedSource {
    let mut lines: Vec<String> = Vec::new();
    let mut line_map: Vec<Option<usize>> = Vec::new();
    while let Some(line) = cur.peek() {
        // Form B: `+` pull-left continuation.
        if is_plus_marker(line) {
            cur.consume();
            let mut attached = LineBuffer::default();
            while let Some(a) = cur.peek() {
                if is_blank_line(a)
                    || is_plus_marker(a)
                    || a.strip_prefix(":: ").is_some()
                    || a.strip_prefix(":  ").is_some()
                {
                    break;
                }
                attached.push(a.to_string(), cur.source_line(cur.pos));
                cur.consume();
            }
            if !attached.lines.is_empty() {
                lines.push(String::new());
                line_map.push(None);
                lines.extend(attached.lines);
                line_map.extend(attached.line_map);
            }
            continue;
        }
        // Form A: an indented continuation line (no intervening blank).
        if !is_blank_line(line) {
            let indent = indent_columns(line);
            if indent >= 3 {
                lines.push(slice_columns(line, 3.min(indent), false));
                line_map.push(cur.source_line(cur.pos));
                cur.consume();
                continue;
            }
            // A new term/definition marker ends the definition (the outer loop
            // picks it up).
            if line.strip_prefix(":: ").is_some() || line.strip_prefix(":  ").is_some() {
                break;
            }
            // Lazy continuation: a flush-left line with no blank before it that
            // does not start an interrupting block folds into the open
            // paragraph (the same rule list items and block quotes use, matching
            // djot). A block opener ends the definition.
            let owned = line.to_string();
            if !interrupts_paragraph(cur, &owned) {
                lines.push(owned);
                line_map.push(cur.source_line(cur.pos));
                cur.consume();
                continue;
            }
            break;
        }
        // Blank line: absorb it as a paragraph separator ONLY when a later line
        // still continues the definition (form A); otherwise leave it for the
        // entry separator / outer block stream.
        let mut look = 0usize;
        while matches!(cur.lines.get(cur.pos + look).copied(), Some(l) if is_blank_line(l)) {
            look += 1;
        }
        match cur.lines.get(cur.pos + look).copied() {
            Some(after) if !is_blank_line(after) && indent_columns(after) >= 3 => {
                for _ in 0..look {
                    lines.push(String::new());
                    line_map.push(cur.source_line(cur.pos));
                    cur.consume();
                }
            }
            _ => break,
        }
    }
    MappedSource {
        source: lines.join("\n"),
        line_map,
    }
}

/// A bare image line is a block image (or figure) ONLY when it stands alone --
/// the next line is blank / EOF, a `^ ` caption, or a paragraph interrupter.
/// When the next line FOLDS (plain text, list marker, another bare image), the
/// image stays inline in a paragraph with that content, per grammar §1722 I3
/// ("an image is not a block of its own; it stays inline in the paragraph").
fn image_is_block(cur: &mut LineCursor) -> bool {
    let Some(next) = cur.lines.get(cur.pos + 1).copied() else {
        return true;
    };
    if is_blank_line(next) || caption_content(next).is_some() {
        return true;
    }
    // Peek-1 interruption: test the next line as if it were current, then rewind.
    let next_owned = next.to_string();
    let saved = cur.pos;
    cur.pos += 1;
    let interrupts = interrupts_paragraph(cur, &next_owned);
    cur.pos = saved;
    interrupts
}

fn consume_caption(cur: &mut LineCursor, options: &Options<'_>) -> Option<Vec<InlineNode>> {
    let saved = cur.pos;
    while matches!(cur.peek(), Some(line) if is_blank_line(line)) {
        cur.consume();
    }
    let Some(line) = cur.peek() else {
        cur.pos = saved;
        return None;
    };
    let Some(text) = caption_content(line) else {
        cur.pos = saved;
        return None;
    };
    let mut joined = text.to_string();
    cur.consume();
    // A caption is multi-line inline content, so it folds following lines like a
    // PARAGRAPH (§10), NOT like a heading: a list marker FOLDS in (djot -- a
    // list needs a blank line to interrupt), while a heading / blockquote /
    // table / fenced code / `:::` div / thematic break / `%%%` comment
    // interrupts and ends the caption. A blank line or a further `^ ` caption
    // line also ends it. Continuation lines join with `\n`.
    while let Some(next) = cur.peek() {
        if is_blank_line(next) || caption_content(next).is_some() {
            break;
        }
        let next_owned = next.to_string();
        if interrupts_paragraph(cur, &next_owned) {
            break;
        }
        joined.push('\n');
        joined.push_str(next);
        cur.consume();
    }
    // §756 (NORMATIVE): strip the final line's trailing whitespace only.
    Some(parse_caption_inline_with_options(
        trim_ascii_end(&joined),
        options,
    ))
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
    if trimmed == "||" {
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
            let last_close_brace = bytes.iter().rposition(|&b| b == b'}');
            if let Some((attrs, next)) = read_attrs_at(bytes, idx + 1, last_close_brace) {
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
        if is_table_continuation(line) {
            if saw_separator && rows.len() == 1 {
                break;
            }
            cur.consume();
            if let Some(last) = rows.last_mut() {
                apply_table_continuation(last, line, options);
            }
            continue;
        }
        cur.consume();
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
    // `continuation_row` ends in `'|'` just like `standard_row`, so the closing
    // pipe is required here too: `+ c | d` is prose and ends the table. Unlike a
    // standard row it has no `row_attributes` slot, so a trailing `|{.x}` does
    // NOT stand in for the closing pipe.
    let trimmed = line.trim();
    if trimmed.len() < 2 || !trimmed.starts_with('+') || !trimmed.ends_with('|') {
        return false;
    }
    trimmed != "+|"
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
        let cell_bytes = cell.as_bytes();
        let last_close_brace = cell_bytes.iter().rposition(|&b| b == b'}');
        if let Some((attrs, next)) = read_attrs_at(cell_bytes, 0, last_close_brace) {
            return TableCell {
                header: false,
                span: None,
                align: None,
                attrs: Some(attrs),
                children: parse_inline_with_options(cell[next..].trim(), options),
            };
        }
    }

    // A leading `=` marks a HEADER cell, but only when GLUED to the `|` (no
    // leading whitespace), per grammar §20. `| =x |` (space before `=`) is a
    // literal `<td>`, matching carve-js / carve-php; check the RAW cell, not
    // the trimmed one.
    let header = cell.starts_with('=');
    let trimmed = if header {
        cell[1..].trim()
    } else {
        cell.trim()
    };
    let mut text = trimmed;
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
    label: Option<String>,
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
            label: None,
            attrs: None,
        });
    }
    if let Some(label) = parse_bare_label(rest) {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            label: Some(label),
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
    let after_kind = &rest[id_end..];
    if !after_kind.is_empty() && !after_kind.starts_with(char::is_whitespace) {
        return None;
    }
    let mut after = after_kind.trim_start();
    let title = if after.starts_with('"') {
        let (title, remainder) = parse_quoted_metadata(after)?;
        after = remainder.trim_start();
        Some(title)
    } else {
        None
    };
    let label = if after.starts_with('[') {
        let close = after.find(']')?;
        let label = after[1..close].to_string();
        if !after[close + 1..].trim().is_empty() {
            return None;
        }
        Some(label)
    } else {
        if !after.is_empty() {
            return None;
        }
        None
    };
    Some(ContainerOpen {
        fence_len,
        kind: Some(kind),
        title,
        label,
        attrs: None,
    })
}

fn parse_bare_label(s: &str) -> Option<String> {
    let close = s.find(']')?;
    if !s.starts_with('[') || !s[close + 1..].trim().is_empty() {
        return None;
    }
    Some(s[1..close].to_string())
}

fn parse_quoted_metadata(s: &str) -> Option<(String, &str)> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some((unescape_quoted_header(&s[1..i]), &s[i + 1..]));
        }
        i += 1;
    }
    None
}

fn parse_container(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let open = detect_container_open(cur.peek().unwrap()).unwrap();
    cur.consume();
    let mut inner = LineBuffer::default();
    while let Some(line) = cur.peek() {
        if line.trim().bytes().all(|b| b == b':') && line.trim().len() >= open.fence_len {
            cur.consume();
            break;
        }
        inner.push(line.to_string(), cur.source_line(cur.pos));
        cur.consume();
    }
    let children = parse_mapped_source(&inner.into_source(), options);
    if let Some(kind) = open.kind {
        BlockNode::Admonition(Admonition {
            attrs: open.attrs,
            kind,
            title: open.title.map(|t| parse_inline_with_options(&t, options)),
            label: open.label,
            children,
        })
    } else {
        BlockNode::Div(Div {
            attrs: open.attrs,
            label: open.label,
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

/// Expand a line's LEADING whitespace to non-breaking spaces so a verse line's
/// indentation survives; tabs advance to the next 4-column stop. The rest of
/// the line is left untouched. Uses the generated-NBSP placeholder (HTML folds
/// it to `&nbsp;`; plain/ANSI turn it back into an ASCII space), so it stays
/// distinct from a literal U+00A0 typed in the source.
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
    format!(
        "{}{}",
        crate::NBSP_PLACEHOLDER.to_string().repeat(columns),
        &line[idx..]
    )
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
    let mut stanzas: Vec<LineBuffer> = Vec::new();
    let mut stanza: Vec<String> = Vec::new();
    let mut stanza_line_map: Vec<Option<usize>> = Vec::new();
    while let Some(line) = cur.peek() {
        let t = trim_ascii(line);
        if !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= fence_len {
            cur.consume();
            break;
        }
        let source_line = cur.source_line(cur.pos);
        cur.consume();
        if is_blank_line(line) {
            if !stanza.is_empty() {
                stanzas.push(LineBuffer {
                    lines: std::mem::take(&mut stanza),
                    line_map: std::mem::take(&mut stanza_line_map),
                });
            }
            continue;
        }
        let stripped = strip_leading_columns(line, base_indent);
        stanza.push(expand_line_block_leading_ws(&stripped));
        stanza_line_map.push(source_line);
    }
    if !stanza.is_empty() {
        stanzas.push(LineBuffer {
            lines: stanza,
            line_map: stanza_line_map,
        });
    }

    let children = stanzas
        .into_iter()
        .map(|lines| {
            let source_line = lines.line_map.first().copied().flatten();
            let inlines = parse_inline_with_options(&lines.lines.join("\n"), options)
                .into_iter()
                .map(|n| match n {
                    InlineNode::SoftBreak => InlineNode::HardBreak,
                    other => other,
                })
                .collect();
            let mut node = BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: inlines,
                ..Default::default()
            });
            if options.source_lines {
                if let Some(line) = source_line {
                    stamp_source_line(&mut node, line);
                }
            }
            node
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
        label: None,
        children,
    })
}

/// A `::: \` local hard-break block opener: a colon fence (3+) then a bare
/// backslash and nothing else (grammar PART 9 §23). Returns the fence length.
/// Deliberately smaller than a line block: it converts soft breaks in DIRECT
/// paragraph children to hard breaks, but does NOT preserve leading whitespace,
/// keeps the stanza/block structure of its body, and does not affect nested
/// blocks. Mirrors carve-js `RE_HARDBREAKS_BLOCK_OPEN` / `parseHardBreaksBlock`.
fn detect_hardbreaks_block_open(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    // `hardbreaks_block_open = colon_fence, space, "\"` -- a space (or tab)
    // between the fence and the backslash is REQUIRED, so `:::\` is not one.
    let after = &trimmed[fence_len..];
    let trimmed_after = after.trim_start_matches([' ', '\t']);
    if trimmed_after.len() == after.len() {
        return None; // no whitespace before the backslash
    }
    if trimmed_after.trim_end() == "\\" {
        Some(fence_len)
    } else {
        None
    }
}

/// Parse a `::: \` local hard-break block into a `<div class="hardbreaks">`:
/// the body is parsed as ordinary blocks, then every soft break in a DIRECT
/// paragraph child becomes a hard break. Unlike a line block, leading
/// whitespace is not preserved and nested blocks keep ordinary soft breaks.
fn parse_hardbreaks_block(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let opener = cur.peek().unwrap();
    let fence_len = detect_hardbreaks_block_open(opener).unwrap();
    cur.consume();
    let mut inner = LineBuffer::default();
    while let Some(line) = cur.peek() {
        let t = line.trim();
        if !t.is_empty() && t.bytes().all(|b| b == b':') && t.len() >= fence_len {
            cur.consume();
            break;
        }
        inner.push(line.to_string(), cur.source_line(cur.pos));
        cur.consume();
    }
    let mut children = parse_mapped_source(&inner.into_source(), options);
    for child in &mut children {
        if let BlockNode::Paragraph(para) = child {
            for node in &mut para.children {
                if matches!(node, InlineNode::SoftBreak) {
                    *node = InlineNode::HardBreak;
                }
            }
        }
    }
    // No inline opener attributes (strict djot); a preceding block-attribute
    // line merges onto this div in parse_blocks.
    BlockNode::Div(Div {
        attrs: Some(Attrs {
            id: None,
            classes: vec!["hardbreaks".to_string()],
            key_values: BTreeMap::new(),
            order: vec![AttrSlot::Class],
        }),
        label: None,
        children,
    })
}

fn detect_abbreviation_def(line: &str) -> Option<AbbreviationDef> {
    let rest = line.strip_prefix("*[")?;
    let (abbr, expansion) = rest.split_once("]:")?;
    let expansion = expansion.strip_prefix(' ')?;
    if abbr.is_empty() || !abbr.chars().all(char::is_alphanumeric) {
        return None;
    }
    Some(AbbreviationDef {
        abbr: abbr.to_string(),
        expansion: expansion.trim().to_string(),
    })
}

/// First byte of an attribute identifier (id/class/key): a letter or `_`
/// (matches `is_identifier`'s first-char rule). Non-ASCII bytes are never a
/// start here (`is_identifier` uses `is_ascii_alphabetic`).
#[inline]
fn is_attr_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

/// Continuation byte of an attribute identifier: a letter, digit, `_`, or `-`
/// (matches `is_identifier`'s tail rule). Non-ASCII → false, ending the run.
#[inline]
fn is_attr_ident_part(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Whether the attribute payload following the `{` at `brace` provably cannot
/// parse -- i.e. `read_attrs_at`'s scan+`parse_attrs` would return `None`.
///
/// It walks the SAME token grammar `attr_tokens`/`parse_attrs` accept and bails
/// at the first byte that cannot continue a valid token, so a doomed payload is
/// rejected in O(1) per opener instead of the char walk running to a far `}`
/// (O(n²) on `[x]{`×n + `}`, `[x]{a `×n + `}`, `[x]{.a `×n + `}`, `[x]{k= `×n +
/// `}`, …). It is a pure SKIP filter: it returns `true` ONLY when the payload is
/// provably invalid; on a `}` (a candidate close), a newline, a quote, an
/// escape, a `key=<value>` with a real value, or ANY non-ASCII byte (a possible
/// Unicode-whitespace separator or non-ASCII content), it returns `false` and
/// the unchanged scan/`parse_attrs` path decides -- so every accepted block, and
/// its output, is byte-identical. A nested `{`/`[` (or any other invalid
/// boundary byte) ends the walk, so each byte is visited O(1) times -> O(n)
/// total. Deferring on non-ASCII keeps it correct without decoding chars (only
/// the ASCII pathological shapes need the O(1) bail; a non-ASCII payload is rare
/// and still handled correctly by the full scan). Mirrors carve-js
/// `spanAttrProvablyInvalid`, matched to carve-rs's first-`}` (non-balancing)
/// acceptance.
fn attr_payload_provably_invalid(bytes: &[u8], brace: usize) -> bool {
    let n = bytes.len();
    let mut i = brace + 1;
    while i < n {
        let c = bytes[i];
        // Non-ASCII: a Unicode-whitespace separator, a non-ASCII value byte, or
        // other subtle content. Defer to the full scan/parse (byte-identical;
        // non-ASCII is never the repeated ASCII pathological shape).
        if !c.is_ascii() {
            return false;
        }
        match c {
            // A candidate close at a token boundary: let the real scan decide.
            b'}' => return false,
            // A newline ends an inline block (read_attrs_at bails); defer.
            b'\n' => return false,
            // Other ASCII whitespace separates tokens (attr_tokens treats
            // char::is_whitespace as a separator); skip it and continue.
            b' ' | b'\t' | 0x0B | 0x0C | b'\r' => i += 1,
            // Quotes and escapes are subtle -- defer.
            b'"' | b'\'' | b'\\' => return false,
            // `#id` / `.class`: an identifier MUST follow, else the token (and
            // the whole payload) is invalid (§14).
            b'#' | b'.' => {
                match bytes.get(i + 1) {
                    Some(&d) if is_attr_ident_start(d) => {}
                    _ => return true,
                }
                i += 2;
                while i < n && is_attr_ident_part(bytes[i]) {
                    i += 1;
                }
            }
            // A bareword: a boolean attribute, or the name in `key=value`.
            _ if is_attr_ident_start(c) => {
                i += 1;
                while i < n && is_attr_ident_part(bytes[i]) {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'=') {
                    // `key=` with an EMPTY value (EOF, `}`, or ASCII whitespace
                    // next) leaves a dangling `=` -> invalid. A bare value
                    // (>=1 non-space) or a quoted value: defer (a valid bare
                    // value is consumed whole by the scan -> linear). Non-ASCII
                    // after `=` (a value byte or Unicode space) also defers.
                    match bytes.get(i + 1) {
                        None => return true,
                        Some(&b'}') => return true,
                        Some(&v) if v.is_ascii() && v.is_ascii_whitespace() => return true,
                        _ => return false,
                    }
                }
                // else continue to the next token.
            }
            // Any other ASCII byte cannot begin a valid token at a boundary
            // (`[`, `{`, `(`, a digit, `-`, `+`, `=`, `,`, …): invalid.
            _ => return true,
        }
    }
    // Ran off the end without a `}`: the scan would fail too.
    true
}

/// Read an inline attribute block `{...}` at `start` (which must index a `{`).
///
/// `last_close_brace` is the index of the last `}` in `bytes` (or `None` if
/// there is none). A block can only close on a `}`, so when no `}` lies at or
/// after `start` the scan could only walk to end-of-text and fail -- it is
/// skipped in O(1). This keeps a run of unclosed `{`-attribute openers
/// (`[x]{`×n, `*a*{`×n, `:a:{`×n, …) linear instead of O(n^2). Callers scanning
/// a fresh slice (block attributes, table cells) pass that slice's own last-`}`
/// index. Skipping only elides a call that would return `None`, so output is
/// byte-identical.
fn read_attrs_at(
    bytes: &[u8],
    start: usize,
    last_close_brace: Option<usize>,
) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    if last_close_brace.map_or(true, |p| p < start) {
        return None;
    }
    // Reject, in O(1) per opener, a payload that provably cannot parse before the
    // char walk below runs to a far `}`. Without this, a run of never-validating
    // openers whose only `}` lies far ahead (`[x]{`×n + `}`, `[x]{a `×n + `}`, …)
    // walks to that `}` AND re-parses the whole tail at every opener -- O(n^2).
    // The filter only reports "invalid" where `parse_attrs` would return `None`
    // too, so output is byte-identical.
    if attr_payload_provably_invalid(bytes, start) {
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
            if value.is_empty() {
                return None;
            }
            // A quoted value unescapes ANY backslash-escaped ASCII punctuation
            // (grammar: escaped_char = '\' ascii_punctuation), not just \" / \'.
            // Route it through the same scan link/image titles use, matching
            // carve-js / carve-php. A bare value carries no escapes.
            let value = if let Some(inner) = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            {
                unescape_title(inner)
            } else {
                value.to_string()
            };
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
        if (ch == '#' || ch == '.') && (buf.starts_with('#') || buf.starts_with('.')) {
            tokens.push(std::mem::take(&mut buf));
            buf.push(ch);
        } else if ch.is_whitespace() {
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
    if !trimmed.starts_with('{') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let last_close_brace = bytes.iter().rposition(|&b| b == b'}');
    let mut pos = 0usize;
    let mut attrs: Option<Attrs> = None;
    while pos < bytes.len() {
        let (incoming, next) = read_attrs_at(bytes, pos, last_close_brace)?;
        merge_attrs(&mut attrs, incoming);
        pos = next;
        if pos < bytes.len() && bytes[pos] != b'{' {
            return None;
        }
    }
    attrs
}

/// A standalone block-attribute block, possibly spanning several contiguous
/// (non-blank) lines: it opens with `{` and closes with `}` on a later line
/// (`{#id` / ` .foo}`). Consumes the lines and returns the parsed attributes,
/// or leaves the cursor untouched if it is not a valid attribute block.
fn parse_standalone_attrs_block(cur: &mut LineCursor) -> Option<Attrs> {
    let first = cur.peek()?;
    if !trim_ascii_start(first).starts_with('{') {
        return None;
    }
    if let Some(attrs) = parse_standalone_attrs(first) {
        cur.consume();
        return Some(attrs);
    }
    // A COMPLETE single line (already closes with `}`) that parse_standalone_attrs
    // rejected is not a valid attribute block -- do NOT rescue it via the
    // multi-line strip-outer path below, which would parse an interior `}{` as an
    // unquoted value (`{k=v}{+i+}` -> k="v}{+i+", swallowing the whole line). The
    // multi-line join is only for a block that genuinely continues onto later
    // lines (`{#id` then `.foo}`), i.e. whose first line does not itself close.
    // Matches carve-js, which keeps such a line literal.
    if trim_ascii_end(first).ends_with('}') {
        return None;
    }
    // Multi-line: join contiguous lines until one closes with `}`.
    let mut joined = String::new();
    let mut count = 0usize;
    while let Some(line) = cur.lines.get(cur.pos + count).copied() {
        if is_blank_line(line) {
            return None;
        }
        if !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(trim_ascii(line));
        count += 1;
        if trim_ascii_end(line).ends_with('}') {
            let inner = trim_ascii(&joined);
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

/// Add a `data-source-line` attribute to a block node, preserving any existing
/// attributes. No-op for blocks that carry no attributes (raw block, comment,
/// abbreviation definition).
fn stamp_source_line(node: &mut BlockNode, line: usize) {
    let slot: Option<&mut Option<Attrs>> = match node {
        BlockNode::Heading(n) => Some(&mut n.attrs),
        BlockNode::Paragraph(n) => Some(&mut n.attrs),
        BlockNode::ThematicBreak(n) => Some(&mut n.attrs),
        BlockNode::CodeBlock(n) => Some(&mut n.attrs),
        BlockNode::List(n) => Some(&mut n.attrs),
        BlockNode::BlockQuote(n) => Some(&mut n.attrs),
        BlockNode::Table(n) => Some(&mut n.attrs),
        BlockNode::Admonition(n) => Some(&mut n.attrs),
        BlockNode::Div(n) => Some(&mut n.attrs),
        BlockNode::DefinitionList(n) => Some(&mut n.attrs),
        BlockNode::Figure(n) => Some(&mut n.attrs),
        BlockNode::Extension(n) => Some(&mut n.attrs),
        BlockNode::BlockImage(n) => Some(&mut n.attrs),
        BlockNode::AbbreviationDef(_) | BlockNode::RawBlock(_) | BlockNode::Comment(_) => None,
    };
    let Some(opt) = slot else {
        return;
    };
    let attrs = opt.get_or_insert_with(Attrs::default);
    stamp_source_line_attr(attrs, line);
}

fn source_line_attrs(
    mut attrs: Option<Attrs>,
    line: Option<usize>,
    options: &Options<'_>,
) -> Option<Attrs> {
    if options.source_lines {
        if let Some(line) = line {
            let attrs = attrs.get_or_insert_with(Attrs::default);
            stamp_source_line_attr(attrs, line);
        }
    }
    attrs
}

fn stamp_source_line_attr(attrs: &mut Attrs, line: usize) {
    let key = "data-source-line";
    if !attrs.key_values.contains_key(key) {
        attrs.key_values.insert(key.to_string(), line.to_string());
        attrs.order.push(AttrSlot::Key(key.to_string()));
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
        // A direct block image (`{#id}\n![…](…)`) carries the leading attrs on
        // the `<img>` itself; the image's own inline attrs win on conflict (§15).
        BlockNode::BlockImage(img) => merge_leading_attrs(&mut img.attrs, attrs),
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
        // A trailing standalone block chains onto an inline literal, promoting a
        // bare literal to a `<span>` (`` !`x`{.a}{.b} `` -> class="a b"). Matches
        // carve-js, whose merge attaches to any non-text node.
        InlineNode::LiteralInline(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Footnote(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::CriticInsert(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::CriticDelete(n) => merge_attrs(&mut n.attrs, attrs),
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
            | InlineNode::LiteralInline(_)
            | InlineNode::Footnote(_)
            | InlineNode::CriticInsert(_)
            | InlineNode::CriticDelete(_)
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

/// Number of distinct `X}` pair closers tracked in `InlineBounds::delim_brace`.
/// Covers the critic markers (`+ - ~ #`) and the forced-emphasis delimiters
/// (`/ * _ ^ , = ~`); `~` is shared between critic substitution/strike.
const DELIM_BRACE_SLOTS: usize = 10;

/// Slot in `InlineBounds::delim_brace` for the leading byte of an `X}` pair, or
/// `None` for a byte that never opens a tracked pair.
#[inline]
fn delim_brace_slot(b: u8) -> Option<usize> {
    Some(match b {
        b'+' => 0,
        b'-' => 1,
        b'~' => 2,
        b'#' => 3,
        b'/' => 4,
        b'*' => 5,
        b'_' => 6,
        b'^' => 7,
        b',' => 8,
        b'=' => 9,
        _ => return None,
    })
}

/// Precomputed, per-inline-text closer positions used to short-circuit the
/// per-position construct scanners in `parse_inline_context`. Each scanner needs
/// a specific closing delimiter somewhere ahead; if the last occurrence of that
/// closer lies before the candidate opener, the scan could only walk to
/// end-of-text and fail, so it is skipped in O(1). Without these bounds, a run
/// of unclosed openers (`{+`×n, `[^`×n, `[x]{`×n, `<`×n, …) forces every opener
/// to re-scan to EOF -- classic O(n^2). Skipping only ever elides a scan that
/// would have returned `None`, so output stays byte-identical.
struct InlineBounds<'a> {
    /// Matching `]` index for every `[` (see `compute_bracket_matches`); empty
    /// when the text contains no bracket construct trigger.
    matches: &'a [usize],
    /// Index of the last `)` (inline link/image destination closer).
    last_close_paren: Option<usize>,
    /// Index of the last `}` (attribute-block closer).
    last_close_brace: Option<usize>,
    /// Index of the last `]` (footnote-ref / inline-footnote / extension closer).
    last_close_bracket: Option<usize>,
    /// Index of the last `>` (crossref / autolink closer).
    last_gt: Option<usize>,
    /// For each tracked `X}` pair, the index of the leading `X` of its LAST
    /// occurrence (see `delim_brace_slot`). Used by critic markup and forced
    /// emphasis, whose closers are two-byte `X}` pairs.
    delim_brace: [Option<usize>; DELIM_BRACE_SLOTS],
}

impl InlineBounds<'_> {
    /// True when a `]` occurs at or after `pos`.
    #[inline]
    fn has_bracket_from(&self, pos: usize) -> bool {
        self.last_close_bracket.is_some_and(|p| p >= pos)
    }

    /// True when a `>` occurs at or after `pos`.
    #[inline]
    fn has_gt_from(&self, pos: usize) -> bool {
        self.last_gt.is_some_and(|p| p >= pos)
    }

    /// True when an `X}` pair with leading byte `delim` occurs at or after
    /// `pos`.
    #[inline]
    fn has_delim_brace_from(&self, delim: u8, pos: usize) -> bool {
        delim_brace_slot(delim).is_some_and(|s| self.delim_brace[s].is_some_and(|p| p >= pos))
    }
}

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
    // Precompute every `[`-to-`]` match once (O(n)) so the per-`[` link /
    // reference / span / image parsers locate their closing bracket in O(1)
    // instead of re-scanning O(n) each. Without this, deeply nested balanced
    // links (`[[[...x]()]()...]`) are O(n^2). Only needed when bracket
    // constructs can actually fire.
    let has_brackets = has_link_trigger || text.contains("![");
    let bracket_matches = if has_brackets {
        compute_bracket_matches(bytes)
    } else {
        Vec::new()
    };
    // Last-occurrence positions of each mandatory closer, precomputed once so the
    // per-position scanners short-circuit in O(1) when their closer cannot lie
    // ahead (see InlineBounds). Each is gated on a cheap presence check; a
    // `rposition`/pair scan runs only when that byte actually appears. This is
    // what keeps runs of unclosed openers linear instead of O(n^2).
    let last_close_paren = if has_brackets {
        bytes.iter().rposition(|&b| b == b')')
    } else {
        None
    };
    let has_close_brace = text.contains('}');
    let last_close_brace = if has_close_brace {
        bytes.iter().rposition(|&b| b == b'}')
    } else {
        None
    };
    let last_close_bracket = if text.contains(']') {
        bytes.iter().rposition(|&b| b == b']')
    } else {
        None
    };
    let last_gt = if text.contains('>') {
        bytes.iter().rposition(|&b| b == b'>')
    } else {
        None
    };
    // For each tracked `X}` pair (`+} -} ~} #}` for critic, plus the forced-
    // emphasis delimiters), record the leading byte's LAST position. Built only
    // when a `}` exists at all, since every such pair ends in `}`.
    let mut delim_brace: [Option<usize>; DELIM_BRACE_SLOTS] = [None; DELIM_BRACE_SLOTS];
    if has_close_brace {
        for p in 0..bytes.len().saturating_sub(1) {
            if bytes[p + 1] == b'}' {
                if let Some(slot) = delim_brace_slot(bytes[p]) {
                    delim_brace[slot] = Some(p);
                }
            }
        }
    }
    let bounds = InlineBounds {
        matches: &bracket_matches,
        last_close_paren,
        last_close_brace,
        last_close_bracket,
        last_gt,
        delim_brace,
    };
    // Per-delimiter memo of the earliest opener position from which the emphasis
    // closer scan already failed. Once an opener of a given delimiter finds no
    // valid closer to EOF, every later opener of that delimiter also fails, so
    // the scan is skipped in O(1). Keeps `_a](`×n / `*a](`×n linear. See
    // cached_find_emphasis_close.
    let mut emphasis_no_close: [Option<usize>; EMPHASIS_DELIM_SLOTS] = [None; EMPHASIS_DELIM_SLOTS];
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];

        // Backslash escapes
        // A backslash at the very end of the content (no following byte) is a
        // hard break, mirroring the `\`-before-newline rule at end of input
        // (`para\` at EOF -> `<br>`), matching djot and the cheatsheet.
        if c == b'\\' && i + 1 >= bytes.len() {
            flush_text(&mut out, &mut buf);
            out.push(InlineNode::HardBreak);
            i += 1;
            continue;
        }
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
                // Push the bare code span. A trailing inline attribute block
                // (`` `code`{.cls} ``) is attached by the general attr-merge in
                // the main loop, which runs AFTER the forced-emphasis / critic
                // checks -- so `` `c`{_u_} `` is a code span + forced underline,
                // not a bogus `_u_` attribute. Matches carve-js / carve-php.
                out.push(InlineNode::Code(value, None));
                i += consumed;
                continue;
            }
        }

        if c == b'$' {
            if let Some((math, consumed)) = parse_math(bytes, i, &bounds) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Math(math));
                i += consumed;
                continue;
            }
        }

        if c == b'{' {
            if let Some((critic, consumed)) =
                parse_critic_markup(bytes, i, options, in_footnote, &bounds)
            {
                flush_text(&mut out, &mut buf);
                out.push(critic);
                i += consumed;
                continue;
            }
            // Forced intraword emphasis `{X…X}` — tried before inline attribute
            // blocks, matching the reference scan order.
            if let Some((mut node, consumed)) =
                parse_forced_emphasis(bytes, i, options, in_footnote, &bounds)
            {
                let mut consumed = consumed;
                // A trailing `{...}` attribute block attaches to the forced span,
                // exactly like a bare span (`{*x*}{.c}` -> <strong class="c">x</strong>).
                if bytes.get(i + consumed) == Some(&b'{') {
                    if let Some((attrs, next)) =
                        read_attrs_at(bytes, i + consumed, bounds.last_close_brace)
                    {
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
                if let Some((attrs, next)) = read_attrs_at(bytes, i, bounds.last_close_brace) {
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

        // Inline literal (§27): a `!` prefix on a verbatim code span, mirroring
        // the `$`-math prefix above. The span content is captured verbatim,
        // later HTML-escaped and emitted by every renderer with the `<code>`
        // wrapper dropped; a trailing `{…}` attaches below via the general
        // attr-merge as an ordinary inline attribute block (no special
        // first-token sigil). Like math it requires a CLOSED span — a bare `!`
        // before an unclosed run stays literal and the run becomes an ordinary
        // (unclosed) code span. Tried before the image case, which needs `[`.
        if c == b'!' && bytes.get(i + 1) == Some(&b'`') {
            if let Some((lit, consumed)) = parse_literal_inline(bytes, i) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::LiteralInline(lit));
                i += consumed;
                continue;
            }
        }

        // Image: ![alt](src), then reference image ![alt][ref] / ![alt][].
        if c == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some((img, consumed)) = parse_image_at(bytes, i, &bounds) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Image(img));
                i += consumed;
                continue;
            }
            if let Some((img, consumed)) = parse_reference_image(bytes, i, &bounds) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Image(img));
                i += consumed;
                continue;
            }
        }

        // Inline link: [text](href)
        if c == b'[' {
            if !in_footnote {
                if let Some((footnote, consumed)) = parse_footnote_ref(bytes, i, &bounds) {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Footnote(footnote));
                    i += consumed;
                    continue;
                }
            }
            if has_link_trigger {
                if let Some((link, consumed)) =
                    parse_inline_link_with_options(bytes, i, options, in_footnote, &bounds)
                {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Link(link));
                    i += consumed;
                    continue;
                }
                if let Some((link, consumed)) =
                    parse_reference_link(bytes, i, options, in_footnote, &bounds)
                {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Link(link));
                    i += consumed;
                    continue;
                }
                if let Some((span, consumed)) = parse_span(bytes, i, options, in_footnote, &bounds)
                {
                    flush_text(&mut out, &mut buf);
                    out.push(InlineNode::Span(span));
                    i += consumed;
                    continue;
                }
            }
        }

        if c == b'<' {
            if let Some((crossref, consumed)) = parse_crossref(text, i, &bounds) {
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
            if let Some((autolink, consumed)) = parse_autolink(text, i, &bounds) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::AutoLink(autolink));
                i += consumed;
                continue;
            }
        }

        // Inline extension: :name[content]
        if c == b':' {
            if let Some((node, consumed)) =
                parse_inline_extension(bytes, i, options, in_footnote, &bounds)
            {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Extension(node));
                i += consumed;
                continue;
            }
            if let Some((symbol, consumed)) = parse_symbol(text, i, &bounds) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Symbol(symbol));
                i += consumed;
                continue;
            }
        }

        // Inline footnote `^[content]`. A `^` anywhere else is literal text
        // (there is no bare superscript), so `^^[x]` is a literal `^` + a note.
        if !in_footnote && c == b'^' && bytes.get(i + 1) == Some(&b'[') {
            if let Some((footnote, consumed)) = parse_inline_footnote(bytes, i, options, &bounds) {
                flush_text(&mut out, &mut buf);
                out.push(InlineNode::Footnote(footnote));
                i += consumed;
                continue;
            }
        }

        // Bold-italic, sub, highlight, then single-char emphasis
        if let Some((mut node, consumed)) =
            match_emphasis(bytes, i, options, in_footnote, &mut emphasis_no_close)
        {
            let mut consumed = consumed;
            if bytes.get(i + consumed) == Some(&b'{') {
                if let Some((attrs, next)) =
                    read_attrs_at(bytes, i + consumed, bounds.last_close_brace)
                {
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
    bounds: &InlineBounds<'_>,
) -> Option<(InlineNode, usize)> {
    // Match the two-byte opener on raw bytes -- validating `bytes[start..]` as
    // UTF-8 here would be O(n) at every `{`, i.e. O(n^2) over a run of critic
    // openers. Only the matched inner slice (up to the closing pair) is
    // validated, once a pair is located.
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let content_start = start + 2;
    // Each critic form closes on a two-byte `X}` pair; if that pair does not lie
    // ahead, the `find_seq` could only scan to end-of-text and fail, so bail in
    // O(1). Keeps `{+`×n / `{-`×n / `{~ }`×n (no closing pair) linear.
    match bytes.get(start + 1).copied()? {
        b'+' => {
            if !bounds.has_delim_brace_from(b'+', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"+}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticInsert(CriticInsert {
                    children: parse_inline_context(inner, options, false, in_footnote),
                    attrs: None,
                }),
                pair + 2 - start,
            ))
        }
        b'-' => {
            if !bounds.has_delim_brace_from(b'-', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"-}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticDelete(CriticDelete {
                    children: parse_inline_context(inner, options, false, in_footnote),
                    attrs: None,
                }),
                pair + 2 - start,
            ))
        }
        b'~' => {
            // A critic substitution is `{~old~>new~}`: the `~>` separator must
            // sit within this `{~ … ~}`. Without it (`{~view~}`), this is not
            // critic markup -- it falls through to forced strike emphasis.
            if !bounds.has_delim_brace_from(b'~', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"~}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            let sep = inner.find("~>")?;
            Some((
                InlineNode::CriticSubstitute(CriticSubstitute {
                    old_text: inner[..sep].to_string(),
                    new_text: inner[sep + 2..].to_string(),
                }),
                pair + 2 - start,
            ))
        }
        b'#' => {
            if !bounds.has_delim_brace_from(b'#', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"#}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticComment(CriticComment {
                    text: inner.to_string(),
                }),
                pair + 2 - start,
            ))
        }
        _ => None,
    }
}

fn parse_footnote_ref(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'[') || bytes.get(start + 1) != Some(&b'^') {
        return None;
    }
    // The id runs to the closing `]`; with no `]` ahead the scan could only walk
    // to end-of-text and fail, so bail in O(1) (keeps `[^`×n linear).
    if !bounds.has_bracket_from(start) {
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
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
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
    bounds: &InlineBounds<'_>,
) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'^') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    // The body runs to a balancing `]`; with no `]` ahead the bracket scan could
    // only walk to end-of-text and fail, so bail in O(1) (keeps `^[`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let (content, after_bracket) = read_bracketed(bytes, start + 1)?;
    if content.trim().is_empty() {
        return None;
    }
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
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
    if format_start == i {
        return None;
    }
    let format = std::str::from_utf8(&bytes[format_start..i]).ok()?;
    // The format must be a valid `format_name` (an identifier: letter/`_`
    // start, then letter/digit/`_`/`-`), per grammar §20. Anything else
    // (`{=h=}`, `{==h==}`, `{=text/html}`) is NOT a raw inline -- it falls back
    // to a plain code span plus forced-emphasis / literal text, matching
    // carve-js / carve-php. Without this rs greedily consumed the code span and
    // dropped its content for a bogus format.
    let mut fc = format.bytes();
    let valid = matches!(fc.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && fc.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        return None;
    }
    Some((
        RawInline {
            format: format.to_string(),
            content: value.to_string(),
        },
        i + 1 - start,
    ))
}

/// Inline literal (`` !`…` ``, grammar PART 9 §27, `literal_inline = '!',
/// code_span`). A `!` PREFIX on a verbatim code span, mirroring `parse_math`'s
/// `$` prefix: the maximal backtick run captures the content verbatim, which is
/// later HTML-escaped and emitted by every renderer with the `<code>` wrapper
/// dropped. A CLOSED span is required — a `!` before an unclosed run returns
/// `None`, leaving the `!` literal and the run to become an ordinary (unclosed)
/// code span, exactly as `$` before an unclosed run behaves.
///
/// Returns a bare literal; a trailing `{…}` is the ORDINARY inline attribute
/// block and is attached by the general attr-merge in the scanner (same path a
/// bare code span uses), so `` !`x`{.ipa} `` and chained `` !`x`{.a}{.b} ``
/// both work without any special first-token handling here.
fn parse_literal_inline(bytes: &[u8], start: usize) -> Option<(LiteralInline, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'`') {
        return None;
    }
    let tick = start + 1;
    // Require a CLOSED span, like `$`-math in carve-js. `parse_inline_code`
    // itself accepts an unclosed opener (consuming to the end of the block), so
    // the closedness is checked explicitly here: a `!` before an unclosed run
    // stays literal and the run becomes an ordinary (unclosed) code span.
    if !inline_code_is_closed(bytes, tick) {
        return None;
    }
    let (content, code_consumed) = parse_inline_code(bytes, tick)?;
    Some((
        LiteralInline {
            content,
            attrs: None,
        },
        tick + code_consumed - start,
    ))
}

/// True iff a verbatim code span opening at `start` (a backtick) has a matching
/// equal-length closing run — i.e. it is CLOSED rather than an opener that runs
/// unclosed to the end of the block. Used to gate the inline literal (§27) to
/// closed spans only, matching the `$`-math prefix.
fn inline_code_is_closed(bytes: &[u8], start: usize) -> bool {
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    if open_len == 0 {
        return false;
    }
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
            return true;
        }
    }
    false
}

fn parse_math(bytes: &[u8], start: usize, bounds: &InlineBounds<'_>) -> Option<(Math, usize)> {
    let display = bytes.get(start + 1) == Some(&b'$');
    let tick = if display { start + 2 } else { start + 1 };
    if bytes.get(tick) != Some(&b'`') {
        return None;
    }
    // Math reuses a code span for its verbatim body (grammar `math_inline =
    // '$', code_span`): a MAXIMAL backtick run opens and an equal-length run
    // closes, so `$``a``` and `$`a``b`` behave like the code span `` `a``b` ``.
    let (content, code_consumed) = parse_inline_code(bytes, tick)?;
    // Empty verbatim content is NOT math (`$``` / `$$```): the `$` stays literal
    // and the backtick pair is an empty code span. Matches carve-js / carve-php.
    if content.is_empty() {
        return None;
    }
    let end = tick + code_consumed;
    // A trailing attribute block attaches to the math span (math reuses the
    // code-span attribute slot), EXCEPT `{=format}`, the raw-inline form,
    // which is code-span-only and not inherited by math -- leave it literal.
    let mut attrs = None;
    let mut after = end;
    if bytes.get(end) == Some(&b'{') && bytes.get(end + 1) != Some(&b'=') {
        if let Some((parsed, next)) = read_attrs_at(bytes, end, bounds.last_close_brace) {
            attrs = Some(parsed);
            after = next;
        }
    }
    Some((
        Math {
            attrs,
            display,
            content,
        },
        after - start,
    ))
}

fn parse_reference_link(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
) -> Option<(Link, usize)> {
    let text_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_text = text_close + 1;
    if bytes.get(after_text) != Some(&b'[') {
        return None;
    }
    let label_close = bracketed_close(bytes, after_text, bounds.matches)?;
    let after_label = label_close + 1;
    // Both brackets are present, so materializing their labels now costs O(1)
    // per accepted reference rather than per candidate `[`.
    let text = std::str::from_utf8(&bytes[start + 1..text_close])
        .ok()?
        .to_string();
    let label = std::str::from_utf8(&bytes[after_text + 1..label_close])
        .ok()?
        .to_string();
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
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
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
            from_crossref: false,
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
            | b':'
            | b';'
            | b'$'
            | b'&'
            | b'?'
    )
}

// The closed-verbatim-span single-space strip: one leading and one trailing
// space are removed when the content BOTH begins and ends with a space -- but
// NOT when it consists entirely of spaces. The all-space guard matches the
// executable spec's `codeText()` and the CommonMark rule it derives from
// ("...but does not consist entirely of space characters"). Without the guard
// `` `  ` `` would strip to the empty string, and an empty verbatim span has no
// representable Carve source (a bare `` `` `` reparses as a two-backtick
// opener), so `carve fmt` could not round-trip it. Shared by the closed and
// unclosed verbatim paths so the two cannot drift apart.
fn strip_verbatim_padding(raw: &str) -> &str {
    if raw.starts_with(' ') && raw.ends_with(' ') && !raw.chars().all(|c| c == ' ') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
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
            return Some((strip_verbatim_padding(raw).to_string(), i - start));
        }
        // Different length closer — keep scanning past it
    }
    // No matching closer: an unclosed verbatim opener is opaque to the end of
    // the text (matches djot / carve-php / carve-js).
    let raw = std::str::from_utf8(&bytes[content_start..]).ok()?;
    Some((strip_verbatim_padding(raw).to_string(), bytes.len() - start))
}

fn parse_image_at(bytes: &[u8], start: usize, bounds: &InlineBounds<'_>) -> Option<(Image, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let alt_close = bracketed_close(bytes, start + 1, bounds.matches)?;
    let after_alt = alt_close + 1;
    if bytes.get(after_alt) != Some(&b'(') {
        return None;
    }
    let (src, title, after_paren) =
        read_link_target(bytes, after_alt + 1, bounds.last_close_paren)?;
    // Only a valid `(target)` reaches here, so the alt copy is deferred off the
    // failing-`![...]()` path that would otherwise be O(n) per position.
    let alt = std::str::from_utf8(&bytes[start + 2..alt_close])
        .ok()?
        .to_string();
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
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
            ref_label: None,
            raw_ref: None,
        },
        after - start,
    ))
}

/// Parse a reference image `![alt][ref]` / collapsed `![alt][]` — the image
/// form of a reference link, mirroring `parse_reference_link`. `src` is empty
/// until `resolve_reference_links` fills it from the matching `[label]: url`
/// def; the full form allows an empty alt (label = ref), collapsed needs a
/// non-empty alt (label = alt).
fn parse_reference_image(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Image, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let alt_close = bracketed_close(bytes, start + 1, bounds.matches)?;
    let after_alt = alt_close + 1;
    if bytes.get(after_alt) != Some(&b'[') {
        return None;
    }
    let label_close = bracketed_close(bytes, after_alt, bounds.matches)?;
    let after_label = label_close + 1;
    let alt = std::str::from_utf8(&bytes[start + 2..alt_close])
        .ok()?
        .to_string();
    let label = std::str::from_utf8(&bytes[after_alt + 1..label_close])
        .ok()?
        .to_string();
    // Collapsed `![alt][]` reuses the alt as the label, so it needs a non-empty
    // alt; the full `![alt][ref]` form accepts an empty alt (label = ref).
    if label.is_empty() && alt.is_empty() {
        return None;
    }
    let ref_label = if label.is_empty() { alt.clone() } else { label };
    let mut attrs = None;
    let mut after = after_label;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Image {
            attrs,
            src: String::new(),
            alt,
            title: None,
            ref_label: Some(ref_label),
            raw_ref: Some(std::str::from_utf8(&bytes[start..after]).ok()?.to_string()),
        },
        after - start,
    ))
}

fn parse_inline_link_with_options(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
) -> Option<(Link, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let text_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_bracket = text_close + 1;
    if bytes.get(after_bracket) != Some(&b'(') {
        return None;
    }
    let (href, title, after_paren) =
        read_link_target(bytes, after_bracket + 1, bounds.last_close_paren)?;
    // The label is copied only once a valid `(target)` follows; the failing
    // `[...]()` path (empty target) returns above without allocating, which is
    // what keeps `[[[...x]()]()...]()` linear instead of quadratic.
    let text = std::str::from_utf8(&bytes[start + 1..text_close])
        .ok()?
        .to_string();
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
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
            from_crossref: false,
        },
        after - start,
    ))
}

fn parse_inline_extension(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
) -> Option<(InlineExtension, usize)> {
    if bytes.get(start) != Some(&b':') {
        return None;
    }
    // The content runs to the first `]`; with no `]` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `:a[`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let mut i = start + 1;
    let name_start = i;
    // `extension_name = identifier`: must start with a letter or `_` -- a
    // digit-first name (`:1[x]`) is invalid and the whole construct stays
    // literal. (`:a1[x]` is fine; digits are allowed after the first char.)
    match bytes.get(i) {
        Some(b) if b.is_ascii_alphabetic() || *b == b'_' => {}
        _ => return None,
    }
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
    {
        i += 1;
    }
    if i == name_start || bytes.get(i) != Some(&b'[') {
        return None;
    }
    let name = std::str::from_utf8(&bytes[name_start..i]).ok()?.to_string();
    // `extension_content = {character - ']'}`: the content runs to the FIRST
    // unescaped `]` and does not balance nested brackets (`:foo[a [b] c]` ->
    // `<span class="ext-foo">a [b</span> c]`).
    let (content, after_bracket) = read_to_first_bracket(bytes, i)?;
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
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
    bounds: &InlineBounds<'_>,
) -> Option<(Span, usize)> {
    let content_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_bracket = content_close + 1;
    if bytes.get(after_bracket) != Some(&b'{') {
        return None;
    }
    let (attrs, after_attrs) = read_attrs_at(bytes, after_bracket, bounds.last_close_brace)
        .or_else(|| read_empty_attrs_at(bytes, after_bracket))?;
    // Absorb a CHAIN of adjacent attribute blocks (`[x]{.a}{.b}` ->
    // class="a b"), accumulating classes (§15). A non-attribute `{...}` (e.g.
    // an empty `{}`) reads as None and is left literal, so `[x]{}{}` keeps the
    // trailing `{}` -- matching carve-php / carve-js.
    let mut attrs = Some(attrs);
    let mut after_attrs = after_attrs;
    while bytes.get(after_attrs) == Some(&b'{') {
        match read_attrs_at(bytes, after_attrs, bounds.last_close_brace) {
            Some((more, next)) => {
                merge_attrs(&mut attrs, more);
                after_attrs = next;
            }
            None => break,
        }
    }
    // Only a valid `{attrs}` follow reaches here, so the content copy stays off
    // the failing `[...]` path (e.g. `[...]()` never gets past the `{` check).
    let content = std::str::from_utf8(&bytes[start + 1..content_close])
        .ok()?
        .to_string();
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

fn parse_symbol(text: &str, pos: usize, bounds: &InlineBounds<'_>) -> Option<(Symbol, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(pos) != Some(&b':') {
        return None;
    }
    if pos > 0 {
        let prev = bytes[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    // The first name char is a letter, digit, `+` or `-` (so the reaction
    // shortcodes `:+1:` / `:-1:` parse), but never `_`: `:_x_:` would steal
    // from underline. Scanning the symbol at the opening `:` also gives it
    // precedence over smart typography, so `:+-:` is the symbol `+-`, not a
    // `±` between colons (grammar PART 9 §7).
    let first = *bytes.get(pos + 1)?;
    if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
        return None;
    }
    let mut len = 1;
    while let Some(&b) = bytes.get(pos + 1 + len) {
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
            len += 1;
        } else {
            break;
        }
    }
    let close_pos = pos + 1 + len;
    if bytes.get(close_pos) != Some(&b':') {
        return None;
    }
    let (attrs, consumed) =
        if let Some((attrs, next)) = read_attrs_at(bytes, close_pos + 1, bounds.last_close_brace) {
            (Some(attrs), next - pos)
        } else {
            (None, len + 2)
        };
    Some((
        Symbol {
            name: text[pos + 1..close_pos].to_string(),
            attrs,
        },
        consumed,
    ))
}

fn parse_autolink(text: &str, pos: usize, bounds: &InlineBounds<'_>) -> Option<(AutoLink, usize)> {
    // The target runs to the closing `>`; with no `>` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `<`×n linear).
    if !bounds.has_gt_from(pos) {
        return None;
    }
    let rest = text.get(pos..)?;
    let close = rest.find('>')?;
    let target = &rest[1..close];
    let mut attrs = None;
    let mut consumed = close + 1;
    let bytes = text.as_bytes();
    if bytes.get(pos + consumed) == Some(&b'{') {
        if let Some((parsed_attrs, next)) =
            read_attrs_at(bytes, pos + consumed, bounds.last_close_brace)
        {
            attrs = Some(parsed_attrs);
            consumed = next - pos;
        }
    }
    if is_url_autolink_target(target) {
        return Some((
            AutoLink {
                attrs,
                href: target.to_string(),
                text: target.to_string(),
            },
            consumed,
        ));
    }
    if is_email_autolink_target(target) {
        return Some((
            AutoLink {
                attrs,
                href: format!("mailto:{target}"),
                text: target.to_string(),
            },
            consumed,
        ));
    }
    None
}

/// `email_char = letter | digit | '.' | '-' | '_' | '+'` (grammar.ebnf).
/// Note `:` is deliberately NOT an email char.
fn is_email_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+')
}

/// `email_autolink = {email_char}+ '@' {email_char}+ '.' {letter}+`.
/// The local part and domain are both non-empty runs of `email_char`, and the
/// domain MUST end in `.` followed by a TLD of one or more ASCII letters. So
/// `<a@b>` (no TLD) and `<x@y:z>` (`:` is not an email_char) stay literal,
/// while `<a@b.com>` is a `mailto:` link.
fn is_email_autolink_target(target: &str) -> bool {
    let bytes = target.as_bytes();
    let Some(at) = bytes.iter().position(|&b| b == b'@') else {
        return false;
    };
    let local = &bytes[..at];
    let domain = &bytes[at + 1..];
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if !local.iter().all(|&b| is_email_char(b)) || !domain.iter().all(|&b| is_email_char(b)) {
        return false;
    }
    // A single `@` only: `@` is not an email_char, so any later `@` already
    // failed the `is_email_char` check above.
    // Domain must end in `.` + TLD ({letter}+).
    let Some(dot) = domain.iter().rposition(|&b| b == b'.') else {
        return false;
    };
    let tld = &domain[dot + 1..];
    if dot == 0 {
        // No host label before the final dot.
        return false;
    }
    !tld.is_empty() && tld.iter().all(|&b| b.is_ascii_alphabetic())
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

fn parse_crossref(text: &str, pos: usize, bounds: &InlineBounds<'_>) -> Option<(CrossRef, usize)> {
    // The target runs to the closing `>`; with no `>` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `</#`×n linear).
    if !bounds.has_gt_from(pos) {
        return None;
    }
    let rest = text.get(pos..)?;
    let inner = rest.strip_prefix("</#")?;
    let close = inner.find('>')?;
    let target = &inner[..close];
    if target.is_empty() || target.bytes().any(|b| b.is_ascii_whitespace()) {
        return None;
    }
    Some((
        CrossRef {
            target: target.to_string(),
        },
        close + 4,
    ))
}

/// O(1) lookup of the matching `]` for the `[` at `start` using a precomputed
/// match table (see `compute_bracket_matches`). `start` must index a `[`.
/// Returns the byte index of the closing `]` (a borrow position, no allocation).
///
/// Callers materialize the bracket label only after the follow (target `(`,
/// reference `[`, span `{`) validates, so a `[` whose construct never completes
/// stays O(1) instead of eagerly copying its label at every position -- the
/// difference between linear and quadratic parsing on pathological input like
/// `[[[...x]()]()...]()`.
fn bracketed_close(bytes: &[u8], start: usize, matches: &[usize]) -> Option<usize> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let close = *matches.get(start)?;
    if close == NO_BRACKET_MATCH {
        return None;
    }
    Some(close)
}

/// Read `[...]` content for an inline extension: the content runs to the
/// FIRST `]` and does NOT balance nested brackets or honor escapes
/// (`extension_content = {character - ']'}`, carve-js regex `\[([^\]]*)\]`).
/// Returns the content and the index just past the closing `]`.
fn read_to_first_bracket(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let content_start = start + 1;
    let mut i = content_start;
    while i < bytes.len() {
        if bytes[i] == b']' {
            let text = std::str::from_utf8(&bytes[content_start..i])
                .ok()?
                .to_string();
            return Some((text, i + 1));
        }
        i += 1;
    }
    None
}

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

/// Sentinel in a bracket-match table meaning "this `[` has no matching `]`".
const NO_BRACKET_MATCH: usize = usize::MAX;

/// Precompute, in a single O(n) pass, the matching `]` index for every `[` in
/// `bytes`, mirroring `read_bracketed`'s scan rules exactly (backslash escapes
/// skip two bytes; an unclosed inline-code span is opaque to end of text and
/// closes no bracket; a `[` increments depth, the first `]` at depth>0
/// decrements it, and the `]` at depth 0 matches the most recent unmatched
/// `[`). The returned table lets the per-`[` link/reference/span parsers find
/// their closing bracket in O(1) instead of re-scanning O(n) at every position,
/// which removes the O(n^2) blowup on deeply nested balanced links
/// (`[[[...x]()]()...]`). Output is unchanged: a lookup yields the same close
/// index `read_bracketed` would return by scanning.
///
/// Entry `i` is meaningful only when `bytes[i] == b'['`; it holds the matching
/// `]` index, or `NO_BRACKET_MATCH` when that `[` never closes.
fn compute_bracket_matches(bytes: &[u8]) -> Vec<usize> {
    let mut matches = vec![NO_BRACKET_MATCH; bytes.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'`' => match skip_code_span(bytes, i) {
                // An unclosed code span is opaque to end of text: no later `]`
                // can close a bracket, so every still-open `[` stays unmatched.
                Some(next) => i = next,
                None => break,
            },
            b'[' => {
                stack.push(i);
                i += 1;
            }
            b']' => {
                if let Some(open) = stack.pop() {
                    matches[open] = i;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    matches
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

fn read_link_target(
    bytes: &[u8],
    start: usize,
    last_close_paren: Option<usize>,
) -> Option<(String, Option<String>, usize)> {
    // A valid inline target MUST end with a `)` (checked below). If no `)`
    // occurs at or after `start`, the destination scan can only walk to
    // end-of-text and then fail -- so short-circuit here in O(1). Without this,
    // a run like `[a](`×n (no `)` anywhere) makes every `[` re-scan to EOF,
    // which is O(n^2). `last_close_paren` is the index of the last `)` in the
    // whole text, precomputed once by the caller; skipping only ever elides a
    // call that would have returned `None`, keeping output byte-identical.
    if last_close_paren.map_or(true, |p| start > p) {
        return None;
    }
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
    if i == href_start {
        return None;
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
    no_close: &mut [Option<usize>; EMPHASIS_DELIM_SLOTS],
) -> Option<(InlineNode, usize)> {
    let c = bytes[i];

    // /*bold italic*/ -- a combined Strong>Emphasis span. The grammar
    // (`boldItalic = "/*" ~spaceOrEnd biInner+ "*/"`) requires the content to
    // start with a non-space char and be non-empty; carve-php additionally
    // rejects a closer whose content ends in whitespace, scanning on to a later
    // `*/`. Empty / space-bounded content is NOT bold-italic and falls through
    // to ordinary `/` emphasis below.
    if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
        let start = i + 2;
        // Opener guard: the first content byte must exist and not be whitespace.
        if bytes.get(start).is_some_and(|b| !b.is_ascii_whitespace()) {
            let mut search = start;
            while let Some(close) = find_seq(bytes, search, b"*/") {
                // Reject empty content or content ending in whitespace; keep
                // scanning for a later closer, matching carve-php.
                if close > start && !bytes[close - 1].is_ascii_whitespace() {
                    let inner = std::str::from_utf8(&bytes[start..close]).ok()?;
                    return Some((
                        InlineNode::Emphasis(Emphasis {
                            attrs: None,
                            kind: EmphasisKind::BoldItalic,
                            children: parse_inline_context(inner, options, false, in_footnote),
                        }),
                        close + 2 - i,
                    ));
                }
                search = close + 1;
            }
        }
    }
    // Single-char delimiters. Highlight `=` is single-char like the rest; a
    // doubled `==` is therefore literal by same-delimiter adjacency (checked
    // below), exactly like `**x**`. There is NO bare `^`/`,` delimiter:
    // superscript and subscript exist only in the braced forms `{^x^}`/`{,x,}`
    // (grammar PART 9 §9 rationale note) -- a bare caret or comma is literal.
    let kind = match c {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'~' => EmphasisKind::Strike,
        b'=' => EmphasisKind::Highlight,
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
    let close = cached_find_emphasis_close(bytes, i + 1, delim, no_close)?;
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
    for blocks in doc.footnote_defs.values() {
        collect_heading_titles(blocks, &mut counts, &mut titles, lowercase_ids);
    }
    let mut caption_counts = BTreeMap::new();
    number_captioned_blocks(&mut doc.children, &mut caption_counts, &mut titles);
    for blocks in doc.footnote_defs.values_mut() {
        number_captioned_blocks(blocks, &mut caption_counts, &mut titles);
    }
    let index = crossref_index(titles);
    for block in &mut doc.children {
        resolve_crossrefs_block(block, &index);
    }
    for blocks in doc.footnote_defs.values_mut() {
        for block in blocks {
            resolve_crossrefs_block(block, &index);
        }
    }
}

fn heading_index(
    children: &[BlockNode],
    footnote_defs: &BTreeMap<String, Vec<BlockNode>>,
    lowercase_ids: bool,
) -> CrossrefIndex {
    let mut counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    collect_heading_titles(children, &mut counts, &mut titles, lowercase_ids);
    for blocks in footnote_defs.values() {
        collect_heading_titles(blocks, &mut counts, &mut titles, lowercase_ids);
    }
    crossref_index(titles)
}

fn crossref_index(titles: BTreeMap<String, String>) -> CrossrefIndex {
    // Case-folded index of known ids -> actual (case-preserved) id. First
    // occurrence wins, so a duplicate that only differs in case does not shadow
    // the earlier heading. Used as a fallback when an exact match fails, so a
    // lowercase reference resolves to a `Getting-Started` heading and the
    // emitted href uses the ACTUAL id.
    let mut folded: BTreeMap<String, String> = BTreeMap::new();
    for id in titles.keys() {
        folded.entry(case_fold(id)).or_insert_with(|| id.clone());
    }
    CrossrefIndex { titles, folded }
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

fn resolve_reference_links(
    doc: &mut Document,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
    preserve_unresolved: bool,
) {
    for block in &mut doc.children {
        resolve_reference_links_block(block, defs, heading_index, preserve_unresolved);
    }
    for blocks in doc.footnote_defs.values_mut() {
        for block in blocks {
            resolve_reference_links_block(block, defs, heading_index, preserve_unresolved);
        }
    }
}

fn resolve_reference_links_block(
    block: &mut BlockNode,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
    preserve_unresolved: bool,
) {
    match block {
        BlockNode::Heading(h) => resolve_reference_links_inline(
            &mut h.children,
            defs,
            heading_index,
            preserve_unresolved,
        ),
        BlockNode::Paragraph(p) => resolve_reference_links_inline(
            &mut p.children,
            defs,
            heading_index,
            preserve_unresolved,
        ),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    resolve_reference_links_block(child, defs, heading_index, preserve_unresolved);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                resolve_reference_links_block(child, defs, heading_index, preserve_unresolved);
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                resolve_reference_links_inline(caption, defs, heading_index, preserve_unresolved);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    resolve_reference_links_inline(
                        &mut cell.children,
                        defs,
                        heading_index,
                        preserve_unresolved,
                    );
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                resolve_reference_links_block(child, defs, heading_index, preserve_unresolved);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                resolve_reference_links_block(child, defs, heading_index, preserve_unresolved);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    resolve_reference_links_inline(term, defs, heading_index, preserve_unresolved);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        resolve_reference_links_block(
                            child,
                            defs,
                            heading_index,
                            preserve_unresolved,
                        );
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            resolve_reference_links_inline(
                &mut f.caption,
                defs,
                heading_index,
                preserve_unresolved,
            );
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        resolve_reference_links_block(
                            child,
                            defs,
                            heading_index,
                            preserve_unresolved,
                        );
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        resolve_reference_links_inline(
                            caption,
                            defs,
                            heading_index,
                            preserve_unresolved,
                        );
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            resolve_reference_links_inline(
                                &mut cell.children,
                                defs,
                                heading_index,
                                preserve_unresolved,
                            );
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

fn resolve_reference_links_inline(
    nodes: &mut Vec<InlineNode>,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
    preserve_unresolved: bool,
) {
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
                    } else if is_collapsed_reference(l) {
                        // Implicit heading reference: slugify the label the same
                        // way heading ids are generated, then resolve against the
                        // heading slug index (resolve() handles case-folding).
                        let slug = slugify_parse(label, false);
                        if let Some((actual_id, _)) = heading_index.resolve(&slug) {
                            l.href = format!("#{actual_id}");
                            l.title = None;
                            l.ref_label = None;
                            l.raw_ref = None;
                            out.push(node);
                        } else if preserve_unresolved {
                            out.push(node);
                        } else {
                            out.push(InlineNode::Text(l.raw_ref.clone().unwrap_or_default()));
                        }
                    } else if preserve_unresolved {
                        out.push(node);
                    } else {
                        out.push(InlineNode::Text(l.raw_ref.clone().unwrap_or_default()));
                    }
                } else {
                    resolve_reference_links_inline(
                        &mut l.children,
                        defs,
                        heading_index,
                        preserve_unresolved,
                    );
                    out.push(node);
                }
            }
            InlineNode::Emphasis(e) => {
                resolve_reference_links_inline(
                    &mut e.children,
                    defs,
                    heading_index,
                    preserve_unresolved,
                );
                out.push(node);
            }
            InlineNode::Span(s) => {
                resolve_reference_links_inline(
                    &mut s.children,
                    defs,
                    heading_index,
                    preserve_unresolved,
                );
                out.push(node);
            }
            InlineNode::Extension(e) => {
                resolve_reference_links_inline(
                    &mut e.children,
                    defs,
                    heading_index,
                    preserve_unresolved,
                );
                out.push(node);
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        resolve_reference_links_inline(
                            prefix,
                            defs,
                            heading_index,
                            preserve_unresolved,
                        );
                    }
                    if let Some(locator) = &mut item.locator {
                        resolve_reference_links_inline(
                            locator,
                            defs,
                            heading_index,
                            preserve_unresolved,
                        );
                    }
                }
                out.push(node);
            }
            InlineNode::Image(img) => {
                if let Some(label) = &img.ref_label {
                    if let Some(def) = defs.get(label) {
                        img.src = def.href.clone();
                        img.title = def.title.clone();
                        img.ref_label = None;
                        img.raw_ref = None;
                        out.push(node);
                    } else if preserve_unresolved {
                        out.push(node);
                    } else {
                        // Unresolved image ref -> literal source. An image ref
                        // never matches heading text (unlike a link ref).
                        out.push(InlineNode::Text(img.raw_ref.clone().unwrap_or_default()));
                    }
                } else {
                    out.push(node);
                }
            }
            _ => out.push(node),
        }
    }
    *nodes = out;
}

/// Promote a paragraph whose sole child is a (resolved) image to a block-level
/// image, matching the standalone inline-image rule (`detect_block_image`) and
/// carve-php. Recurses into container blocks. An unresolved reference image
/// already became a `Text` node, so its paragraph is left untouched.
/// Length (in bytes) of a leading `^` + one-or-more whitespace caption marker
/// (`RE_CAPTION = /^\^\s+/`), or `None` when the text does not open a caption.
/// A caption line mirrors a heading's first line (`detect_heading`): `^` +
/// one-or-more literal spaces (the grammar delimiter is a space, not a tab) +
/// non-empty content. Returns the caption text with leading spaces skipped and
/// trailing whitespace trimmed. None when there is no space after `^`, the
/// delimiter is a tab, or the content is empty (`^ ` alone).
fn caption_content(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'^') || bytes.get(1) != Some(&b' ') {
        return None;
    }
    let mut start = 1;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    // Verbatim content (see detect_heading): a caption folds continuation lines
    // like a paragraph, so first-line trailing is interior; only the final
    // assembled caption is trailing-stripped (§756). The gate still tests a
    // trailing-stripped view so `^ ` / `^  ` / `^ \t` are not captions.
    let text = &line[start..];
    if trim_ascii_end(text).is_empty() {
        return None;
    }
    Some(text)
}

/// Byte length of a caption marker (`^` + one-or-more spaces) at the START of an
/// inline Text node, used when splitting a reference-image figure caption off
/// its leading text. Mirrors `caption_content`'s delimiter: a space, not a tab.
/// Content-emptiness is decided separately (`caption_first_line_has_content`),
/// because the caption's content may live in a following inline node (`^ *b*`,
/// where the marker node is just `"^ "` and `*b*` is an Emphasis sibling).
fn caption_marker_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'^') || bytes.get(1) != Some(&b' ') {
        return None;
    }
    let mut n = 1;
    while n < bytes.len() && bytes[n] == b' ' {
        n += 1;
    }
    Some(n)
}

/// Whether a string carries caption content: at least one non-ASCII-whitespace
/// byte. A non-breaking space (U+00A0) and any other non-ASCII byte count as
/// content, matching the direct-caption path (`caption_content` trims only
/// ASCII whitespace) and carve-php's byte-mode `\S`. `str::trim` is
/// Unicode-aware and would wrongly drop NBSP, so test bytes directly.
fn has_caption_content(s: &str) -> bool {
    s.bytes().any(|b| !b.is_ascii_whitespace())
}

/// Whether a `[Image, SoftBreak, "^ …", …]` paragraph's caption carries any
/// content on its FIRST line: text after the `^ ` marker on the marker node, or
/// any following inline node before the first soft break. Rejects an empty
/// first-line caption (`^ ` with content only on later folded lines, or none).
fn caption_first_line_has_content(children: &[InlineNode]) -> bool {
    if let InlineNode::Text(t) = &children[2] {
        if let Some(n) = caption_marker_len(t) {
            if has_caption_content(&t[n..]) {
                return true;
            }
        }
    }
    for child in &children[3..] {
        match child {
            InlineNode::SoftBreak => break,
            InlineNode::Text(t) if !has_caption_content(t) => continue,
            _ => return true,
        }
    }
    false
}

fn promote_block_images(blocks: &mut [BlockNode], figures_only: bool) {
    for block in blocks.iter_mut() {
        // The sole-image -> block-image promotion is skipped in `figures_only`
        // mode (the formatter): a paragraph and a bare block image serialize
        // identically, so the only effect there would be dropping a leading
        // block-attribute line (`{#id}`) that the paragraph carries but a bare
        // block image cannot. The formatter keeps it a paragraph so those attrs
        // survive.
        //
        // Only a REAL image (direct or resolved reference) promotes. An
        // unresolved reference image keeps its `ref_label` and renders as
        // literal text; in HTML mode it is already a Text node here, so the
        // guard only matters for the parse-only formatter path, where the
        // unresolved Image survives.
        let single_image = !figures_only
            && matches!(
                block,
                BlockNode::Paragraph(p)
                    if p.children.len() == 1
                        && matches!(&p.children[0], InlineNode::Image(img) if img.ref_label.is_none())
            );
        if single_image {
            // Take the children out first so the paragraph borrow ends before
            // `block` is reassigned. A leading block-attribute line (`{#id}`)
            // landed on the paragraph; carry it onto the promoted block image
            // (its own inline attrs win on conflict, §15), matching a direct
            // block image -- otherwise the id would be lost with the wrapper.
            let (mut children, para_attrs) = match block {
                BlockNode::Paragraph(p) => (std::mem::take(&mut p.children), p.attrs.take()),
                _ => unreachable!(),
            };
            if let InlineNode::Image(mut img) = children.remove(0) {
                if let Some(attrs) = para_attrs {
                    merge_leading_attrs(&mut img.attrs, attrs);
                }
                *block = BlockNode::BlockImage(img);
            }
            continue;
        }
        // A resolved reference image on its own line followed by a `^ ` caption
        // becomes a Figure, matching a direct-image figure and carve-php. A
        // reference image arrives here as `Paragraph[Image, SoftBreak,
        // "^ caption…"]` (the syntactic block-image/caption pass only knows the
        // inline `![…](…)` form); an unresolved ref is a Text node (not an
        // Image) so it stays literal. The caption inlines are already parsed
        // (paragraph interruption already stopped the caption at a block opener,
        // so a multi-line caption keeps its interior soft breaks); strip the
        // `^ ` marker from the leading Text.
        // Strict column-0 (docs/divergence-from-djot.md §11): the image must have
        // sat at its container's content column. An INDENTED image + caption is
        // literal paragraph text (a `<p>` with an inline image and a literal
        // `^ caption` line), matching carve-php / carve-js -- so gate on
        // `at_content_column`. A flush-left DIRECT image + caption never reaches
        // here (it became a Figure at parse time); this path serves resolved
        // REFERENCE images, which likewise promote only when flush-left.
        let ref_figure = matches!(
            block,
            BlockNode::Paragraph(p)
                if p.at_content_column
                    && p.children.len() >= 3
                    && matches!(&p.children[0], InlineNode::Image(img) if img.ref_label.is_none())
                    && matches!(p.children[1], InlineNode::SoftBreak)
                    && matches!(&p.children[2], InlineNode::Text(t) if caption_marker_len(t).is_some())
                    && caption_first_line_has_content(&p.children)
        );
        if ref_figure {
            // Carry a leading block-attribute line (`{#id}` etc.) from the
            // paragraph onto the figure, matching a direct-image figure (which
            // takes the attrs at parse time) and carve-php -- otherwise
            // `carve fmt` would drop it.
            let (mut children, attrs) = match block {
                BlockNode::Paragraph(p) => (std::mem::take(&mut p.children), p.attrs.take()),
                _ => unreachable!(),
            };
            let InlineNode::Image(img) = children.remove(0) else {
                unreachable!()
            };
            children.remove(0); // the soft break
            if let InlineNode::Text(t) = &mut children[0] {
                let n = caption_marker_len(t).unwrap();
                let rest = t[n..].to_string();
                if rest.is_empty() {
                    children.remove(0);
                } else {
                    *t = rest;
                }
            }
            *block = BlockNode::Figure(Figure {
                attrs,
                target: FigureTarget::Image(img),
                caption: children,
            });
            continue;
        }
        match block {
            BlockNode::BlockQuote(b) => promote_block_images(&mut b.children, figures_only),
            BlockNode::Admonition(a) => promote_block_images(&mut a.children, figures_only),
            BlockNode::Div(d) => promote_block_images(&mut d.children, figures_only),
            BlockNode::List(l) => {
                for item in &mut l.items {
                    promote_block_images(&mut item.children, figures_only);
                }
            }
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for def in &mut item.definitions {
                        promote_block_images(def, figures_only);
                    }
                }
            }
            _ => {}
        }
    }
}

fn is_collapsed_reference(link: &Link) -> bool {
    let Some(raw) = &link.raw_ref else {
        return false;
    };
    let bytes = raw.as_bytes();
    let Some((_, after_text)) = read_bracketed(bytes, 0) else {
        return false;
    };
    let Some((label, _)) = read_bracketed(bytes, after_text) else {
        return false;
    };
    label.is_empty()
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
                        from_crossref: true,
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
            // An inline literal renders as visible prose (§27), so it feeds the
            // parse-time cross-reference slug like a code span does.
            InlineNode::LiteralInline(l) => out.push_str(&l.content),
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

/// Code points removed from a heading-id source before slugging: the
/// bidi-override / isolate controls (also stripped from rendered text, see
/// `escape::is_bidi_control`) plus the zero-width characters that are NOT
/// stripped from text but must never leak into an `id="..."`.
fn is_id_strippable(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'   // bidi LRE/RLE/PDF/LRO/RLO
        | '\u{2066}'..='\u{2069}' // bidi isolates LRI/RLI/FSI/PDI
        | '\u{200B}'              // zero-width space
        | '\u{200C}'              // zero-width non-joiner
        | '\u{200D}'              // zero-width joiner
        | '\u{2060}'              // word joiner
        | '\u{FEFF}'              // zero-width no-break space / BOM
        | '\u{00AD}'              // soft hyphen
    )
}

/// NFC-normalize, then drop the invisible/dangerous controls (see
/// `is_id_strippable`). The pre-slug transform that makes a generated id
/// deterministic and Trojan-Source-safe (corpus 117). Parity with carve-js
/// `sanitizeIdSource`.
fn sanitize_id_source(text: &str) -> String {
    crate::unicode_nfc::nfc(text)
        .chars()
        .filter(|c| !is_id_strippable(*c))
        .collect()
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
    // Trojan-Source hardening for generated ids (corpus 117), applied BEFORE
    // the slug run so the remaining text slugs as usual:
    //   - NFC normalization, so a precomposed `é` (U+00E9) and a decomposed
    //     `e`+U+0301 produce the SAME id.
    //   - strip bidi-override / isolate controls (U+202A..U+202E, U+2066..U+2069)
    //     and zero-width characters (U+200B/C/D, U+2060, U+FEFF, U+00AD) so none
    //     of these can ever appear inside an `id="..."`.
    // Matches carve-js `sanitizeIdSource` (heading-ids.ts).
    let sanitized = sanitize_id_source(text);
    let detyped = de_typography(&sanitized);
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
    bounds: &InlineBounds<'_>,
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
    // The span closes on a `delim}` pair; with no such pair ahead the scan could
    // only walk to end-of-text and fail, so bail in O(1) (keeps `{/`×n linear).
    if !bounds.has_delim_brace_from(delim, i) {
        return None;
    }
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

/// Number of distinct single-char emphasis delimiters (`/ * _ ~ =`), plus one
/// catch-all slot. Sizes the per-`parse_inline_context` no-close memo.
const EMPHASIS_DELIM_SLOTS: usize = 6;

#[inline]
fn emphasis_delim_index(delim: u8) -> usize {
    match delim {
        b'/' => 0,
        b'*' => 1,
        b'_' => 2,
        b'~' => 3,
        b'=' => 4,
        _ => 5,
    }
}

/// `find_emphasis_close` with a per-delimiter failure memo. Once an opener of a
/// given delimiter finds no valid closer scanning to end-of-text, every later
/// opener of that delimiter (a larger `from`) also fails: the main loop only
/// calls `match_emphasis` at positions outside code spans / escapes -- the same
/// positions `find_emphasis_close` treats as "clean" -- so a suffix scan from a
/// larger `from` can never expose a closer that the earlier, wider scan missed.
/// This bounds `_a](`×n / `*a](`×n at O(n) instead of O(n^2) while keeping
/// output byte-identical (skipping only ever elides a call that would fail).
fn cached_find_emphasis_close(
    bytes: &[u8],
    from: usize,
    delim: u8,
    no_close: &mut [Option<usize>; EMPHASIS_DELIM_SLOTS],
) -> Option<usize> {
    let idx = emphasis_delim_index(delim);
    if let Some(first) = no_close[idx] {
        if from >= first {
            return None;
        }
    }
    let close = find_emphasis_close(bytes, from, delim);
    if close.is_none() {
        no_close[idx] = Some(match no_close[idx] {
            Some(f) => f.min(from),
            None => from,
        });
    }
    close
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
