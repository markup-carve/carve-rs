use std::sync::atomic::{AtomicU64, Ordering};
use std::{fmt, ops::Range};

use crate::{parse_with_source_layout, Document};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Debug, Clone)]
pub struct ParserSnapshot {
    source: String,
    document: Document,
    plain_paragraphs: bool,
    identity: Option<crate::NodeIdentity>,
    next_identity_id: usize,
}

impl ParserSnapshot {
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone)]
pub struct IncrementalParse {
    pub document: Document,
    pub source_layout_json: String,
    pub snapshot: ParserSnapshot,
    pub changed_source: Vec<Range<usize>>,
    pub reused_previous_tree: bool,
    /// Bytes passed to the parser during this operation; source mapping is separate.
    pub parsed_source_bytes: usize,
    /// Present when parsing began with `parse_snapshot_with_identity`.
    pub node_identity: Option<crate::NodeIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalParseError(pub String);

impl fmt::Display for IncrementalParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for IncrementalParseError {}

pub fn parse_snapshot(source: &str) -> IncrementalParse {
    let (document, source_layout_json) = parse_with_source_layout(source);
    let plain_paragraphs = reusable_paragraphs(source, &document);
    IncrementalParse {
        document: document.clone(),
        source_layout_json,
        snapshot: ParserSnapshot {
            source: source.to_owned(),
            document,
            plain_paragraphs,
            identity: None,
            next_identity_id: 0,
        },
        changed_source: std::iter::once(0..source.len()).collect(),
        reused_previous_tree: false,
        parsed_source_bytes: source.len(),
        node_identity: None,
    }
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

/// Start a parse session with ephemeral node identity enabled.
pub fn parse_snapshot_with_identity(source: &str) -> IncrementalParse {
    let mut result = parse_snapshot(source);
    let serial = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let session = format!(
        "rs-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        serial
    );
    let identity = crate::fresh_node_identity(&result.document, session)
        .expect("a parsed document has a valid canonical AST");
    result.snapshot.next_identity_id = identity.nodes.len();
    result.snapshot.identity = Some(identity.clone());
    result.node_identity = Some(identity);
    result
}

pub fn reparse(
    snapshot: ParserSnapshot,
    changes: &[TextChange],
) -> Result<IncrementalParse, IncrementalParseError> {
    let ParserSnapshot {
        source: old_source,
        document: old_document,
        plain_paragraphs,
        identity: old_identity,
        mut next_identity_id,
    } = snapshot;
    let mut source = String::with_capacity(old_source.len());
    let mut ordered = changes.to_vec();
    ordered.sort_by_key(|change| change.range.start);
    for pair in ordered.windows(2) {
        if pair[0].range.end > pair[1].range.start {
            return Err(IncrementalParseError("text changes overlap".into()));
        }
    }
    let mut cursor = 0;
    for change in &ordered {
        if change.range.start > change.range.end || change.range.end > old_source.len() {
            return Err(IncrementalParseError("text change is out of bounds".into()));
        }
        if !old_source.is_char_boundary(change.range.start)
            || !old_source.is_char_boundary(change.range.end)
        {
            return Err(IncrementalParseError(
                "text change splits a UTF-8 code point".into(),
            ));
        }
        source.push_str(&old_source[cursor..change.range.start]);
        source.push_str(&change.replacement);
        cursor = change.range.end;
    }
    source.push_str(&old_source[cursor..]);
    let mut attempted_bytes = 0;
    let incremental = if ordered.is_empty() && plain_paragraphs {
        Some((old_document.clone(), 0))
    } else if plain_paragraphs {
        reparse_paragraph(&old_document, &old_source, &ordered, &mut attempted_bytes)
    } else {
        None
    };
    let reused_previous_tree =
        incremental.is_some() && (ordered.is_empty() || old_document.children.len() > 1);
    let (document, source_layout_json, parsed_source_bytes) = match incremental {
        Some((document, bytes)) => {
            let layout = crate::to_source_layout_json(&source, &document);
            (document, layout, bytes)
        }
        None => {
            let (document, layout) = parse_with_source_layout(&source);
            (document, layout, source.len() + attempted_bytes)
        }
    };
    let plain_paragraphs = reusable_paragraphs(&source, &document);
    let identity = match old_identity {
        Some(previous) => {
            let ranges = ordered
                .iter()
                .map(|change| (change.range.clone(), change.replacement.len()))
                .collect::<Vec<_>>();
            Some(
                crate::ast_sidecars::retain_node_identity(
                    &old_document,
                    &old_source,
                    &document,
                    &source,
                    &ranges,
                    &previous,
                    &mut next_identity_id,
                )
                .map_err(|err| IncrementalParseError(err.to_string()))?,
            )
        }
        None => None,
    };
    Ok(IncrementalParse {
        document: document.clone(),
        source_layout_json,
        snapshot: ParserSnapshot {
            source,
            document,
            plain_paragraphs,
            identity: identity.clone(),
            next_identity_id,
        },
        changed_source: ordered.into_iter().map(|change| change.range).collect(),
        reused_previous_tree,
        parsed_source_bytes,
        node_identity: identity,
    })
}

fn literal_paragraph_text(source: &str) -> bool {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"\A[\pL\pN\pM .,!?\n]*\z").unwrap())
        .is_match(source)
}

