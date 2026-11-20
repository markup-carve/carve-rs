//! A read-only view of the arena tree with the `serde_json::Value` read API the
//! lint rules use. Keys, entry order and null handling follow the tree's serde
//! output (`build.rs` generates the node lookups), so a rule reads exactly what
//! it read from `serde_json::to_value` of the tree, without building that copy.

use crate::ast::{Attrs, Document, Marker, Node, Pos, Task};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(crate) enum V<'a> {
    Null,
    Doc(&'a Document<'a>),
    Node(&'a Node<'a>),
    Nodes(&'a [Node<'a>]),
    Str(&'a str),
    Int(i64),
    #[cfg_attr(not(test), allow(dead_code))]
    Num(f64),
    Bool(bool),
    Strs(&'a [&'a str]),
    Pos(&'a Pos),
    Segments(&'a [Pos]),
    Attrs(&'a Attrs<'a>),
    Pairs(&'a [(&'a str, &'a str)]),
    Pair(&'a (&'a str, &'a str)),
    Marker(&'a Marker<'a>),
    Task(&'a Task<'a>),
    Json(&'a Value),
}

impl<'a> V<'a> {
    /// `serde_json` writes a non-finite float as `null`.
    pub(crate) const fn num(value: f64) -> Self {
        if value.is_finite() {
            Self::Num(value)
        } else {
            Self::Null
        }
    }

    pub(crate) fn get(self, key: &str) -> Option<Self> {
        match self {
            Self::Doc(doc) => doc.view_get(key),
            Self::Node(node) => match key {
                "type" => Some(Self::Str(node.type_name())),
                "pos" => node.pos().map(Self::Pos),
                "attrs" => node.attrs().map(Self::Attrs),
                _ => node.view_get(key),
            },
            Self::Json(value) => value.get(key).map(Self::Json),
            Self::Pos(pos) => pos_field(pos, key).map(|n| Self::Int(i64::from(n))),
            _ => self
                .value_fields()?
                .into_iter()
                .find(|(name, _)| *name == key)
                .and_then(|(_, value)| value),
        }
    }

    /// The keys of a position, attrs, marker or task in `serde_json` map order, with the values serde
    /// writes; absent keys are `None`, padding is an empty key.
    fn value_fields(self) -> Option<[(&'static str, Option<Self>); 6]> {
        let none: (&'static str, Option<Self>) = ("", None);
        let int = |n: u32| Some(Self::Int(i64::from(n)));
        Some(match self {
            Self::Pos(pos) => [
                ("endColumn", int(pos.end_column)),
                ("endLine", int(pos.end_line)),
                ("endOffset", int(pos.end_offset)),
                ("startColumn", int(pos.start_column)),
                ("startLine", int(pos.start_line)),
                ("startOffset", int(pos.start_offset)),
            ],
            Self::Attrs(attrs) => [
                (
                    "classes",
                    (!attrs.classes.is_empty()).then_some(Self::Strs(attrs.classes)),
                ),
                ("id", attrs.id.map(Self::Str)),
                (
                    "keyValues",
                    (!attrs.key_values.is_empty()).then_some(Self::Pairs(attrs.key_values)),
                ),
                none,
                none,
                none,
            ],
            Self::Marker(marker) => [
                ("char", marker.bullet.map(Self::Str)),
                ("delimiter", marker.delimiter.map(Self::Str)),
                ("kind", Some(Self::Str(marker.kind))),
                ("start", marker.start.map(Self::Int)),
                ("style", marker.style.map(Self::Str)),
                none,
            ],
            Self::Task(task) => [
                ("mark", Some(Self::Str(task.mark))),
                none,
                none,
                none,
                none,
                none,
            ],
            _ => return None,
        })
    }

    /// `value["type"]`, read off a parsed node without a key lookup.
    pub(crate) fn ty(self) -> Self {
        match self {
            Self::Node(node) => Self::Str(node.type_name()),
            Self::Doc(_) => Self::Str("document"),
            _ => self.at("type"),
        }
    }

    /// `value.get("pos")`, read off a parsed node without a key lookup.
    pub(crate) fn pos(self) -> Option<Self> {
        match self {
            Self::Node(node) => node.pos().map(Self::Pos),
            Self::Doc(doc) => doc.pos.as_ref().map(Self::Pos),
            _ => self.get("pos"),
        }
    }

    /// The start and end offsets of `pos`.
    pub(crate) fn offsets(self) -> Option<(usize, usize)> {
        match self.pos()? {
            Self::Pos(pos) => Some((pos.start(), pos.end())),
            pos => Some((
                usize::try_from(pos.at("startOffset").as_u64()?).ok()?,
                usize::try_from(pos.at("endOffset").as_u64()?).ok()?,
            )),
        }
    }

    /// `value[i]` of an array: the item, or null.
    pub(crate) fn item(self, at: usize) -> Self {
        self.as_array()
            .and_then(|items| items.get(at))
            .unwrap_or(Self::Null)
    }

    /// `value[key]`: the field, or null.
    pub(crate) fn at(self, key: &str) -> Self {
        self.get(key).unwrap_or(Self::Null)
    }

    /// The value at a `/`-separated path of keys and decimal indices.
    pub(crate) fn lookup(mut self, path: &str) -> Option<Self> {
        for part in path.strip_prefix('/')?.split('/') {
            self = if let Some(items) = self.as_array() {
                items.get(part.parse::<usize>().ok()?)?
            } else {
                self.get(part)?
            };
        }
        Some(self)
    }

    /// [`Self::lookup`], or null.
    pub(crate) fn field(self, path: &str) -> Self {
        self.lookup(path).unwrap_or(Self::Null)
    }

    pub(crate) fn as_array(self) -> Option<Arr<'a>> {
        match self {
            Self::Nodes(nodes) => Some(Arr::Nodes(nodes)),
            Self::Strs(strs) => Some(Arr::Strs(strs)),
            Self::Segments(segments) => Some(Arr::Segments(segments)),
            Self::Pairs(pairs) => Some(Arr::Pairs(pairs)),
            Self::Pair(pair) => Some(Arr::Pair(pair)),
            Self::Json(Value::Array(items)) => Some(Arr::Json(items)),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> Option<&'a str> {
        match self {
            Self::Str(text) => Some(text),
            Self::Json(value) => value.as_str(),
            _ => None,
        }
    }

    pub(crate) fn as_u64(self) -> Option<u64> {
        match self {
            Self::Int(n) => u64::try_from(n).ok(),
            Self::Json(value) => value.as_u64(),
            _ => None,
        }
    }

    pub(crate) fn as_bool(self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(b),
            Self::Json(value) => value.as_bool(),
            _ => None,
        }
    }

    pub(crate) const fn is_object(self) -> bool {
        matches!(
            self,
            Self::Doc(_)
                | Self::Node(_)
                | Self::Pos(_)
                | Self::Attrs(_)
                | Self::Marker(_)
                | Self::Task(_)
                | Self::Json(Value::Object(_))
        )
    }

    pub(crate) fn is_array(self) -> bool {
        self.as_array().is_some()
    }

    /// An object or an array: a value a tree walk descends into.
    pub(crate) fn is_container(self) -> bool {
        self.is_object() || self.is_array()
    }

    /// Calls `f` with each key and value of an object, in `serde_json` map order.
    pub(crate) fn each(self, f: &mut dyn FnMut(&'a str, Self)) {
        match self {
            Self::Doc(doc) => doc.view_each(f),
            Self::Node(node) => node.view_each(f),
            Self::Json(Value::Object(map)) => {
                for (key, value) in map {
                    f(key, Self::Json(value));
                }
            }
            _ => {
                for (key, value) in self.value_fields().into_iter().flatten() {
                    if let Some(value) = value {
                        f(key, value);
                    }
                }
            }
        }
    }

    /// The number of keys of an object.
    pub(crate) fn object_len(self) -> Option<usize> {
        if !self.is_object() {
            return None;
        }
        let mut n = 0;
        self.each(&mut |_, _| n += 1);
        Some(n)
    }
}

fn pos_field(pos: &Pos, key: &str) -> Option<u32> {
    Some(match key {
        "startLine" => pos.start_line,
        "endLine" => pos.end_line,
        "startColumn" => pos.start_column,
        "endColumn" => pos.end_column,
        "startOffset" => pos.start_offset,
        "endOffset" => pos.end_offset,
        _ => return None,
    })
}

impl PartialEq<&str> for V<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl PartialEq<str> for V<'_> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<bool> for V<'_> {
    fn eq(&self, other: &bool) -> bool {
        self.as_bool() == Some(*other)
    }
}

/// An array of the tree.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Arr<'a> {
    Nodes(&'a [Node<'a>]),
    Strs(&'a [&'a str]),
    Segments(&'a [Pos]),
    Pairs(&'a [(&'a str, &'a str)]),
    Pair(&'a (&'a str, &'a str)),
    Json(&'a [Value]),
}

impl<'a> Arr<'a> {
    pub(crate) const fn empty() -> Self {
        Self::Json(&[])
    }

    pub(crate) const fn len(self) -> usize {
        match self {
            Self::Nodes(items) => items.len(),
            Self::Strs(items) => items.len(),
            Self::Segments(items) => items.len(),
            Self::Pairs(items) => items.len(),
            Self::Pair(_) => 2,
            Self::Json(items) => items.len(),
        }
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub(crate) fn get(self, at: usize) -> Option<V<'a>> {
        match self {
            Self::Nodes(items) => items.get(at).map(V::Node),
            Self::Strs(items) => items.get(at).map(|s| V::Str(s)),
            Self::Segments(items) => items.get(at).map(V::Pos),
            Self::Pairs(items) => items.get(at).map(V::Pair),
            Self::Pair((key, value)) => match at {
                0 => Some(V::Str(key)),
                1 => Some(V::Str(value)),
                _ => None,
            },
            Self::Json(items) => items.get(at).map(V::Json),
        }
    }

    /// The only item.
    pub(crate) fn single(self) -> Option<V<'a>> {
        if self.len() == 1 { self.first() } else { None }
    }

    pub(crate) fn first(self) -> Option<V<'a>> {
        self.get(0)
    }

    pub(crate) const fn iter(self) -> ArrIter<'a> {
        ArrIter {
            arr: self,
            front: 0,
            back: self.len(),
        }
    }
}

impl<'a> IntoIterator for Arr<'a> {
    type Item = V<'a>;
    type IntoIter = ArrIter<'a>;

    fn into_iter(self) -> ArrIter<'a> {
        self.iter()
    }
}

#[derive(Clone)]
pub(crate) struct ArrIter<'a> {
    arr: Arr<'a>,
    front: usize,
    back: usize,
}

impl<'a> Iterator for ArrIter<'a> {
    type Item = V<'a>;

    fn next(&mut self) -> Option<V<'a>> {
        if self.front >= self.back {
            return None;
        }
        self.front += 1;
        self.arr.get(self.front - 1)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.back - self.front;
        (n, Some(n))
    }
}

impl DoubleEndedIterator for ArrIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        self.back -= 1;
        self.arr.get(self.back)
    }
}

