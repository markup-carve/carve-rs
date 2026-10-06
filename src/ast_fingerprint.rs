use std::collections::HashMap;

use serde_json::Value;

// Intern complete structural keys; hash collisions never imply equal trees.
#[derive(Hash, PartialEq, Eq)]
enum Key {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    SerializedNumber(String),
    String(String),
    Array(Vec<usize>),
    Object(Vec<(String, usize)>),
}

pub(crate) struct Fingerprints<'a> {
    roots: Vec<&'a Value>,
    keys: HashMap<Key, usize>,
    ids: HashMap<(usize, bool), usize>,
    strip_source_length: bool,
    preserve_key_values: bool,
}

impl<'a> Fingerprints<'a> {
    pub(crate) fn new(strip_source_length: bool, preserve_key_values: bool) -> Self {
        Self {
            roots: Vec::new(),
            keys: HashMap::new(),
            ids: HashMap::new(),
            strip_source_length,
            preserve_key_values,
        }
    }

    /// The value must belong to an added root under the same metadata policy.
    pub(crate) fn id(&self, value: &Value, strip: bool) -> usize {
        self.ids[&(value as *const Value as usize, strip)]
    }

    pub(crate) fn add(&mut self, root: &'a Value) {
        self.roots.push(root);
        let mut pending = vec![(root, true, false)];
        while let Some((value, strip, entered)) = pending.pop() {
            let address = (value as *const Value as usize, strip);
            if self.ids.contains_key(&address) {
                continue;
            }
            let keep = |key: &str| {
                !strip || (key != "pos" && (!self.strip_source_length || key != "srcByteLength"))
            };
            let child_strip =
                |key: &str| strip && !(self.preserve_key_values && key == "keyValues");
            if !entered {
                pending.push((value, strip, true));
                match value {
                    Value::Array(values) => {
                        pending.extend(values.iter().map(|child| (child, strip, false)));
                    }
                    Value::Object(values) => {
                        pending.extend(
                            values
                                .iter()
                                .filter(|(key, _)| keep(key))
                                .map(|(key, child)| (child, child_strip(key), false)),
                        );
                    }
                    _ => {}
                }
                continue;
            }
            let key = match value {
                Value::Null => Key::Null,
                Value::Bool(value) => Key::Bool(*value),
                Value::Number(value) if self.strip_source_length => Key::Number(value.clone()),
                Value::Number(value) => Key::SerializedNumber(value.to_string()),
                Value::String(value) => Key::String(value.clone()),
                Value::Array(values) => {
                    Key::Array(values.iter().map(|child| self.id(child, strip)).collect())
                }
                Value::Object(values) => Key::Object(
                    values
                        .iter()
                        .filter(|(key, _)| keep(key))
                        .map(|(key, child)| (key.clone(), self.id(child, child_strip(key))))
                        .collect(),
                ),
            };
            let next = self.keys.len();
            let id = *self.keys.entry(key).or_insert(next);
            self.ids.insert(address, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identity_and_merge_keep_their_distinct_metadata_rules() {
        let a = json!({"pos": 1, "attrs": {"keyValues": {"pos": "a", "srcByteLength": "kept"}}});
        let b = json!({"pos": 2, "attrs": {"keyValues": {"pos": "b", "srcByteLength": "kept"}}});
        let mut identity = Fingerprints::new(false, false);
        identity.add(&a);
        identity.add(&b);
        assert_eq!(identity.id(&a, true), identity.id(&b, true));
        let mut merging = Fingerprints::new(true, true);
        merging.add(&a);
        merging.add(&b);
        assert_ne!(merging.id(&a, true), merging.id(&b, true));
        let c = json!({"pos": 3, "attrs": {"keyValues": {"pos": "a", "srcByteLength": "kept"}}});
        merging.add(&c);
        assert_eq!(merging.id(&a, true), merging.id(&c, true));
    }

    #[test]
    fn merge_numbers_use_json_equality_and_identity_uses_serialized_spelling() {
        let a = json!(0.0);
        let b = json!(-0.0);
        let mut merging = Fingerprints::new(true, true);
        merging.add(&a);
        merging.add(&b);
        assert_eq!(merging.id(&a, true), merging.id(&b, true));
        let mut identity = Fingerprints::new(false, false);
        identity.add(&a);
        identity.add(&b);
        assert_ne!(identity.id(&a, true), identity.id(&b, true));
    }

    #[test]
    fn deep_trees_intern_each_value_once_and_share_equal_subtrees() {
        let build = |depth: usize, position: usize| {
            let mut value = json!({"type": "text", "value": "leaf"});
            for _ in 0..depth {
                value = json!({"type": "block_quote", "children": [value], "pos": position});
            }
            value
        };
        for depth in [25, 50, 190] {
            let a = build(depth, 1);
            let b = build(depth, 2);
            let mut index = Fingerprints::new(true, true);
            index.add(&a);
            assert_eq!(index.ids.len(), 3 * depth + 3);
            let distinct = index.keys.len();
            index.add(&b);
            assert_eq!(index.ids.len(), 2 * (3 * depth + 3));
            assert_eq!(index.keys.len(), distinct);
            assert_eq!(index.id(&a, true), index.id(&b, true));
        }
    }
}