fn reusable_paragraphs(source: &str, document: &Document) -> bool {
    literal_paragraph_text(source)
        && !document.children.is_empty()
        && document.frontmatter.is_empty()
        && document.frontmatter_raw.is_none()
        && document.footnote_defs.is_empty()
        && document.children.iter().all(|block| match block {
            crate::BlockNode::Paragraph(paragraph) => paragraph.attrs.is_none()
                && paragraph
                    .pos
                    .as_ref()
                    .is_some_and(|pos| pos.start_line == pos.end_line && pos.start_column == 1)
                && paragraph.children.iter().all(
                    |inline| matches!(inline, crate::InlineNode::Text(text) if text.pos.is_some()),
                ),
            _ => false,
        })
}

fn shift_paragraph(block: &mut crate::BlockNode, offset: isize, lines: usize) {
    let shift = |position: &mut Option<crate::Pos>| {
        if let Some(pos) = position {
            pos.start_offset = pos.start_offset.checked_add_signed(offset).unwrap();
            pos.end_offset = pos.end_offset.checked_add_signed(offset).unwrap();
            pos.start_line += lines;
            pos.end_line += lines;
        }
    };
    if let crate::BlockNode::Paragraph(paragraph) = block {
        shift(&mut paragraph.pos);
        for inline in &mut paragraph.children {
            if let crate::InlineNode::Text(text) = inline {
                shift(&mut text.pos);
            }
        }
    }
}

