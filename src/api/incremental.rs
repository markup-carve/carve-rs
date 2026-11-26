//! The 0.1 incremental parse and editor session calls over the 0.2 parser.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fmt, ops::Range};

use serde_json::Value;

use super::{Document, Input, Options};
use crate::ast::owned::Node;

/// Replacement of one UTF-8 byte range in a source snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    /// Byte range to replace.
    pub range: Range<usize>,
    /// Replacement source text.
    pub replacement: String,
}

/// Stable id assigned to one tree path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeIdentityEntry {
    /// Opaque node id.
    pub id: String,
    /// RFC 6901 pointer to the node in the encoded tree.
    pub path: String,
}

/// Node ids minted for one parse session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeIdentity {
    /// Sidecar schema version.
    pub version: u32,
    /// Opaque session identifier.
    pub session: String,
    /// Node ids and their paths, in document order.
    pub nodes: Vec<NodeIdentityEntry>,
}

/// Source and parse state kept for a later edit.
#[derive(Debug, Clone)]
pub struct ParserSnapshot {
    source: String,
    document: Document,
    plain_paragraphs: bool,
    identity: Option<NodeIdentity>,
    next_identity_id: usize,
}

impl ParserSnapshot {
    /// Source text represented by this snapshot.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Updated document, source layout, and snapshot after parsing.
#[derive(Debug, Clone)]
pub struct IncrementalParse {
    /// Parsed 0.2 tree, equal to a fresh [`super::parse`] of the source.
    pub document: Document,
    /// Source-layout JSON of [`super::parse_with_layout`].
    pub source_layout_json: String,
    /// Snapshot to pass to the next [`reparse`] call.
    pub snapshot: ParserSnapshot,
    /// Changed byte ranges in the previous snapshot's coordinates. An initial
    /// parse reports the full range of its new source.
    pub changed_source: Vec<Range<usize>>,
    /// Whether unchanged nodes from the previous tree were reused.
    pub reused_previous_tree: bool,
    /// Bytes passed to the parser during this operation.
    pub parsed_source_bytes: usize,
    /// Present when parsing began with [`parse_snapshot_with_identity`].
    pub node_identity: Option<NodeIdentity>,
}

/// Invalid text changes, or a source the parser refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalParseError(pub String);

impl fmt::Display for IncrementalParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for IncrementalParseError {}

impl From<super::Error> for IncrementalParseError {
    fn from(error: super::Error) -> Self {
        Self(error.to_string())
    }
}

fn full_parse(source: &str) -> Result<(Document, String), IncrementalParseError> {
    let parsed = super::parse_with_layout(source, &Options::default())?;
    Ok((parsed.tree.into_document(), parsed.layout.to_string()))
}

fn layout_of(source: &str, document: &Document) -> Result<String, IncrementalParseError> {
    let value = tree_value(document)?;
    Ok(super::layout::build_from(source, &value, Input::Text(source)).to_string())
}

/// Parses a source string and creates a snapshot for later edits.
///
/// # Errors
///
/// A source longer than the tree positions can count.
pub fn parse_snapshot(source: &str) -> Result<IncrementalParse, IncrementalParseError> {
    let (document, source_layout_json) = full_parse(source)?;
    let plain_paragraphs = reusable_paragraphs(source, &document);
    Ok(IncrementalParse {
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
    })
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

/// Starts a parse session with ephemeral node identity: [`reparse`] keeps
/// the id of every node outside the edited ranges whose content is unchanged.
///
/// # Errors
///
/// As [`parse_snapshot`].
pub fn parse_snapshot_with_identity(
    source: &str,
) -> Result<IncrementalParse, IncrementalParseError> {
    let mut result = parse_snapshot(source)?;
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
    let value = tree_value(&result.document)?;
    let nodes = node_keys(&value)
        .into_iter()
        .enumerate()
        .map(|(index, NodeKey { path, .. })| NodeIdentityEntry {
            id: format!("n{index}"),
            path,
        })
        .collect::<Vec<_>>();
    result.snapshot.next_identity_id = nodes.len();
    let identity = NodeIdentity {
        version: 1,
        session,
        nodes,
    };
    result.snapshot.identity = Some(identity.clone());
    result.node_identity = Some(identity);
    Ok(result)
}

/// The changes sorted by start, checked against the source.
fn ordered_changes(
    source: &str,
    changes: &[TextChange],
) -> Result<Vec<TextChange>, IncrementalParseError> {
    let mut ordered = changes.to_vec();
    ordered.sort_by_key(|change| change.range.start);
    // The last change's error first, as in 0.1.
    for change in ordered.iter().rev() {
        if change.range.start > change.range.end || change.range.end > source.len() {
            return Err(IncrementalParseError("text change is out of bounds".into()));
        }
        if !source.is_char_boundary(change.range.start)
            || !source.is_char_boundary(change.range.end)
        {
            return Err(IncrementalParseError(
                "text change splits a UTF-8 code point".into(),
            ));
        }
    }
    if ordered
        .windows(2)
        .any(|pair| matches!(pair, [a, b] if a.range.end > b.range.start))
    {
        return Err(IncrementalParseError("text changes overlap".into()));
    }
    Ok(ordered)
}

fn applied(source: &str, ordered: &[TextChange]) -> String {
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for change in ordered {
        out.push_str(source.get(cursor..change.range.start).unwrap_or(""));
        out.push_str(&change.replacement);
        cursor = change.range.end;
    }
    out.push_str(source.get(cursor..).unwrap_or(""));
    out
}

/// Applies text changes to a snapshot and parses the resulting source.
///
/// # Errors
///
/// Overlapping changes, a range out of bounds or splitting a UTF-8 code
/// point, or a result the parser refuses.
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
    let ordered = ordered_changes(&old_source, changes)?;
    let source = applied(&old_source, &ordered);
    let old_tree = match old_identity {
        Some(_) => Some(tree_value(&old_document)?),
        None => None,
    };
    let old_blocks = old_document.children.len();
    let mut attempted_bytes = 0;
    let incremental = if !plain_paragraphs {
        None
    } else if ordered.is_empty() {
        Some((old_document, 0))
    } else {
        paragraph_splice(&old_document, &old_source, &ordered, &mut attempted_bytes)
            .map(|splice| apply_splice(old_document, splice))
    };
    let reused_previous_tree = incremental.is_some() && (ordered.is_empty() || old_blocks > 1);
    let (document, source_layout_json, parsed_source_bytes) =
        if let Some((document, bytes)) = incremental {
            let layout = layout_of(&source, &document)?;
            (document, layout, bytes)
        } else {
            let (document, layout) = full_parse(&source)?;
            (document, layout, source.len() + attempted_bytes)
        };
    let plain_paragraphs = reusable_paragraphs(&source, &document);
    let identity = match (old_identity, old_tree) {
        (Some(previous), Some(old_tree)) => Some(retain_identity(
            &old_tree,
            &document,
            &ordered,
            &previous,
            &mut next_identity_id,
        )?),
        _ => None,
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
    static PATTERN: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"\A[\pL\pN\pM .,!?\n]*\z").ok())
        .as_ref()
        .is_some_and(|pattern| pattern.is_match(source))
}

