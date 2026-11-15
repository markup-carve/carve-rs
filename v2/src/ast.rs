//! The typed AST 2.0 (`ast.md`). The node structs and the `Node` enum are
//! generated from `spec-data/ast-schema.json` by `build.rs`; this module adds
//! the shared value types the schema defines inline.

use serde::{Deserialize, Serialize};

/// `attrs` of a node (C2-A-032): id, classes and the ordered key pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attrs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(rename = "keyValues", default, skip_serializing_if = "Vec::is_empty")]
    pub key_values: Vec<(String, String)>,
}

impl Attrs {
    /// True when the block holds no part.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.key_values.is_empty()
    }

    /// The value of a key, compared without ASCII case (C2-A-035).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.key_values
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// Merges `later` into `self` (C2-A-035, C2-I-039): classes add up, a later
    /// id wins, a later key replaces the value at the first key's place.
    pub fn merge(&mut self, later: Attrs) {
        if later.id.is_some() {
            self.id = later.id;
        }
        self.classes.extend(later.classes);
        for (k, v) in later.key_values {
            if let Some(slot) = self
                .key_values
                .iter_mut()
                .find(|(ek, _)| ek.eq_ignore_ascii_case(&k))
            {
                slot.1 = v;
            } else {
                self.key_values.push((k, v));
            }
        }
    }
}

/// A source range (C2-A-140).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pos {
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

/// `list.marker` (C2-A-054, C2-A-056).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    pub kind: String,
    #[serde(rename = "char", default, skip_serializing_if = "Option::is_none")]
    pub bullet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
}

/// `list_item.task` (C2-A-060).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub mark: String,
}

include!(concat!(env!("OUT_DIR"), "/ast_gen.rs"));