fn reparse_paragraph(
    document: &Document,
    source: &str,
    changes: &[TextChange],
    attempted_bytes: &mut usize,
) -> Option<(Document, usize)> {
    let [change] = changes else { return None };
    if change.replacement.contains('\n') || !literal_paragraph_text(&change.replacement) {
        return None;
    }
    let mut byte_offsets: Vec<usize> = source.char_indices().map(|(offset, _)| offset).collect();
    byte_offsets.push(source.len());
    for (index, block) in document.children.iter().enumerate() {
        let crate::BlockNode::Paragraph(paragraph) = block else {
            return None;
        };
        let pos = paragraph.pos.as_ref()?;
        let start = *byte_offsets.get(pos.start_offset)?;
        let end = *byte_offsets.get(pos.end_offset)?;
        if change.range.start < start || change.range.end > end {
            continue;
        }
        let mut fragment = source[start..end].to_owned();
        if fragment.contains('\n') {
            return None;
        }
        fragment.replace_range(
            change.range.start - start..change.range.end - start,
            &change.replacement,
        );
        if fragment.is_empty() || fragment.starts_with(' ') || fragment.ends_with(' ') {
            return None;
        }
        *attempted_bytes = fragment.len();
        let (mut replacement, _) = parse_with_source_layout(&fragment);
        if replacement.children.len() != 1 || !reusable_paragraphs(&fragment, &replacement) {
            return None;
        }
        let delta =
            fragment.chars().count() as isize - (pos.end_offset - pos.start_offset) as isize;
        let mut result = document.clone();
        let mut edited = replacement.children.remove(0);
        shift_paragraph(&mut edited, pos.start_offset as isize, pos.start_line - 1);
        result.children[index] = edited;
        for following in &mut result.children[index + 1..] {
            shift_paragraph(following, delta, 0);
        }
        result.source_len =
            source.len() - (change.range.end - change.range.start) + change.replacement.len();
        return Some((result, fragment.len()));
    }
    None
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn paragraph_edit_reuses_other_blocks_and_preserves_positions() {
        for (source, replacement) in [
            ("first\n\nmiddle\n\nlast\n", "longer text"),
            ("één\n\nMitte\n\n終わり\n", "日本語"),
        ] {
            let first = parse_snapshot_with_identity(source);
            let start = source.find("\n\n").unwrap() + 2;
            let end = start + source[start..].find('\n').unwrap();
            let changed = reparse(
                first.snapshot,
                &[TextChange {
                    range: start..end,
                    replacement: replacement.into(),
                }],
            )
            .unwrap();
            let mut expected = source.to_owned();
            expected.replace_range(start..end, replacement);
            let (document, layout) = parse_with_source_layout(&expected);
            assert_eq!(changed.document, document);
            assert_eq!(changed.source_layout_json, layout);
            assert!(changed.reused_previous_tree);
            assert_eq!(changed.parsed_source_bytes, replacement.len());
        }
    }

    #[test]
    fn complex_edits_fall_back_and_noop_reuses_everything() {
        for replacement in ["# Heading", "[ref]: /url", "first\n\nsecond", "1. item", ""] {
            let first = parse_snapshot("before\n\ntext\n\nafter");
            let changed = reparse(
                first.snapshot,
                &[TextChange {
                    range: 8..12,
                    replacement: replacement.into(),
                }],
            )
            .unwrap();
            let source = format!("before\n\n{replacement}\n\nafter");
            assert_eq!(changed.document, parse_with_source_layout(&source).0);
            assert!(!changed.reused_previous_tree);
            assert!(changed.parsed_source_bytes >= source.len());
        }
        for source in ["plain text", "# Heading\n\n[link](/url)"] {
            let first = parse_snapshot(source);
            let changed = reparse(first.snapshot, &[]).unwrap();
            assert_eq!(changed.reused_previous_tree, source == "plain text");
            assert_eq!(
                changed.parsed_source_bytes,
                if source == "plain text" {
                    0
                } else {
                    source.len()
                }
            );
            assert_eq!(changed.document, first.document);
        }
    }

    #[test]
    fn repeated_local_edits_match_fresh_parses() {
        let mut state = parse_snapshot("first\n\ntext\n\nlast");
        for index in 0..100 {
            let start = 7;
            let end = state.snapshot.source()[start..].find('\n').unwrap() + start;
            let replacement = format!("paragraph {index} é");
            state = reparse(
                state.snapshot,
                &[TextChange {
                    range: start..end,
                    replacement,
                }],
            )
            .unwrap();
            let (fresh, layout) = parse_with_source_layout(state.snapshot.source());
            assert_eq!(state.document, fresh);
            assert_eq!(state.source_layout_json, layout);
            assert!(state.reused_previous_tree);
        }
    }

    #[test]
    fn unchanged_node_keeps_id_after_prior_insertion() {
        let first = parse_snapshot_with_identity("alpha\n\nbeta");
        let old = first.node_identity.as_ref().unwrap();
        let beta = old
            .nodes
            .iter()
            .find(|entry| entry.path == "/children/1")
            .unwrap()
            .id
            .clone();
        let second = reparse(
            first.snapshot,
            &[TextChange {
                range: 0..0,
                replacement: "new\n\n".into(),
            }],
        )
        .unwrap();
        let next = second.node_identity.as_ref().unwrap();
        assert_eq!(next.session, old.session);
        assert_eq!(
            next.nodes
                .iter()
                .find(|entry| entry.path == "/children/2")
                .unwrap()
                .id,
            beta
        );
    }

    #[test]
    fn edited_node_retires_its_id_within_session() {
        let first = parse_snapshot_with_identity("alpha");
        let old = first.node_identity.as_ref().unwrap();
        let paragraph = old
            .nodes
            .iter()
            .find(|entry| entry.path == "/children/0")
            .unwrap()
            .id
            .clone();
        let second = reparse(
            first.snapshot,
            &[TextChange {
                range: 0..5,
                replacement: "omega".into(),
            }],
        )
        .unwrap();
        let next = second.node_identity.as_ref().unwrap();
        assert!(next.nodes.iter().all(|entry| entry.id != paragraph));
        let third = reparse(
            second.snapshot,
            &[TextChange {
                range: 0..5,
                replacement: "alpha".into(),
            }],
        )
        .unwrap();
        assert!(third
            .node_identity
            .unwrap()
            .nodes
            .iter()
            .all(|entry| entry.id != paragraph));
    }
}