/// A document of one-line plain paragraphs, whose paragraphs a parse of
/// their own text reproduces.
fn reusable_paragraphs(source: &str, document: &Document) -> bool {
    literal_paragraph_text(source)
        && !document.children.is_empty()
        && document.children.iter().all(|block| match block {
            Node::Paragraph(paragraph) => {
                paragraph.attrs.is_none()
                    && paragraph
                        .pos
                        .as_ref()
                        .is_some_and(|pos| pos.start_line == pos.end_line && pos.start_column == 1)
                    && paragraph
                        .children
                        .iter()
                        .all(|inline| matches!(inline, Node::Text(text) if text.pos.is_some()))
            }
            _ => false,
        })
}

fn shift_paragraph(block: &mut Node, offset: i64, lines: u32) {
    let shift = |position: &mut Option<crate::ast::Pos>| {
        if let Some(pos) = position {
            let moved = |at: u32| u32::try_from(i64::from(at) + offset).unwrap_or(at);
            pos.start_offset = moved(pos.start_offset);
            pos.end_offset = moved(pos.end_offset);
            pos.start_line += lines;
            pos.end_line += lines;
        }
    };
    if let Node::Paragraph(paragraph) = block {
        shift(&mut paragraph.pos);
        for inline in &mut paragraph.children {
            if let Node::Text(text) = inline {
                shift(&mut text.pos);
            }
        }
    }
}

