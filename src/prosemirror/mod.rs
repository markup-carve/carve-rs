//! Conversion from the Carve AST to the ProseMirror/CarveKit JSON shape.

mod from_pm;
mod to_pm;

use std::collections::BTreeMap;
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
    /// Types whose entry says it exists for the profile vocabulary only - an
    /// admonition is a div with a type class, an autolink is a link whose text
    /// is its destination. They share a ProseMirror name with the type that
    /// owns it, so a reverse lookup has to prefer the owner.
    profile_only: std::collections::BTreeSet<String>,
    unmapped: BTreeMap<String, String>,
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
        let mut profile_only = std::collections::BTreeSet::new();
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
            if matches!(entry.get("notes"), Some(Json::String(n)) if n.starts_with("profile vocabulary only"))
            {
                profile_only.insert(ty.clone());
            }
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
        SchemaMap {
            names,
            accepts,
            profile_only,
            unmapped,
        }
    })
}
