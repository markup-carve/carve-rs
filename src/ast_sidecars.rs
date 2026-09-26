//! Optional metadata beside the canonical AST (PART 12 §§38, 40, 41).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ast::Document;
use crate::ast_json::{parse_value, try_to_json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstSidecarError(String);

impl fmt::Display for AstSidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AstSidecarError {}

fn error(message: impl Into<String>) -> AstSidecarError {
    AstSidecarError(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeIdentityEntry {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeIdentity {
    pub version: u32,
    pub session: String,
    pub nodes: Vec<NodeIdentityEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationAnchor {
    pub path: String,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationRange {
    pub id: String,
    pub kind: String,
    pub start: AnnotationAnchor,
    pub end: AnnotationAnchor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationRanges {
    pub version: u32,
    pub ranges: Vec<AnnotationRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSource {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceOrigin {
    Authored,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NodeProvenance {
    pub path: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_byte: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_byte: Option<usize>,
    pub origin: ProvenanceOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub version: u32,
    pub sources: Vec<ProvenanceSource>,
    pub nodes: Vec<NodeProvenance>,
}

fn ast_value(doc: &Document) -> Result<Value, AstSidecarError> {
    let json = try_to_json(doc).map_err(|err| error(err.to_string()))?;
    parse_value(&json).map_err(|err| error(err.to_string()))
}

pub(crate) fn retain_node_identity(
    old_doc: &Document,
    old_source: &str,
    new_doc: &Document,
    new_source: &str,
    changes: &[(Range<usize>, usize)],
    previous: &NodeIdentity,
    next_id: &mut usize,
) -> Result<NodeIdentity, AstSidecarError> {
    let old_ast = ast_value(old_doc)?;
    let new_ast = ast_value(new_doc)?;
    let old_offsets = codepoint_bytes(old_source);
    let new_offsets = codepoint_bytes(new_source);
    let mut old_keys = HashMap::new();
    for entry in &previous.nodes {
        if entry.path.is_empty() {
            continue;
        }
        let node = node_at(&old_ast, &entry.path)?;
        let Some((start, end)) = node_bytes(node, &old_offsets) else {
            continue;
        };
        if changes.iter().any(|(range, _)| {
            range.start < end && range.end > start
                || range.start == range.end && range.start > start && range.start < end
        }) {
            continue;
        }
        let adjustment: isize = changes
            .iter()
            .filter(|(range, _)| range.end <= start)
            .map(|(range, replacement_len)| *replacement_len as isize - range.len() as isize)
            .sum();
        let (Some(adjusted_start), Some(adjusted_end)) = (
            start.checked_add_signed(adjustment),
            end.checked_add_signed(adjustment),
        ) else {
            continue;
        };
        let key = (adjusted_start, adjusted_end, fingerprint(node));
        old_keys
            .entry(key)
            .and_modify(|id| *id = None)
            .or_insert_with(|| Some(entry.id.clone()));
    }
    let fresh_nodes = fresh_node_identity(new_doc, previous.session.clone())?.nodes;
    let mut new_key_counts = HashMap::new();
    for entry in &fresh_nodes {
        let node = node_at(&new_ast, &entry.path)?;
        if let Some((start, end)) = node_bytes(node, &new_offsets) {
            let key = (start, end, fingerprint(node));
            *new_key_counts.entry(key).or_insert(0usize) += 1;
        }
    }
    let mut nodes = Vec::new();
    for fresh in fresh_nodes {
        let id = if fresh.path.is_empty() {
            previous
                .nodes
                .iter()
                .find(|entry| entry.path.is_empty())
                .map(|entry| entry.id.clone())
        } else {
            let node = node_at(&new_ast, &fresh.path)?;
            node_bytes(node, &new_offsets).and_then(|(start, end)| {
                let key = (start, end, fingerprint(node));
                if new_key_counts.get(&key) == Some(&1) {
                    old_keys.remove(&key).flatten()
                } else {
                    None
                }
            })
        };
        let id = id.unwrap_or_else(|| {
            let id = format!("n{next_id}");
            *next_id += 1;
            id
        });
        nodes.push(NodeIdentityEntry {
            id,
            path: fresh.path,
        });
    }
    let sidecar = NodeIdentity {
        version: 1,
        session: previous.session.clone(),
        nodes,
    };
    validate_identity(&sidecar, &new_ast)?;
    Ok(sidecar)
}

fn codepoint_bytes(source: &str) -> Vec<usize> {
    let mut offsets: Vec<usize> = source.char_indices().map(|(byte, _)| byte).collect();
    offsets.push(source.len());
    offsets
}

fn node_bytes(node: &Value, offsets: &[usize]) -> Option<(usize, usize)> {
    let pos = node.get("pos")?;
    if pos.get("file").is_some() {
        return None;
    }
    let start = usize::try_from(pos.get("startOffset")?.as_u64()?).ok()?;
    let end = usize::try_from(pos.get("endOffset")?.as_u64()?).ok()?;
    if start > end {
        return None;
    }
    Some((*offsets.get(start)?, *offsets.get(end)?))
}

fn fingerprint(node: &Value) -> String {
    let mut stripped = node.clone();
    let mut pending = vec![&mut stripped];
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(object) => {
                object.remove("pos");
                pending.extend(object.values_mut());
            }
            Value::Array(values) => pending.extend(values.iter_mut()),
            _ => {}
        }
    }
    serde_json::to_string(&stripped).expect("AST JSON is serializable")
}

fn node_at<'a>(ast: &'a Value, path: &str) -> Result<&'a Value, AstSidecarError> {
    if !valid_pointer(path) {
        return Err(error(format!("invalid RFC 6901 pointer {path:?}")));
    }
    let mut node = ast;
    for encoded in path.split('/').skip(1) {
        let segment = encoded.replace("~1", "/").replace("~0", "~");
        node = match node {
            Value::Object(object) if structural_field(&segment) => object.get(&segment),
            Value::Array(array) if segment == "0" || !segment.starts_with('0') => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| array.get(index)),
            _ => None,
        }
        .ok_or_else(|| {
            error(format!(
                "sidecar path {path:?} does not address AST structure"
            ))
        })?;
    }
    if node.get("type").and_then(Value::as_str).is_none() {
        return Err(error(format!(
            "sidecar path {path:?} does not address an AST node"
        )));
    }
    Ok(node)
}

pub(crate) fn structural_field(field: &str) -> bool {
    matches!(
        field,
        "children"
            | "blocks"
            | "content"
            | "pairs"
            | "rows"
            | "cells"
            | "items"
            | "terms"
            | "definitions"
            | "target"
            | "fallback"
            | "caption"
            | "shortCaption"
            | "title"
            | "base"
            | "annotation"
            | "old"
            | "new"
            | "prefix"
            | "locator"
            | "suffix"
            | "inline"
    )
}

fn valid_pointer(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if !path.starts_with('/') {
        return false;
    }
    let mut chars = path.chars();
    while let Some(ch) = chars.next() {
        if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

fn nonempty(value: &str, name: &str) -> Result<(), AstSidecarError> {
    if value.is_empty() {
        Err(error(format!("{name} must not be empty")))
    } else {
        Ok(())
    }
}

fn version(version: u32) -> Result<(), AstSidecarError> {
    if version == 1 {
        Ok(())
    } else {
        Err(error(format!(
            "unsupported AST sidecar version {version}; implemented version is 1"
        )))
    }
}

fn validate_identity(sidecar: &NodeIdentity, ast: &Value) -> Result<(), AstSidecarError> {
    version(sidecar.version)?;
    nonempty(&sidecar.session, "session")?;
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for entry in &sidecar.nodes {
        nonempty(&entry.id, "node id")?;
        node_at(ast, &entry.path)?;
        if !ids.insert(&entry.id) || !paths.insert(&entry.path) {
            return Err(error("node identity ids and paths must be unique"));
        }
    }
    Ok(())
}

fn validate_ranges(sidecar: &AnnotationRanges, ast: &Value) -> Result<(), AstSidecarError> {
    version(sidecar.version)?;
    let metrics = text_metrics(ast);
    let mut ids = HashSet::new();
    for range in &sidecar.ranges {
        nonempty(&range.id, "range id")?;
        nonempty(&range.kind, "range kind")?;
        if !ids.insert(&range.id) {
            return Err(error("annotation range ids must be unique"));
        }
        let mut positions = Vec::new();
        for anchor in [&range.start, &range.end] {
            node_at(ast, &anchor.path)?;
            let (start, length) = metrics
                .get(&anchor.path)
                .ok_or_else(|| error("anchor does not address AST text"))?;
            if anchor.offset > *length {
                return Err(error(format!(
                    "anchor at {:?} exceeds node text",
                    anchor.path
                )));
            }
            positions.push(start + anchor.offset);
        }
        if positions[0] > positions[1] {
            return Err(error(format!("range {:?} ends before it starts", range.id)));
        }
    }
    Ok(())
}

const TEXT_FIELDS: &[&str] = &[
    "target",
    "title",
    "children",
    "items",
    "rows",
    "cells",
    "blocks",
    "inline",
    "content",
    "prefix",
    "locator",
    "suffix",
    "old",
    "new",
    "pairs",
    "base",
    "annotation",
    "caption",
    "shortCaption",
    "fallback",
];

fn text_metrics(ast: &Value) -> HashMap<String, (usize, usize)> {
    let mut metrics = HashMap::new();
    let mut cursor = 0;
    let mut pending = vec![(String::new(), ast, None)];
    while let Some((path, value, entered)) = pending.pop() {
        if let Some(start) = entered {
            metrics.insert(path, (start, cursor - start));
            continue;
        }
        match value {
            Value::Object(object) => {
                if let Some(kind) = object.get("type").and_then(Value::as_str) {
                    pending.push((path.clone(), value, Some(cursor)));
                    cursor += match kind {
                        "soft_break" | "hard_break" | "non_breaking_space" => 1,
                        _ => ["value", "content", "text", "alt"]
                            .iter()
                            .find_map(|key| object.get(*key).and_then(Value::as_str))
                            .map_or(0, |text| text.chars().count()),
                    };
                }
                for key in TEXT_FIELDS.iter().rev() {
                    if let Some(child) = object.get(*key) {
                        if child.is_object() || child.is_array() {
                            pending.push((format!("{path}/{key}"), child, None));
                        }
                    }
                }
            }
            Value::Array(values) => {
                for (index, child) in values.iter().enumerate().rev() {
                    pending.push((format!("{path}/{index}"), child, None));
                }
            }
            _ => {}
        }
    }
    metrics
}

fn validate_provenance(sidecar: &Provenance, ast: &Value) -> Result<(), AstSidecarError> {
    version(sidecar.version)?;
    let mut sources = HashMap::new();
    for source in &sidecar.sources {
        nonempty(&source.id, "source id")?;
        if let Some(uri) = &source.uri {
            nonempty(uri, "source uri")?;
        }
        if let Some(format) = &source.format {
            nonempty(format, "source format")?;
        }
        if !sources
            .insert(source.id.as_str(), source.parent.as_deref())
            .is_none()
        {
            return Err(error("provenance source ids must be unique"));
        }
    }
    for source in &sidecar.sources {
        let mut seen = HashSet::new();
        let mut current = Some(source.id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(error("provenance source parent cycle"));
            }
            current = *sources
                .get(id)
                .ok_or_else(|| error(format!("unknown source {id:?}")))?;
        }
    }
    let mut paths = HashSet::new();
    for node in &sidecar.nodes {
        node_at(ast, &node.path)?;
        if !paths.insert(&node.path) {
            return Err(error("provenance node paths must be unique"));
        }
        if !sources.contains_key(node.source.as_str()) {
            return Err(error(format!("unknown source {:?}", node.source)));
        }
        if node.start_byte.is_some() != node.end_byte.is_some()
            || node
                .start_byte
                .zip(node.end_byte)
                .is_some_and(|(a, b)| a > b)
        {
            return Err(error("provenance byte range is invalid"));
        }
        if let Some(step) = &node.step {
            nonempty(step, "provenance step")?;
        }
    }
    Ok(())
}

fn read<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T, AstSidecarError> {
    let value = parse_value(input).map_err(|err| error(err.to_string()))?;
    serde_json::from_value(value).map_err(|err| error(err.to_string()))
}

fn write<T: Serialize>(sidecar: &T) -> Result<String, AstSidecarError> {
    serde_json::to_string(sidecar).map_err(|err| error(err.to_string()))
}

pub fn from_node_identity_json(
    input: &str,
    doc: &Document,
) -> Result<NodeIdentity, AstSidecarError> {
    let sidecar: NodeIdentity = read(input)?;
    validate_identity(&sidecar, &ast_value(doc)?)?;
    Ok(sidecar)
}

pub fn to_node_identity_json(
    sidecar: &NodeIdentity,
    doc: &Document,
) -> Result<String, AstSidecarError> {
    validate_identity(sidecar, &ast_value(doc)?)?;
    write(sidecar)
}

pub fn from_annotation_ranges_json(
    input: &str,
    doc: &Document,
) -> Result<AnnotationRanges, AstSidecarError> {
    let sidecar: AnnotationRanges = read(input)?;
    validate_ranges(&sidecar, &ast_value(doc)?)?;
    Ok(sidecar)
}

pub fn to_annotation_ranges_json(
    sidecar: &AnnotationRanges,
    doc: &Document,
) -> Result<String, AstSidecarError> {
    validate_ranges(sidecar, &ast_value(doc)?)?;
    write(sidecar)
}

pub fn from_provenance_json(input: &str, doc: &Document) -> Result<Provenance, AstSidecarError> {
    let sidecar: Provenance = read(input)?;
    validate_provenance(&sidecar, &ast_value(doc)?)?;
    Ok(sidecar)
}

pub fn to_provenance_json(sidecar: &Provenance, doc: &Document) -> Result<String, AstSidecarError> {
    validate_provenance(sidecar, &ast_value(doc)?)?;
    write(sidecar)
}

/// Mint ids for one tree. The caller supplies a new opaque session token for
/// each fresh parse; reusing it after an edit would misidentify moved nodes.
pub fn fresh_node_identity(
    doc: &Document,
    session: impl Into<String>,
) -> Result<NodeIdentity, AstSidecarError> {
    let ast = ast_value(doc)?;
    let mut pending = vec![(String::new(), &ast)];
    let mut nodes = Vec::new();
    while let Some((path, value)) = pending.pop() {
        if value.get("type").and_then(Value::as_str).is_some() {
            nodes.push(NodeIdentityEntry {
                id: format!("n{}", nodes.len()),
                path: path.clone(),
            });
        }
        match value {
            Value::Object(object) => {
                for (key, child) in object.iter().rev().filter(|(key, _)| structural_field(key)) {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    pending.push((format!("{path}/{key}"), child));
                }
            }
            Value::Array(array) => {
                for (index, child) in array.iter().enumerate().rev() {
                    pending.push((format!("{path}/{index}"), child));
                }
            }
            _ => {}
        }
    }
    let sidecar = NodeIdentity {
        version: 1,
        session: session.into(),
        nodes,
    };
    validate_identity(&sidecar, &ast)?;
    Ok(sidecar)
}

/// Record measured byte spans for nodes parsed from one Carve input.
///
/// Nodes without a position, or with a different `pos.file`, are omitted.
/// Imported and generated nodes need provenance supplied by their producer.
pub fn authored_provenance(
    doc: &Document,
    source: &str,
    uri: Option<&str>,
) -> Result<Provenance, AstSidecarError> {
    if doc.source_len != source.len() {
        return Err(error(
            "source bytes do not match the parsed document length",
        ));
    }
    let ast = ast_value(doc)?;
    let byte_offsets = codepoint_bytes(source);
    let mut nodes = Vec::new();
    for entry in fresh_node_identity(doc, "provenance-walk")?.nodes {
        let value = node_at(&ast, &entry.path)?;
        let Some(pos) = value.get("pos") else {
            continue;
        };
        if pos.get("file").is_some() {
            continue;
        }
        let Some(start) = pos
            .get("startOffset")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
        else {
            continue;
        };
        let Some(end) = pos
            .get("endOffset")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
        else {
            continue;
        };
        if start > end || end >= byte_offsets.len() {
            continue;
        }
        nodes.push(NodeProvenance {
            path: entry.path,
            source: "s0".into(),
            start_byte: Some(byte_offsets[start]),
            end_byte: Some(byte_offsets[end]),
            origin: ProvenanceOrigin::Authored,
            step: None,
        });
    }
    let sidecar = Provenance {
        version: 1,
        sources: vec![ProvenanceSource {
            id: "s0".into(),
            uri: uri.map(str::to_owned),
            format: None,
            parent: None,
        }],
        nodes,
    };
    validate_provenance(&sidecar, &ast)?;
    Ok(sidecar)
}

/// Parse one source and return authored provenance only for measured nodes.
pub fn parse_with_authored_provenance(
    source: &str,
    uri: Option<&str>,
) -> Result<(Document, Provenance), AstSidecarError> {
    let (document, _) = crate::parse_with_source_layout(source);
    let provenance = authored_provenance(&document, source, uri)?;
    Ok((document, provenance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn identity_round_trip_and_version_refusal() {
        let doc = parse("alpha *beta*");
        let sidecar = fresh_node_identity(&doc, "session-1").unwrap();
        assert!(sidecar
            .nodes
            .iter()
            .any(|entry| entry.path == "/children/0"));
        let json = to_node_identity_json(&sidecar, &doc).unwrap();
        assert_eq!(from_node_identity_json(&json, &doc).unwrap(), sidecar);
        assert!(
            from_node_identity_json(&json.replace("\"version\":1", "\"version\":2"), &doc).is_err()
        );
        assert!(from_node_identity_json(&json.replace("\"session-1\"", "\"\""), &doc).is_err());
    }

    #[test]
    fn identity_reaches_ruby_pairs_and_inline_extension_content() {
        let doc = crate::from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[{"type":"ruby","pairs":[{"base":[{"type":"text","value":"base"}],"annotation":[{"type":"text","value":"note"}]}]},{"type":"inline_extension","name":"example.org/x","content":[{"type":"text","value":"inner"}]}]}]}"#).unwrap();
        let sidecar = fresh_node_identity(&doc, "session").unwrap();
        assert!(sidecar
            .nodes
            .iter()
            .any(|entry| entry.path == "/children/0/children/0/pairs/0/base/0"));
        assert!(sidecar
            .nodes
            .iter()
            .any(|entry| entry.path == "/children/0/children/1/content/0"));
        assert!(to_node_identity_json(&sidecar, &doc).is_ok());
    }

    #[test]
    fn ranges_overlap_and_reject_non_nodes() {
        let doc = parse("abcd");
        let ranges = AnnotationRanges {
            version: 1,
            ranges: vec![
                AnnotationRange {
                    id: "a".into(),
                    kind: "example.org/comment".into(),
                    start: AnnotationAnchor {
                        path: "/children/0/children/0".into(),
                        offset: 1,
                    },
                    end: AnnotationAnchor {
                        path: "/children/0/children/0".into(),
                        offset: 3,
                    },
                    data: None,
                },
                AnnotationRange {
                    id: "b".into(),
                    kind: "example.org/comment".into(),
                    start: AnnotationAnchor {
                        path: "/children/0/children/0".into(),
                        offset: 2,
                    },
                    end: AnnotationAnchor {
                        path: "/children/0/children/0".into(),
                        offset: 4,
                    },
                    data: None,
                },
            ],
        };
        let json = to_annotation_ranges_json(&ranges, &doc).unwrap();
        assert_eq!(from_annotation_ranges_json(&json, &doc).unwrap(), ranges);
        let mut invalid = ranges;
        invalid.ranges[0].start.path = "/children/0/children".into();
        assert!(to_annotation_ranges_json(&invalid, &doc).is_err());
        invalid.ranges[0].start.path = "/children/0".into();
        invalid.ranges[0].start.offset = 5;
        assert!(to_annotation_ranges_json(&invalid, &doc).is_err());
    }

    #[test]
    fn positioned_cross_node_range_cannot_run_backwards() {
        let (doc, _) = crate::parse_with_source_layout("first\n\nsecond");
        let ranges = AnnotationRanges {
            version: 1,
            ranges: vec![AnnotationRange {
                id: "r".into(),
                kind: "example.org/comment".into(),
                start: AnnotationAnchor {
                    path: "/children/0".into(),
                    offset: 5,
                },
                end: AnnotationAnchor {
                    path: "/children/1".into(),
                    offset: 0,
                },
                data: None,
            }],
        };
        assert!(to_annotation_ranges_json(&ranges, &doc).is_ok());
        let mut reversed = ranges;
        reversed.ranges[0].start.path = "/children/1".into();
        reversed.ranges[0].start.offset = 2;
        reversed.ranges[0].end.path = "/children/0".into();
        reversed.ranges[0].end.offset = 4;
        assert!(to_annotation_ranges_json(&reversed, &doc).is_err());
    }

    #[test]
    fn provenance_checks_sources_and_byte_pairs() {
        let doc = parse("hello");
        let sidecar = Provenance {
            version: 1,
            sources: vec![ProvenanceSource {
                id: "s0".into(),
                uri: Some("file:///doc.crv".into()),
                format: None,
                parent: None,
            }],
            nodes: vec![NodeProvenance {
                path: "/children/0".into(),
                source: "s0".into(),
                start_byte: Some(0),
                end_byte: Some(5),
                origin: ProvenanceOrigin::Authored,
                step: None,
            }],
        };
        let json = to_provenance_json(&sidecar, &doc).unwrap();
        assert_eq!(from_provenance_json(&json, &doc).unwrap(), sidecar);
        let mut invalid = sidecar;
        invalid.nodes[0].end_byte = None;
        assert!(to_provenance_json(&invalid, &doc).is_err());
    }

    #[test]
    fn authored_provenance_converts_codepoints_to_bytes() {
        let source = "écho";
        let (doc, _) = crate::parse_with_source_layout(source);
        let sidecar = authored_provenance(&doc, source, Some("file:///doc.crv")).unwrap();
        assert_eq!(sidecar.sources[0].uri.as_deref(), Some("file:///doc.crv"));
        assert!(sidecar
            .nodes
            .iter()
            .any(|node| node.end_byte == Some(source.len())));
        assert!(sidecar
            .nodes
            .iter()
            .all(|node| node.origin == ProvenanceOrigin::Authored));
    }

    #[test]
    fn payload_type_is_not_an_ast_node() {
        let doc = crate::from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"block_extension","name":"example.org/x","fallback":{"type":"paragraph","children":[]},"payload":{"format":"application/json","value":{"type":"not_a_node"}}}]}"#).unwrap();
        let sidecar = fresh_node_identity(&doc, "s1").unwrap();
        assert!(!sidecar
            .nodes
            .iter()
            .any(|entry| entry.path.contains("payload")));
        let injected = r#"{"version":1,"session":"s1","nodes":[{"id":"n1","path":"/children/0/payload/value"}]}"#;
        assert!(from_node_identity_json(injected, &doc).is_err());
    }
}