/// The edited paragraph of a single edit inside one plain paragraph, parsed
/// on its own, and how to put it in place.
struct Splice {
    index: usize,
    edited: Node,
    delta: i64,
    length: usize,
    parsed: usize,
}

fn paragraph_splice(
    document: &Document,
    source: &str,
    changes: &[TextChange],
    attempted_bytes: &mut usize,
) -> Option<Splice> {
    let [change] = changes else { return None };
    if !literal_paragraph_text(&change.replacement) || change.replacement.contains('\n') {
        return None;
    }
    for (index, block) in document.children.iter().enumerate() {
        let Node::Paragraph(paragraph) = block else {
            return None;
        };
        let pos = paragraph.pos.as_ref()?;
        let (start, end) = (pos.start_offset as usize, pos.end_offset as usize);
        if change.range.start < start || change.range.end > end {
            continue;
        }
        let fragment = edited_fragment(source, start..end, change)?;
        *attempted_bytes = fragment.len();
        let mut replacement = super::parse(fragment.as_str(), &Options::default())
            .ok()?
            .into_document();
        if replacement.children.len() != 1 || !reusable_paragraphs(&fragment, &replacement) {
            return None;
        }
        let mut edited = replacement.children.pop()?;
        shift_paragraph(&mut edited, i64::from(pos.start_offset), pos.start_line - 1);
        return Some(Splice {
            index,
            edited,
            delta: i64::try_from(fragment.len()).ok()? - i64::from(pos.end_offset)
                + i64::from(pos.start_offset),
            length: source.len() - change.range.len() + change.replacement.len(),
            parsed: fragment.len(),
        });
    }
    None
}

/// The paragraph's text after the edit, when it can still stand alone.
fn edited_fragment(source: &str, paragraph: Range<usize>, change: &TextChange) -> Option<String> {
    let mut fragment = source.get(paragraph.clone())?.to_owned();
    fragment.replace_range(
        change.range.start - paragraph.start..change.range.end - paragraph.start,
        &change.replacement,
    );
    (!fragment.is_empty() && !fragment.starts_with(' ') && !fragment.ends_with(' '))
        .then_some(fragment)
}

fn apply_splice(mut document: Document, splice: Splice) -> (Document, usize) {
    let Splice {
        index,
        edited,
        delta,
        length,
        parsed,
    } = splice;
    if let Some(block) = document.children.get_mut(index) {
        *block = edited;
    }
    for block in document.children.iter_mut().skip(index + 1) {
        shift_paragraph(block, delta, 0);
    }
    document.src_byte_length = i64::try_from(length).ok();
    (document, parsed)
}

fn tree_value(document: &Document) -> Result<Value, IncrementalParseError> {
    let mut value =
        serde_json::to_value(document).map_err(|e| IncrementalParseError(e.to_string()))?;
    if let Some(map) = value.as_object_mut() {
        map.insert("type".into(), "document".into());
    }
    Ok(value)
}

/// One node of an encoded tree: its pointer, byte span, and a hash of its
/// content without positions, equal for a node that only moved.
struct NodeKey {
    path: String,
    span: Option<(usize, usize)>,
    content: u64,
}

/// The nodes of an encoded tree in document order.
fn node_keys(tree: &Value) -> Vec<NodeKey> {
    fn walk(value: &Value, path: &str, out: &mut Vec<NodeKey>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        match value {
            Value::Object(object) => {
                let slot = value.get("type").and_then(Value::as_str).map(|_| {
                    out.push(NodeKey {
                        path: path.to_owned(),
                        span: byte_span(value),
                        content: 0,
                    });
                    out.len() - 1
                });
                0u8.hash(&mut hasher);
                for (key, child) in object
                    .iter()
                    .filter(|(key, _)| !matches!(key.as_str(), "pos" | "segments"))
                {
                    key.hash(&mut hasher);
                    let escaped = key.replace('~', "~0").replace('/', "~1");
                    walk(child, &format!("{path}/{escaped}"), out).hash(&mut hasher);
                }
                let content = hasher.finish();
                if let Some(entry) = slot.and_then(|slot| out.get_mut(slot)) {
                    entry.content = content;
                }
                return content;
            }
            Value::Array(array) => {
                1u8.hash(&mut hasher);
                for (index, child) in array.iter().enumerate() {
                    walk(child, &format!("{path}/{index}"), out).hash(&mut hasher);
                }
            }
            other => {
                2u8.hash(&mut hasher);
                other.to_string().hash(&mut hasher);
            }
        }
        hasher.finish()
    }
    let mut out = Vec::new();
    walk(tree, "", &mut out);
    out
}