impl ExactSizeIterator for ArrIter<'_> {}

#[cfg(test)]
mod tests {
    use super::V;
    use serde_json::{Map, Value};

    fn to_json(value: V<'_>) -> Value {
        if let Some(items) = value.as_array() {
            return Value::Array(items.iter().map(to_json).collect());
        }
        if value.is_object() {
            let mut map = Map::new();
            value.each(&mut |key, child| {
                let json = to_json(child);
                assert_eq!(value.get(key).map(to_json).as_ref(), Some(&json), "{key}");
                map.insert(key.to_string(), json);
            });
            return Value::Object(map);
        }
        match value {
            V::Str(text) => Value::String(text.to_string()),
            V::Int(n) => Value::from(n),
            V::Num(n) => Value::from(n),
            V::Bool(b) => Value::Bool(b),
            V::Json(json) => json.clone(),
            _ => Value::Null,
        }
    }

    #[test]
    fn view_reads_what_serde_writes() {
        let cases: Value = serde_json::from_str(include_str!("../../tests/data/lint-cases.json"))
            .unwrap_or_default();
        let mut inputs: Vec<String> = cases
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|case| [case["trigger"].as_str(), case["near"].as_str()])
            .flatten()
            .map(str::to_string)
            .collect();
        inputs.push(
            "---\ntitle: x\n---\n\n{#t .c k=v}\n|= a |= b |\n| *x* {-2-} | `c` |\n\n- [x] one\n- [ ] two\n\n3) a\n4) b\n\n\
             Text[^n] and </#t> [r][] <https://x.y> $`m`\n\n[r]: /u\n[^n]: note\n\n::: note\nbody\n:::\n"
                .to_string(),
        );
        for input in &inputs {
            let (checked, _) = crate::with_lint_tree(
                input,
                &crate::Options::default(),
                &crate::Options::default(),
                true,
                |tree, _| {
                    assert_eq!(
                        tree.map(to_json),
                        crate::tree_json_value(input, &crate::Options::default()),
                        "{input}"
                    );
                    tree.is_some()
                },
            );
            assert!(checked, "{input}");
        }
        assert!(inputs.len() > 100);
    }
}
