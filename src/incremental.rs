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
    IncrementalParse {
        document,
        source_layout_json,
        snapshot: ParserSnapshot {
            source: source.to_owned(),
            identity: None,
            next_identity_id: 0,
        },
        changed_source: std::iter::once(0..source.len()).collect(),
        reused_previous_tree: false,
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
        identity: old_identity,
        mut next_identity_id,
    } = snapshot;
    let mut source = old_source.clone();
    let mut ordered = changes.to_vec();
    ordered.sort_by_key(|change| change.range.start);
    for pair in ordered.windows(2) {
        if pair[0].range.end > pair[1].range.start {
            return Err(IncrementalParseError("text changes overlap".into()));
        }
    }
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
        source.replace_range(change.range.clone(), &change.replacement);
    }
    let (document, source_layout_json) = parse_with_source_layout(&source);
    let identity = match old_identity {
        Some(previous) => {
            let (old_doc, _) = parse_with_source_layout(&old_source);
            let ranges = ordered
                .iter()
                .map(|change| (change.range.clone(), change.replacement.len()))
                .collect::<Vec<_>>();
            Some(
                crate::ast_sidecars::retain_node_identity(
                    &old_doc,
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
        document,
        source_layout_json,
        snapshot: ParserSnapshot {
            source,
            identity: identity.clone(),
            next_identity_id,
        },
        changed_source: ordered.into_iter().map(|change| change.range).collect(),
        // This first draft establishes edit validation and snapshot semantics.
        // Region reuse follows without changing the public result contract.
        reused_previous_tree: false,
        node_identity: identity,
    })
}

#[cfg(test)]
mod identity_tests {
    use super::*;

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
