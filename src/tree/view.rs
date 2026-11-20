//! One read interface over a tree's JSON value and over the owned typed tree,
//! so the writer and lint can walk either: a field by its schema name, the
//! node type, the children, positions and attributes. The typed side is
//! generated (`build.rs`), so both answer every field alike.
// The typed side has no caller until the writer moves onto it (carve-rs#4511).
#![cfg_attr(not(test), allow(dead_code))]

use crate::ast::Pos;
use crate::ast::owned::{Attrs, Document, Marker, Node, Task};
use serde_json::Value;

/// A field of a node, by its wire form.
#[derive(Debug, Clone, Copy)]
pub enum Field<'t, V: View<'t>> {
    Absent,
    Null,
    Str(&'t str),
    Int(i64),
    Num(f64),
    Bool(bool),
    Node(V),
    Nodes(V::List),
    /// A list of strings of the typed tree; the JSON side gives [`Field::Json`].
    Strings(&'t [String]),
    Pos(Pos),
    Segments(&'t [Pos]),
    Attrs(&'t Attrs),
    Marker(&'t Marker),
    Task(&'t Task),
    /// Any other value, or an object or list that holds no nodes.
    Json(&'t Value),
}

/// The nodes of one slot.
pub trait NodeList<'t, V>: Copy {
    fn len(self) -> usize;
    fn get(self, i: usize) -> Option<V>;
}

impl<'t> NodeList<'t, &'t Value> for &'t [Value] {
    fn len(self) -> usize {
        <[Value]>::len(self)
    }
    fn get(self, i: usize) -> Option<&'t Value> {
        <[Value]>::get(self, i)
    }
}

impl<'t> NodeList<'t, Typed<'t>> for &'t [Node] {
    fn len(self) -> usize {
        <[Node]>::len(self)
    }
    fn get(self, i: usize) -> Option<Typed<'t>> {
        <[Node]>::get(self, i).map(Typed::Node)
    }
}

/// A node or a reference to one, as the accessor functions take it.
pub trait AsView<'t> {
    type V: View<'t>;
    fn view(self) -> Self::V;
}

impl<'t, V: View<'t>> AsView<'t> for V {
    type V = V;
    fn view(self) -> V {
        self
    }
}

impl<'t> AsView<'t> for &&'t Value {
    type V = &'t Value;
    fn view(self) -> &'t Value {
        self
    }
}

impl<'t> AsView<'t> for &mut &'t Value {
    type V = &'t Value;
    fn view(self) -> &'t Value {
        self
    }
}

/// A node as the writer reads it.
pub trait View<'t>: Copy + 't {
    type List: NodeList<'t, Self>;

    fn empty() -> Self::List;
    /// The node type; empty for a value that is not a node.
    fn ty(self) -> &'t str;
    fn field(self, name: &str) -> Field<'t, Self>;
    /// The identity of a node, stable while the tree is borrowed.
    fn id(self) -> usize;

    fn children(self) -> Self::List {
        match self.field("children") {
            Field::Nodes(l) => l,
            _ => Self::empty(),
        }
    }
    fn list(self, name: &str) -> Option<Self::List> {
        match self.field(name) {
            Field::Nodes(l) => Some(l),
            _ => None,
        }
    }
    fn s(self, name: &str) -> Option<&'t str> {
        match self.field(name) {
            Field::Str(s) => Some(s),
            _ => None,
        }
    }
    fn flag(self, name: &str) -> bool {
        matches!(self.field(name), Field::Bool(true))
    }
    fn int(self, name: &str) -> Option<i64> {
        match self.field(name) {
            Field::Int(n) => Some(n),
            _ => None,
        }
    }
    /// Whether the field is set and not null.
    fn has(self, name: &str) -> bool {
        !matches!(self.field(name), Field::Absent | Field::Null)
    }
    fn node(self, name: &str) -> Option<Self> {
        match self.field(name) {
            Field::Node(n) => Some(n),
            _ => None,
        }
    }
    fn pos(self) -> Option<Pos>;
    /// The start and end byte of `pos`.
    fn span(self) -> Option<(usize, usize)> {
        let pos = self.pos()?;
        Some((pos.start(), pos.end()))
    }
    /// One member of `pos` by its wire name.
    fn pos_field(self, field: &str) -> Option<i64> {
        let pos = self.pos()?;
        let v = match field {
            "startLine" => pos.start_line,
            "endLine" => pos.end_line,
            "startColumn" => pos.start_column,
            "endColumn" => pos.end_column,
            "startOffset" => pos.start_offset,
            "endOffset" => pos.end_offset,
            _ => return None,
        };
        Some(i64::from(v))
    }
    /// A string member of an object field: `attrs.id`, `marker.kind`, `task.mark`.
    fn sub_str(self, name: &str, key: &str) -> Option<&'t str>;
    fn sub_int(self, name: &str, key: &str) -> Option<i64>;
    fn classes(self) -> Vec<&'t str>;
    fn key_values(self) -> Vec<(&'t str, &'t str)>;
}

