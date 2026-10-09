use std::collections::BTreeMap;

use super::{normalize_source, trim_ascii_end, DEFAULT_FRONTMATTER_FORMAT};
use crate::ast::{Frontmatter, Pos};

type SplitFrontmatter<'a> = (BTreeMap<String, String>, Option<Frontmatter>, &'a str);

/// The key/value view of a frontmatter block, derived from its raw text.
///
/// Shared with the AST decoder rather than duplicated there. The wire form
/// carries the RAW block only (PART 12 §7 - a parsed map cannot be serialized
/// back to the bytes the author wrote), so a decoded document has to rebuild
/// this the same way a parsed one built it. Deriving it with the same function
/// is what makes decode(encode(x)) equal x instead of nearly equal.
pub(crate) fn frontmatter_map(format: &str, content: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    // Only the bare / yaml form is key:value; typed blocks (json/toml) are
    // structured and just stripped.
    if format.is_empty() || format.eq_ignore_ascii_case("yaml") {
        for line in content.lines() {
            if let Some((key, value)) = line.split_once(':') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    map
}

/// The span of a frontmatter block, fences included. It always starts at the
/// first character of the document, so only the end has to be worked out - and
/// the block is taken from the raw source before any line is stripped, so every
/// column here is a column in the document.
fn frontmatter_pos(source: &str, block_end: usize) -> Pos {
    let block = &source[..block_end];
    let last_line_start = block.rfind('\n').map_or(0, |at| at + 1);
    Pos {
        start_line: 1,
        start_column: 1,
        start_offset: 0,
        end_line: block.bytes().filter(|b| *b == b'\n').count() + 1,
        // Columns and offsets are counted in CODEPOINTS (PART 12 section 4).
        end_column: block[last_line_start..].chars().count() + 1,
        end_offset: block.chars().count(),
        file: None,
    }
}

/// The format token of a frontmatter opener, given everything after the `---`.
pub(crate) fn frontmatter_format_token(after_marker: &str) -> Option<&str> {
    let token_start = after_marker
        .find(|c: char| !matches!(c, ' ' | '\t'))
        .unwrap_or(after_marker.len());
    let kind = trim_ascii_end(&after_marker[token_start..]);
    // NOTHING AFTER THE MARKER: the run is the LINE ENDING, not this slot.
    //
    // POSITION DECIDES (carve#1295). A tab BEFORE content is a separator and
    // the terminal is `space` alone; a tab with nothing after it is TRAILING,
    // and PART 2's NO TRAILING WHITESPACE drops it - its run is `whitespace`,
    // `' ' | '\t'`. A frontmatter delimiter takes no content on its line, so
    // `---<TAB>` lands on the trailing side and opens the block.
    //
    // It was reaching the space-only test below, which refused it - while the
    // same line still read as a THEMATIC BREAK. One trailing tab disqualified
    // one construct and not the other, on the same line.
    //
    // The test order is what carries this: the emptiness question is asked
    // BEFORE the terminal question, because the terminal only governs a slot
    // and there is no slot on a content-less line.
    if kind.is_empty() {
        return Some(kind);
    }
    if after_marker[..token_start].chars().any(|c| c != ' ') {
        return None;
    }
    // AND EXACTLY ONE SPACE (carve#912). `frontmatter_open = "---", [space],
    // [frontmatter_format]` spells the slot as one; a wider run makes the line
    // no typed opener, and since it is not a thematic break either it is
    // ordinary paragraph text that the metadata lines fold into.
    if token_start > 1 {
        return None;
    }
    if !kind.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(kind)
}

/// Whether a BARE `---` block's content has the SHAPE of a mapping.
///
/// A shape test by byte scanning, never a parse (markup-carve/carve#2799).
/// Three YAML libraries disagree on edge cases, so carve-js, carve-php and
/// carve-rs each scan to this one rule and agree byte for byte instead. It
/// therefore differs from a parse on malformed content such as
/// `title: [unclosed`, which counts as a mapping here; that is deliberate.
///
/// A TYPED opener is never asked: `---FORMAT` says what the block is, and a
/// break is a dash run and nothing else (PART 1), so the collision this test
/// resolves cannot arise there.
pub(crate) fn has_mapping_shape(content: &str) -> bool {
    let Some(line) = content.lines().find(|line| {
        let bare = line.trim_start_matches([' ', '\t']);
        !bare.is_empty() && !bare.starts_with('#')
    }) else {
        // An empty or comment-only block has no mapping in it.
        return false;
    };
    // The key is at column 0: a double- or single-quoted string, or a run that
    // starts with neither whitespace nor `-`, `[`, `{`, `"`, `'`, `#` and holds
    // no `:`. A space, a tab or the line end must follow the colon.
    let Some(rest) = after_mapping_key(line) else {
        return false;
    };
    let rest = rest.as_bytes();
    rest.first() == Some(&b':') && matches!(rest.get(1), None | Some(b' ') | Some(b'\t'))
}

/// The key of a mapping line at column 0, returning what follows it.
fn after_mapping_key(line: &str) -> Option<&str> {
    let first = line.chars().next()?;
    if first == '"' || first == '\'' {
        let end = line[1..].find(first)?;
        return Some(&line[1 + end + 1..]);
    }
    if first.is_whitespace() || matches!(first, '-' | '[' | '{' | '#') {
        return None;
    }
    let end = line.find(':')?;
    // An empty key is no key.
    if end == 0 {
        return None;
    }
    Some(&line[end..])
}

/// Whether `source` opens a frontmatter block, by the parser's own test.
pub(crate) fn opens_frontmatter(source: &str) -> bool {
    let normalized = normalize_source(source);
    split_frontmatter(normalized.as_ref(), false).1.is_some()
}

pub(super) fn split_frontmatter(source: &str, positions: bool) -> SplitFrontmatter<'_> {
    // Opening fence: `---` optionally followed by a type token (`---yaml`,
    // `---json`, `---toml`, ...; canonical has no space). Closer is a bare `---`.
    if !source.starts_with("---") {
        return (BTreeMap::new(), None, source);
    }
    let Some(first_nl) = source.find('\n') else {
        return (BTreeMap::new(), None, source);
    };
    let Some(kind) = frontmatter_format_token(&source[3..first_nl]) else {
        return (BTreeMap::new(), None, source);
    };
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
        return (BTreeMap::new(), None, source);
    };
    let frontmatter_src = &rest[..content_len];
    let body = &rest[after..];
    let frontmatter = frontmatter_map(kind, frontmatter_src);
    let raw = Frontmatter {
        // A bare fence is yaml, which is what the reference publishes.
        format: if kind.is_empty() {
            DEFAULT_FRONTMATTER_FORMAT.to_string()
        } else {
            kind.to_string()
        },
        content: frontmatter_src.trim_end_matches('\n').to_string(),
        pos: positions.then(|| {
            // `after` runs past the closing fence's newline when it has one;
            // the span stops at the fence, not at the blank after it.
            let block_end = first_nl + 1 + after;
            frontmatter_pos(source, source[..block_end].trim_end_matches('\n').len())
        }),
    };
    (frontmatter, Some(raw), body)
}
