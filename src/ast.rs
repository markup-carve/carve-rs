//! The typed AST 2.0 (`ast.md`). The node structs and the `Node` enum are
//! generated from `spec-data/ast-schema.json` by `build.rs` in two families:
//! the arena tree here, which borrows its strings and nodes from an
//! [`Arena`](crate::Arena) and the source, and the owned tree in [`owned`].
//! This module adds the value types the schema defines inline.

use serde::{Deserialize, Serialize};

/// A source range (C2-A-140). The fields are `u32`, which halves the size
/// of every position; a value past `u32::MAX` saturates (see [`Pos::at`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pos {
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
    pub start_offset: u32,
    pub end_offset: u32,
}

impl Pos {
    /// A field value from a count, saturated at `u32::MAX`.
    #[must_use]
    pub fn at(n: usize) -> u32 {
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// The start offset as an index.
    #[must_use]
    pub fn start(&self) -> usize {
        usize::try_from(self.start_offset).unwrap_or(usize::MAX)
    }

    /// The end offset as an index.
    #[must_use]
    pub fn end(&self) -> usize {
        usize::try_from(self.end_offset).unwrap_or(usize::MAX)
    }
}

fn empty<T>(s: &&[T]) -> bool {
    s.is_empty()
}

/// `attrs` of a node (C2-A-032): id, classes and the ordered key pairs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Attrs<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<&'a str>,
    #[serde(skip_serializing_if = "empty")]
    pub classes: &'a [&'a str],
    #[serde(rename = "keyValues", skip_serializing_if = "empty")]
    pub key_values: &'a [(&'a str, &'a str)],
}

impl<'a> Attrs<'a> {
    /// True when the block holds no part.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.key_values.is_empty()
    }

    /// The value of a key, compared without ASCII case (C2-A-035).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&'a str> {
        self.key_values
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }

    /// The attributes as an owned value.
    #[must_use]
    pub fn owned(&self) -> owned::Attrs {
        owned::Attrs {
            id: self.id.map(str::to_string),
            classes: self.classes.iter().map(|c| (*c).to_string()).collect(),
            key_values: self
                .key_values
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }
}

/// `list.marker` (C2-A-054, C2-A-056).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Marker<'a> {
    pub kind: &'a str,
    #[serde(rename = "char", skip_serializing_if = "Option::is_none")]
    pub bullet: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
}

impl Marker<'_> {
    /// The marker as an owned value.
    #[must_use]
    pub fn owned(&self) -> owned::Marker {
        owned::Marker {
            kind: self.kind.to_string(),
            bullet: self.bullet.map(str::to_string),
            style: self.style.map(str::to_string),
            delimiter: self.delimiter.map(str::to_string),
            start: self.start,
        }
    }
}

/// `list_item.task` (C2-A-060).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Task<'a> {
    pub mark: &'a str,
}

impl Task<'_> {
    /// The task as an owned value.
    #[must_use]
    pub fn owned(&self) -> owned::Task {
        owned::Task {
            mark: self.mark.to_string(),
        }
    }
}

/// The owned tree: the same types with owned strings, boxed nodes and `Vec`
/// child lists, for trees that are kept, cloned or deserialized.
pub mod owned {
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
            self.key_values.extend(later.key_values);
            dedupe_keys_semantic(&mut self.key_values);
        }
    }

    /// A lower-case semantic span key is distinct from its ordinary upper-case spelling (C2-H-013).
    pub fn dedupe_keys_semantic(kv: &mut Vec<(String, String)>) {
        let mut out: Vec<(String, String)> = Vec::with_capacity(kv.len());
        if kv.len() <= 16 {
            for (key, value) in kv.drain(..) {
                let folded = key.to_ascii_lowercase();
                let semantic = ["abbr", "time", "kbd"].contains(&folded.as_str());
                let exact = key == folded;
                let slot = out.iter_mut().find(|(old, _)| {
                    old.eq_ignore_ascii_case(&key)
                        && (!semantic || (old == &old.to_ascii_lowercase()) == exact)
                });
                match slot {
                    Some((_, old_value)) => *old_value = value,
                    None => out.push((key, value)),
                }
            }
        } else {
            let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for (key, value) in kv.drain(..) {
                let mut folded = key.to_ascii_lowercase();
                if ["abbr", "time", "kbd"].contains(&folded.as_str()) && key != folded {
                    folded.push('~');
                }
                if let Some(slot) = at.get(&folded).and_then(|&i| out.get_mut(i)) {
                    slot.1 = value;
                } else {
                    at.insert(folded, out.len());
                    out.push((key, value));
                }
            }
        }
        *kv = out;
    }

    /// Merges keys that are equal without ASCII case: the later value at the
    /// first key's place (C2-A-035). Linear: a long list goes through a map.
    pub fn dedupe_keys(kv: &mut Vec<(String, String)>) {
        let mut out: Vec<(String, String)> = Vec::with_capacity(kv.len());
        if kv.len() <= 16 {
            for (k, v) in kv.drain(..) {
                match out.iter_mut().find(|(ek, _)| ek.eq_ignore_ascii_case(&k)) {
                    Some(slot) => slot.1 = v,
                    None => out.push((k, v)),
                }
            }
        } else {
            let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for (k, v) in kv.drain(..) {
                if let Some(slot) = at
                    .get(&k.to_ascii_lowercase())
                    .and_then(|&i| out.get_mut(i))
                {
                    slot.1 = v;
                } else {
                    at.insert(k.to_ascii_lowercase(), out.len());
                    out.push((k, v));
                }
            }
        }
        *kv = out;
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

    impl Attrs {
        /// The attributes copied into an arena.
        #[must_use]
        pub fn in_arena<'a>(&self, a: &'a crate::Arena) -> super::Attrs<'a> {
            super::Attrs {
                id: self.id.as_deref().map(|s| a.str(s)),
                classes: a.strs(&self.classes),
                key_values: a.pairs(&self.key_values),
            }
        }
    }

    impl Marker {
        /// The marker copied into an arena.
        #[must_use]
        pub fn in_arena<'a>(&self, a: &'a crate::Arena) -> super::Marker<'a> {
            super::Marker {
                kind: a.str(&self.kind),
                bullet: self.bullet.as_deref().map(|s| a.str(s)),
                style: self.style.as_deref().map(|s| a.str(s)),
                delimiter: self.delimiter.as_deref().map(|s| a.str(s)),
                start: self.start,
            }
        }
    }

    impl Task {
        /// The task copied into an arena.
        #[must_use]
        pub fn in_arena<'a>(&self, a: &'a crate::Arena) -> super::Task<'a> {
            super::Task {
                mark: a.str(&self.mark),
            }
        }
    }

    include!(concat!(env!("OUT_DIR"), "/ast_owned_gen.rs"));
}

include!(concat!(env!("OUT_DIR"), "/ast_gen.rs"));
include!(concat!(env!("OUT_DIR"), "/ast_conv_gen.rs"));

// Generated field lookups for the lint view: one closure per field keeps the generator uniform.
#[allow(
    clippy::wildcard_imports,
    clippy::redundant_closure,
    clippy::cognitive_complexity
)]
mod view_gen {
    use super::*;
    use crate::view::V;

    include!(concat!(env!("OUT_DIR"), "/ast_view_gen.rs"));
}
