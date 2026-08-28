//! Conversion from the Carve AST to the ProseMirror/CarveKit JSON shape.

mod from_pm;
mod to_pm;

use serde_json::Map;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use crate::ast_json::{parse_value, Json};

pub use from_pm::{from_prosemirror, ProseMirrorError};
pub use to_pm::to_prosemirror;

/// The result of converting a Carve document for a ProseMirror editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseMirrorDoc {
    /// The ProseMirror document, JSON-encoded.
    pub json: String,
    /// Carve AST type -> why its content is gone.
    pub dropped: BTreeMap<String, String>,
    /// Carve AST type -> why its node type is gone while its text survives.
    pub degraded: BTreeMap<String, String>,
}

struct SchemaMap {
    names: BTreeMap<String, Vec<String>>,
    accepts: BTreeMap<String, Vec<String>>,
    unmapped: BTreeMap<String, String>,
    /// The carrier node for a mark with no content, from `markCarrierNodes`.
    ///
    /// Found by the attribute contract rather than by the key, because the
    /// section is keyed BY the ProseMirror name and looking one up by key is
    /// the same thing as writing the name in this file. The entry that
    /// declares `markType` is the one that stands in for a mark.
    mark_carrier: Option<String>,
    /// The `preservationNodes` names, so a payload carrying one is answered
    /// with what it is rather than with "unknown node".
    preservation: BTreeSet<String>,
}

fn schema_map() -> &'static SchemaMap {
    static MAP: OnceLock<SchemaMap> = OnceLock::new();
    MAP.get_or_init(|| {
        let value = parse_value(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/prosemirror-schema-map.json"
        )))
        .expect("the vendored ProseMirror schema map is valid JSON");
        let Json::Object(root) = value else {
            panic!("schema map root is not an object")
        };
        let Json::Object(types) = root.get("types").expect("schema map has types") else {
            panic!("schema map types is not an object")
        };
        let mut names = BTreeMap::new();
        let mut accepts = BTreeMap::new();
        for (ty, entry) in types {
            let Json::Object(entry) = entry else { continue };
            let pm = match entry.get("pm") {
                Some(Json::String(name)) => vec![name.clone()],
                Some(Json::Array(values)) => values
                    .iter()
                    .filter_map(|v| match v {
                        Json::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            names.insert(ty.clone(), pm);
            let accepted = match entry.get("accepts") {
                Some(Json::Array(values)) => values
                    .iter()
                    .filter_map(|v| match v {
                        Json::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            accepts.insert(ty.clone(), accepted);
        }
        let Json::Object(unmapped_values) = root.get("unmapped").expect("schema map has unmapped")
        else {
            panic!("schema map unmapped is not an object")
        };
        let unmapped = unmapped_values
            .iter()
            .filter_map(|(k, v)| match v {
                Json::String(s) => Some((k.clone(), s.clone())),
                _ => None,
            })
            .collect();
        // `types` is not the whole map. Two further sections name nodes that
        // are part of the wire without being Carve types, and a bridge that
        // reads only `types` refuses both of them as unknown.
        let section = |key: &str| -> BTreeMap<String, Map<String, Json>> {
            match root.get(key) {
                Some(Json::Object(entries)) => entries
                    .iter()
                    .filter_map(|(name, entry)| match entry {
                        Json::Object(fields) => Some((name.clone(), fields.clone())),
                        _ => None,
                    })
                    .collect(),
                _ => BTreeMap::new(),
            }
        };
        let mark_carrier = section("markCarrierNodes")
            .into_iter()
            .find(|(_, entry)| match entry.get("attrs") {
                Some(Json::Object(attrs)) => attrs.contains_key("markType"),
                _ => false,
            })
            .map(|(name, _)| name);
        let preservation = section("preservationNodes")
            .into_iter()
            .filter(|(_, entry)| entry.contains_key("attrs"))
            .map(|(name, _)| name)
            .collect();
        SchemaMap {
            names,
            accepts,
            unmapped,
            mark_carrier,
            preservation,
        }
    })
}