fn byte_span(node: &Value) -> Option<(usize, usize)> {
    let pos = node.get("pos")?;
    let start = usize::try_from(pos.get("startOffset")?.as_u64()?).ok()?;
    let end = usize::try_from(pos.get("endOffset")?.as_u64()?).ok()?;
    (start <= end).then_some((start, end))
}

/// The id of each old node no change touches, keyed by its shifted span and
/// content; `None` where two old nodes share a key.
fn kept_old_ids(
    old_nodes: &HashMap<String, NodeKey>,
    changes: &[TextChange],
    previous: &NodeIdentity,
) -> HashMap<(usize, usize, u64), Option<String>> {
    let mut shifts = vec![0isize];
    for change in changes {
        let last = shifts.last().copied().unwrap_or(0);
        let grown = isize::try_from(change.replacement.len()).unwrap_or(isize::MAX);
        let removed = isize::try_from(change.range.len()).unwrap_or(isize::MAX);
        shifts.push(last.saturating_add(grown).saturating_sub(removed));
    }
    let mut old_keys = HashMap::new();
    for entry in previous.nodes.iter().filter(|entry| !entry.path.is_empty()) {
        let Some(NodeKey {
            span: Some((start, end)),
            content,
            ..
        }) = old_nodes.get(&entry.path)
        else {
            continue;
        };
        let (start, end) = (*start, *end);
        let before = changes.partition_point(|change| change.range.end <= start);
        if changes.get(before).is_some_and(|change| {
            change.range.start < end && change.range.end > start
                || change.range.is_empty() && change.range.start > start && change.range.start < end
        }) {
            continue;
        }
        let shift = shifts.get(before).copied().unwrap_or(0);
        let (Some(start), Some(end)) = (
            start.checked_add_signed(shift),
            end.checked_add_signed(shift),
        ) else {
            continue;
        };
        old_keys
            .entry((start, end, *content))
            .and_modify(|id| *id = None)
            .or_insert_with(|| Some(entry.id.clone()));
    }
    old_keys
}

/// Keeps the id of each old node that no change touches and that the new
/// tree holds once, unchanged, at its shifted span; every other node gets a
/// new id, so an id is never reused for different content.
fn retain_identity(
    old_tree: &Value,
    new_document: &Document,
    changes: &[TextChange],
    previous: &NodeIdentity,
    next_id: &mut usize,
) -> Result<NodeIdentity, IncrementalParseError> {
    let old_nodes: HashMap<String, NodeKey> = node_keys(old_tree)
        .into_iter()
        .map(|key| (key.path.clone(), key))
        .collect();
    let mut old_keys = kept_old_ids(&old_nodes, changes, previous);
    let fresh = node_keys(&tree_value(new_document)?);
    let key_of = |node: &NodeKey| node.span.map(|(start, end)| (start, end, node.content));
    let mut counts: HashMap<(usize, usize, u64), usize> = HashMap::new();
    for key in fresh.iter().filter_map(key_of) {
        *counts.entry(key).or_default() += 1;
    }
    let root = previous
        .nodes
        .iter()
        .find(|entry| entry.path.is_empty())
        .map(|entry| entry.id.clone());
    let mut nodes = Vec::with_capacity(fresh.len());
    for node in &fresh {
        let kept = if node.path.is_empty() {
            root.clone()
        } else {
            key_of(node)
                .filter(|key| counts.get(key) == Some(&1))
                .and_then(|key| old_keys.remove(&key).flatten())
        };
        let id = kept.unwrap_or_else(|| {
            let id = format!("n{next_id}");
            *next_id += 1;
            id
        });
        nodes.push(NodeIdentityEntry {
            id,
            path: node.path.clone(),
        });
    }
    Ok(NodeIdentity {
        version: 1,
        session: previous.session.clone(),
        nodes,
    })
}