impl<'t> View<'t> for &'t Value {
    type List = &'t [Value];

    fn empty() -> &'t [Value] {
        &[]
    }

    fn ty(self) -> &'t str {
        self.get("type").and_then(Value::as_str).unwrap_or("")
    }

    fn field(self, name: &str) -> Field<'t, Self> {
        let Some(v) = self.get(name) else {
            return Field::Absent;
        };
        match v {
            Value::Null => Field::Null,
            Value::String(s) => Field::Str(s),
            Value::Bool(b) => Field::Bool(*b),
            Value::Number(n) => n
                .as_i64()
                .map_or_else(|| n.as_f64().map_or(Field::Json(v), Field::Num), Field::Int),
            Value::Array(items) => Field::Nodes(items.as_slice()),
            Value::Object(_) if v.get("type").is_some() => Field::Node(v),
            Value::Object(_) => Field::Json(v),
        }
    }

    fn id(self) -> usize {
        std::ptr::from_ref(self).addr()
    }

    fn pos(self) -> Option<Pos> {
        let pos = self.get("pos")?;
        let at = |k: &str| {
            pos.get(k)
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
        };
        Some(Pos {
            start_line: at("startLine")?,
            end_line: at("endLine")?,
            start_column: at("startColumn")?,
            end_column: at("endColumn")?,
            start_offset: at("startOffset")?,
            end_offset: at("endOffset")?,
        })
    }

    fn span(self) -> Option<(usize, usize)> {
        let pos = self.get("pos")?;
        let start = usize::try_from(pos.get("startOffset")?.as_u64()?).ok()?;
        let end = usize::try_from(pos.get("endOffset")?.as_u64()?).ok()?;
        Some((start, end))
    }

    fn pos_field(self, field: &str) -> Option<i64> {
        self.get("pos")?.get(field)?.as_i64()
    }

    fn sub_str(self, name: &str, key: &str) -> Option<&'t str> {
        self.get(name)?.get(key)?.as_str()
    }

    fn sub_int(self, name: &str, key: &str) -> Option<i64> {
        self.get(name)?.get(key)?.as_i64()
    }

    fn classes(self) -> Vec<&'t str> {
        self.get("attrs")
            .and_then(|a| a.get("classes"))
            .and_then(Value::as_array)
            .map(|c| c.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    fn key_values(self) -> Vec<(&'t str, &'t str)> {
        self.get("attrs")
            .and_then(|a| a.get("keyValues"))
            .and_then(Value::as_array)
            .map(|kv| {
                kv.iter()
                    .filter_map(|p| {
                        let p = p.as_array()?;
                        Some((p.first()?.as_str()?, p.get(1)?.as_str()?))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A node of the owned typed tree; the root is a [`Document`] outside [`Node`].
#[derive(Debug, Clone, Copy)]
pub enum Typed<'t> {
    Doc(&'t Document),
    Node(&'t Node),
}

impl<'t> Typed<'t> {
    fn attrs(self) -> Option<&'t Attrs> {
        match self {
            Typed::Doc(d) => d.attrs.as_deref(),
            Typed::Node(n) => n.attrs(),
        }
    }
}

impl<'t> View<'t> for Typed<'t> {
    type List = &'t [Node];

    fn empty() -> &'t [Node] {
        &[]
    }

    fn ty(self) -> &'t str {
        match self {
            Typed::Doc(_) => "document",
            Typed::Node(n) => n.type_name(),
        }
    }

    fn field(self, name: &str) -> Field<'t, Self> {
        match self {
            Typed::Doc(d) => d.view_field(name),
            Typed::Node(n) => n.view_field(name),
        }
    }

    fn id(self) -> usize {
        match self {
            Typed::Doc(d) => std::ptr::from_ref(d).addr(),
            Typed::Node(n) => n.view_addr(),
        }
    }

    fn pos(self) -> Option<Pos> {
        match self {
            Typed::Doc(d) => d.pos,
            Typed::Node(n) => n.pos().copied(),
        }
    }

    fn sub_str(self, name: &str, key: &str) -> Option<&'t str> {
        match (self.field(name), key) {
            (Field::Attrs(a), "id") => a.id.as_deref(),
            (Field::Marker(m), "kind") => Some(&m.kind),
            (Field::Marker(m), "char") => m.bullet.as_deref(),
            (Field::Marker(m), "style") => m.style.as_deref(),
            (Field::Marker(m), "delimiter") => m.delimiter.as_deref(),
            (Field::Task(t), "mark") => Some(&t.mark),
            (Field::Json(v), _) => v.get(key)?.as_str(),
            _ => None,
        }
    }

    fn sub_int(self, name: &str, key: &str) -> Option<i64> {
        match (self.field(name), key) {
            (Field::Marker(m), "start") => m.start,
            (Field::Json(v), _) => v.get(key)?.as_i64(),
            _ => None,
        }
    }

    fn classes(self) -> Vec<&'t str> {
        self.attrs()
            .map(|a| a.classes.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    fn key_values(self) -> Vec<(&'t str, &'t str)> {
        self.attrs()
            .map(|a| {
                a.key_values
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Field, NodeList, Typed, View};
    use serde_json::Value;

    /// Every field the JSON value of a tree holds reads alike through the typed view.
    /// Every property name of the tree schema.
    fn names() -> &'static Vec<String> {
        static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        NAMES.get_or_init(|| {
            let schema: Value =
                serde_json::from_str(include_str!("../../spec-data/ast-schema.json"))
                    .unwrap_or_default();
            let mut out = std::collections::BTreeSet::new();
            for def in schema
                .get("$defs")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                for name in def
                    .1
                    .get("properties")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                {
                    out.insert(name.0.clone());
                }
            }
            out.into_iter().collect()
        })
    }

    fn same(v: &Value, t: Typed<'_>, at: &str) {
        assert_eq!(v.ty(), t.ty(), "{at}: type");
        assert!(v.is_object(), "{at}: not an object");
        assert_eq!(v.span(), t.span(), "{at}: span");
        assert_eq!(v.classes(), t.classes(), "{at}: classes");
        assert_eq!(v.key_values(), t.key_values(), "{at}: keyValues");
        let map = v.as_object().cloned().unwrap_or_default();
        for name in names() {
            if !map.contains_key(name) {
                assert!(matches!(t.field(name), Field::Absent), "{at}: {name} set");
            }
        }
        for (name, value) in &map {
            same_field(v, t, name, value, &format!("{at}/{name}"));
        }
    }

    fn same_members(v: &Value, t: Typed<'_>, name: &str, here: &str) {
        for key in ["kind", "char", "style", "delimiter", "mark"] {
            assert_eq!(v.sub_str(name, key), t.sub_str(name, key), "{here}/{key}");
        }
        assert_eq!(v.sub_int(name, "start"), t.sub_int(name, "start"), "{here}");
    }

    fn same_list(a: &[Value], b: &[crate::ast::owned::Node], here: &str) {
        assert_eq!(a.len(), NodeList::len(b), "{here}: length");
        for (i, x) in a.iter().enumerate() {
            if let Some(y) = NodeList::get(b, i) {
                same(x, y, &format!("{here}/{i}"));
            }
        }
    }

    fn same_field(v: &Value, t: Typed<'_>, name: &str, value: &Value, here: &str) {
        match (v.field(name), t.field(name)) {
            (Field::Str(a), Field::Str(b)) => assert_eq!(a, b, "{here}"),
            (Field::Int(a), Field::Int(b)) => assert_eq!(a, b, "{here}"),
            (Field::Bool(a), Field::Bool(b)) => assert_eq!(a, b, "{here}"),
            (Field::Num(a), Field::Num(b)) => assert!((a - b).abs() < f64::EPSILON, "{here}"),
            (Field::Node(a), Field::Node(b)) => same(a, b, here),
            (Field::Nodes(a), Field::Nodes(b)) => same_list(a, b, here),
            (Field::Nodes(a), Field::Strings(b)) => {
                let a: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
                assert_eq!(
                    a,
                    b.iter().map(String::as_str).collect::<Vec<_>>(),
                    "{here}"
                );
            }
            (Field::Json(_), Field::Pos(p)) => assert_eq!(v.pos(), Some(p), "{here}"),
            (Field::Nodes(_), Field::Segments(s)) => {
                let read: Vec<crate::ast::Pos> =
                    serde_json::from_value(value.clone()).unwrap_or_default();
                assert_eq!(read, s, "{here}");
            }
            (Field::Json(_), Field::Attrs(_)) => {
                assert_eq!(v.sub_str(name, "id"), t.sub_str(name, "id"), "{here}");
            }
            (Field::Json(_), Field::Marker(_) | Field::Task(_)) => same_members(v, t, name, here),
            (Field::Json(a), Field::Json(b)) => assert_eq!(a, b, "{here}"),
            (a, b) => assert_eq!(format!("{a:?}"), format!("{b:?}"), "{here}: kinds"),
        }
    }

    fn inputs() -> Vec<String> {
        let mut out = Vec::new();
        for (text, list) in [
            (
                include_str!("../../spec-conformance/examples.json"),
                "examples",
            ),
            (
                include_str!("../../spec-conformance/clause-cases.json"),
                "cases",
            ),
        ] {
            let mut de = serde_json::Deserializer::from_str(text);
            de.disable_recursion_limit();
            let data: Value = serde::Deserialize::deserialize(&mut de).unwrap_or_default();
            for case in data
                .get(list)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(input) = case.get("input").and_then(Value::as_str) {
                    out.push(input.to_string());
                }
            }
        }
        out
    }

    #[test]
    fn the_typed_view_reads_every_field_as_the_json_value() {
        let inputs = inputs();
        assert!(inputs.len() > 1000, "found {} inputs", inputs.len());
        let opts = crate::api::Options::default();
        for (i, input) in inputs.iter().enumerate() {
            let Ok(tree) = crate::api::parse(input.as_str(), &opts) else {
                continue;
            };
            let value = tree.value().unwrap_or_default();
            same(&value, Typed::Doc(tree.document()), &format!("input {i}"));
        }
    }
}
