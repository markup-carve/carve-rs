//! Generates the typed AST and the rule tables from the specification data in
//! `spec-data/` (a verbatim copy of `spec/0.2/data/` of the carve repository).
//! Nothing in `src/` copies a value from those files by hand.

#![allow(clippy::similar_names)]

use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("spec-data");
    println!("cargo:rerun-if-changed=spec-data");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let [arena, owned, conv, view] = gen_ast(
        &load(&dir, "ast-schema.json"),
        &load(&dir, "node-slots.json"),
    );
    write(&out.join("ast_gen.rs"), &arena);
    write(&out.join("ast_owned_gen.rs"), &owned);
    write(&out.join("ast_conv_gen.rs"), &conv);
    write(&out.join("ast_view_gen.rs"), &view);
    write(&out.join("tables_gen.rs"), &gen_tables(&dir));
    write(&out.join("package_info_gen.rs"), &gen_package_info(&dir));
}

/// `SPEC_COMMIT` and `NODE_TYPES` of the public library, from the pinned commit
/// and the children of `document` and `paragraph` in the tree schema.
fn gen_package_info(dir: &Path) -> String {
    let commit = fs::read_to_string(dir.join("SOURCE_COMMIT")).unwrap_or_default();
    let schema = load(dir, "ast-schema.json");
    let types = |node: &str| -> String {
        schema["$defs"][node]["properties"]["children"]["items"]["oneOf"]
            .as_array()
            .unwrap_or_else(|| panic!("ast-schema.json: no children of {node}"))
            .iter()
            .filter_map(|e| e["$ref"].as_str()?.rsplit('/').next())
            .map(|t| format!("{t:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "pub(crate) const SPEC_COMMIT: &str = {:?};\npub(crate) const BLOCK_TYPES: &[&str] = &[{}];\npub(crate) const INLINE_TYPES: &[&str] = &[{}];\n",
        commit.trim(),
        types("document"),
        types("paragraph"),
    )
}

fn load(dir: &Path, name: &str) -> Value {
    let text = fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if up {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            up = false;
        } else {
            up = true;
        }
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'V');
    }
    out
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else if c == '.' || c == '-' {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}

const COMMON: [&str; 8] = [
    "type", "attrs", "data", "pos", "segments", "metadata", "dir", "lang",
];

enum FieldKind {
    Nodes,
    NodeBox,
    Str,
    Int,
    Num,
    Bool,
    Enum(Vec<String>),
    Strings,
    Json,
    Marker,
    Task,
}

fn is_node_list(p: &Value) -> bool {
    let Some(items) = p.get("items") else {
        return false;
    };
    if let Some(r) = items.get("$ref").and_then(Value::as_str) {
        return r.starts_with("#/$defs/");
    }
    items
        .get("oneOf")
        .and_then(Value::as_array)
        .is_some_and(|a| {
            a.iter().all(|x| {
                x.get("$ref")
                    .and_then(Value::as_str)
                    .is_some_and(|r| r.starts_with("#/$defs/"))
            })
        })
}

fn field_kind(name: &str, p: &Value) -> FieldKind {
    if name == "marker" {
        return FieldKind::Marker;
    }
    if name == "task" {
        return FieldKind::Task;
    }
    if p.get("$ref")
        .and_then(Value::as_str)
        .is_some_and(|r| r.starts_with("#/$defs/"))
    {
        return FieldKind::NodeBox;
    }
    if let Some(values) = p.get("enum").and_then(Value::as_array) {
        return FieldKind::Enum(
            values
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
        );
    }
    if let Some(c) = p.get("const") {
        return if c.is_boolean() {
            FieldKind::Bool
        } else {
            FieldKind::Str
        };
    }
    match p.get("type").and_then(Value::as_str) {
        Some("array") if is_node_list(p) => FieldKind::Nodes,
        Some("array") => {
            let items = p.get("items").cloned().unwrap_or(Value::Null);
            let scalar = items.get("type").and_then(Value::as_str) == Some("string")
                || items.get("enum").is_some();
            if scalar {
                FieldKind::Strings
            } else {
                FieldKind::Json
            }
        }
        Some("string") => FieldKind::Str,
        Some("integer") => FieldKind::Int,
        Some("number") => FieldKind::Num,
        Some("boolean") => FieldKind::Bool,
        _ => FieldKind::Json,
    }
}

struct Field {
    json: String,
    rust: String,
    required: bool,
    /// One of the common fields only a deserialized tree carries (`RARE`).
    rare: bool,
    common: bool,
    kind: FieldKind,
    /// The Rust name of the field's enum type.
    enum_ty: Option<String>,
}

/// The two families of tree types: the arena tree the engine builds and the
/// owned tree for callers that keep or deserialize one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fam {
    Arena,
    Owned,
}

impl Fam {
    /// The path of a hand-written value type of the family.
    fn hand(self, name: &str) -> String {
        match self {
            Fam::Arena => format!("crate::ast::{name}<'a>"),
            Fam::Owned => format!("crate::ast::owned::{name}"),
        }
    }
}

fn rust_ident(name: &str) -> String {
    let s = snake(name);
    match s.as_str() {
        "type" | "box" | "ref" | "match" | "mod" | "use" | "where" => format!("r#{s}"),
        _ => s,
    }
}

struct NodeDef {
    name: String,
    ty: String,
    fields: Vec<Field>,
}

fn string_set(v: Option<&Value>) -> BTreeSet<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn node_defs(defs: &Map<String, Value>, enums: &mut String) -> Vec<NodeDef> {
    let mut out = Vec::new();
    for (name, def) in defs {
        let ty = camel(name);
        let props = def
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let required = string_set(def.get("required"));
        let fields = collect_fields(&ty, &props, &required, enums);
        out.push(NodeDef {
            name: name.clone(),
            ty,
            fields,
        });
    }
    out
}

/// The arena tree (`ast_gen.rs`), the owned tree (`ast_owned_gen.rs`), the
/// converters between them (`ast_conv_gen.rs`) and the lint view of the arena
/// tree (`ast_view_gen.rs`).
fn gen_ast(schema: &Value, slots: &Value) -> [String; 4] {
    let defs = schema
        .get("$defs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let head = "// @generated by build.rs from spec-data/ast-schema.json and node-slots.json.\n\n";
    let mut enums = String::new();
    let nodes = node_defs(&defs, &mut enums);
    let mut arena = String::from(head);
    let mut owned = String::from(head);
    let mut conv = String::from(head);
    for n in &nodes {
        write_struct(&mut arena, n, Fam::Arena);
        write_struct(&mut owned, n, Fam::Owned);
        write_conv(&mut conv, n);
    }
    arena.push_str(&enums);
    write_node_enum(&mut arena, &nodes, Fam::Arena);
    write_node_enum(&mut owned, &nodes, Fam::Owned);
    write_node_conv(&mut conv, &nodes);
    write_slots(&mut arena, &defs, slots, Fam::Arena);
    write_slots(&mut owned, &defs, slots, Fam::Owned);
    let mut view = String::from(head);
    for n in &nodes {
        write_view(&mut view, n);
    }
    write_node_view(&mut view, &nodes);
    [arena, owned, conv, view]
}

// ---------------------------------------------------------------- lint view

/// The view of a field `x` (a reference to the field's value without its `Option`), as serde writes it.
fn view_expr(f: &Field) -> String {
    match &f.kind {
        FieldKind::Nodes => "V::Nodes(x)".into(),
        FieldKind::NodeBox => "V::Node(x)".into(),
        FieldKind::Str => "V::Str(x)".into(),
        FieldKind::Strings => "V::Strs(x)".into(),
        FieldKind::Int => "V::Int(*x)".into(),
        FieldKind::Num => "V::num(*x)".into(),
        FieldKind::Bool => "V::Bool(*x)".into(),
        FieldKind::Enum(_) => "V::Str(x.as_str())".into(),
        FieldKind::Json => match f.json.as_str() {
            "attrs" => "V::Attrs(x)".into(),
            "pos" => "V::Pos(x)".into(),
            "segments" => "V::Segments(x)".into(),
            _ => "V::Json(x)".into(),
        },
        FieldKind::Marker => "V::Marker(x)".into(),
        FieldKind::Task => "V::Task(x)".into(),
    }
}

/// `view_get` and `view_each` of a node struct: its JSON keys, `type` included, in `serde_json` map order.
fn write_view(out: &mut String, n: &NodeDef) {
    let ty = &n.ty;
    let mut fields: Vec<&Field> = n.fields.iter().collect();
    fields.sort_by(|a, b| a.json.cmp(&b.json));
    let _ = writeln!(
        out,
        "impl<'a> {ty}<'a> {{\n    pub(crate) fn view_get(&'a self, key: &str) -> Option<V<'a>> {{\n        match key {{\n            \"type\" => Some(V::Str({:?})),",
        n.name
    );
    for f in &fields {
        let get = if f.required {
            format!("Some({{ let x = &self.{}; {} }})", f.rust, view_expr(f))
        } else {
            format!("self.{}.as_ref().map(|x| {})", f.rust, view_expr(f))
        };
        let _ = writeln!(out, "            {:?} => {get},", f.json);
    }
    let _ = writeln!(
        out,
        "            _ => None,\n        }}\n    }}\n\n    pub(crate) fn view_each(&'a self, f: &mut dyn FnMut(&'a str, V<'a>)) {{"
    );
    let mut keys: Vec<(&str, Option<&Field>)> =
        fields.iter().map(|f| (f.json.as_str(), Some(*f))).collect();
    keys.push(("type", None));
    keys.sort_by(|a, b| a.0.cmp(b.0));
    for (key, f) in keys {
        match f {
            None => {
                let _ = writeln!(out, "        f(\"type\", V::Str({:?}));", n.name);
            }
            Some(f) if f.required => {
                let _ = writeln!(
                    out,
                    "        {{ let x = &self.{}; f({key:?}, {}); }}",
                    f.rust,
                    view_expr(f)
                );
            }
            Some(f) => {
                let _ = writeln!(
                    out,
                    "        if let Some(x) = &self.{} {{ f({key:?}, {}); }}",
                    f.rust,
                    view_expr(f)
                );
            }
        }
    }
    let _ = writeln!(out, "    }}\n}}\n");
}

fn write_node_view(out: &mut String, nodes: &[NodeDef]) {
    out.push_str("impl<'a> Node<'a> {\n    pub(crate) fn view_get(&'a self, key: &str) -> Option<V<'a>> {\n        match self {\n");
    for n in nodes {
        let _ = writeln!(out, "            Self::{}(n) => n.view_get(key),", n.ty);
    }
    out.push_str("        }\n    }\n\n    pub(crate) fn view_each(&'a self, f: &mut dyn FnMut(&'a str, V<'a>)) {\n        match self {\n");
    for n in nodes {
        let _ = writeln!(out, "            Self::{}(n) => n.view_each(f),", n.ty);
    }
    out.push_str("        }\n    }\n}\n");
}

fn collect_fields(
    ty: &str,
    props: &Map<String, Value>,
    required: &BTreeSet<String>,
    enums: &mut String,
) -> Vec<Field> {
    let mut fields = Vec::new();
    for (pname, p) in props {
        if pname == "type" {
            continue;
        }
        let common = COMMON.contains(&pname.as_str());
        let kind = if common {
            common_kind(pname)
        } else {
            field_kind(pname, p)
        };
        let enum_ty = if let FieldKind::Enum(values) = &kind {
            let ename = format!("{ty}{}", camel(pname));
            write_enum(enums, &ename, values);
            Some(ename)
        } else {
            None
        };
        fields.push(Field {
            json: pname.clone(),
            rust: rust_ident(pname),
            required: required.contains(pname),
            rare: common && RARE.contains(&pname.as_str()),
            common,
            kind,
            enum_ty,
        });
    }
    fields
}

/// The common fields only a deserialized tree carries.
const RARE: [&str; 5] = ["attrs", "data", "dir", "metadata", "segments"];

fn common_kind(name: &str) -> FieldKind {
    match name {
        "dir" | "lang" => FieldKind::Str,
        _ => FieldKind::Json,
    }
}

fn common_type(name: &str, fam: Fam) -> String {
    match (name, fam) {
        ("attrs", Fam::Arena) => "&'a crate::ast::Attrs<'a>".into(),
        ("attrs", Fam::Owned) => "crate::ast::owned::Attrs".into(),
        ("pos", _) => "crate::ast::Pos".into(),
        ("segments", Fam::Arena) => "&'a [crate::ast::Pos]".into(),
        ("segments", Fam::Owned) => "Vec<crate::ast::Pos>".into(),
        (_, Fam::Arena) => "&'a serde_json::Value".into(),
        (_, Fam::Owned) => "serde_json::Value".into(),
    }
}

/// The Rust type of a field without its `Option`.
fn base_type(f: &Field, fam: Fam) -> String {
    let arena = fam == Fam::Arena;
    let base = match &f.kind {
        FieldKind::Nodes | FieldKind::NodeBox | FieldKind::Str | FieldKind::Strings => {
            plain_type(&f.kind, arena).to_string()
        }
        FieldKind::Int => "i64".to_string(),
        FieldKind::Num => "f64".to_string(),
        FieldKind::Bool => "bool".to_string(),
        FieldKind::Json if f.common => common_type(&f.json, fam),
        FieldKind::Json => common_type("", fam),
        FieldKind::Marker => fam.hand("Marker"),
        FieldKind::Task => fam.hand("Task"),
        FieldKind::Enum(_) => format!("crate::ast::{}", f.enum_ty.as_deref().unwrap_or("")),
    };
    // Owned: the fields the engine never sets are boxed, so each node is smaller.
    if f.rare && !arena {
        format!("Box<{base}>")
    } else {
        base
    }
}

/// The types that differ between the families only by borrowing.
fn plain_type(kind: &FieldKind, arena: bool) -> &'static str {
    let (a, o) = match kind {
        FieldKind::Nodes => ("&'a mut [Node<'a>]", "Vec<Node>"),
        FieldKind::NodeBox => ("&'a mut Node<'a>", "Box<Node>"),
        FieldKind::Strings => ("&'a [&'a str]", "Vec<String>"),
        _ => ("&'a str", "String"),
    };
    if arena { a } else { o }
}

fn field_type(f: &Field, fam: Fam) -> String {
    let base = base_type(f, fam);
    if f.required {
        base
    } else {
        format!("Option<{base}>")
    }
}

fn serde_attrs(f: &Field, fam: Fam) -> Vec<String> {
    let mut attrs = Vec::new();
    if f.json != f.rust.trim_start_matches("r#") {
        attrs.push(format!("rename = {:?}", f.json));
    }
    let owned = fam == Fam::Owned;
    if !f.required {
        if owned {
            attrs.push("default".into());
        }
        attrs.push("skip_serializing_if = \"Option::is_none\"".into());
    } else if owned && matches!(f.kind, FieldKind::Nodes) {
        attrs.push("default".into());
    }
    attrs
}

fn write_enum(out: &mut String, name: &str, values: &[String]) {
    let _ = writeln!(
        out,
        concat!(
            "#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]\n",
            "pub enum {name} {{"
        ),
        name = name,
    );
    for (i, v) in values.iter().enumerate() {
        let default = if i == 0 { "#[default]\n    " } else { "" };
        let _ = writeln!(
            out,
            "    {default}#[serde(rename = {v:?})]\n    {},",
            camel(v)
        );
    }
    let _ = writeln!(
        out,
        "}}\n\nimpl {name} {{\n    #[must_use]\n    pub const fn as_str(self) -> &'static str {{\n        match self {{"
    );
    for v in values {
        let _ = writeln!(out, "            Self::{} => {v:?},", camel(v));
    }
    let _ = writeln!(
        out,
        concat!(
            "        }}\n    }}\n\n    /// The value the schema spells `name`.\n    #[must_use]\n",
            "    pub fn from_name(name: &str) -> Option<Self> {{\n        match name {{",
        )
    );
    for v in values {
        let _ = writeln!(out, "            {v:?} => Some(Self::{}),", camel(v));
    }
    let _ = writeln!(out, "            _ => None,\n        }}\n    }}\n}}\n");
}

fn write_struct(out: &mut String, n: &NodeDef, fam: Fam) {
    let (name, ty) = (&n.name, &n.ty);
    let (derive, extra, generics) = match fam {
        Fam::Arena => ("Debug, Default, PartialEq, serde::Serialize", "", "<'a>"),
        Fam::Owned => (
            "Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize",
            "#[serde(deny_unknown_fields)]\n",
            "",
        ),
    };
    let _ = writeln!(
        out,
        "/// The `{name}` node.\n#[derive({derive})]\n{extra}pub struct {ty}{generics} {{"
    );
    for f in &n.fields {
        let attrs = serde_attrs(f, fam);
        if !attrs.is_empty() {
            let _ = writeln!(out, "    #[serde({})]", attrs.join(", "));
        }
        let _ = writeln!(out, "    pub {}: {},", f.rust, field_type(f, fam));
    }
    let _ = writeln!(out, "}}\n");
    if fam == Fam::Arena {
        // The bump allocator runs no destructors: a node type must own no heap memory.
        let _ = writeln!(
            out,
            "const _: () = assert!(!std::mem::needs_drop::<{ty}<'static>>());\n"
        );
    }
}

fn write_node_enum(out: &mut String, nodes: &[NodeDef], fam: Fam) {
    let (derive, node, head) = match fam {
        Fam::Arena => ("Debug, PartialEq, serde::Serialize", "Node<'a>", "Node<'_>"),
        Fam::Owned => (
            "Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize",
            "Node",
            "Node",
        ),
    };
    let _ = writeln!(
        out,
        "/// Every node type of the 2.0 contract (C2-A-050).\n#[derive({derive})]\n#[serde(tag = \"type\")]\npub enum {node} {{"
    );
    for n in nodes {
        let inner = match fam {
            Fam::Arena => format!("&'a mut {}<'a>", n.ty),
            Fam::Owned => format!("Box<{}>", n.ty),
        };
        let _ = writeln!(
            out,
            "    #[serde(rename = {:?})]\n    {}({inner}),",
            n.name, n.ty
        );
    }
    let _ = writeln!(
        out,
        concat!(
            "}}\n\nimpl {head} {{\n    /// The node type name as the schema spells it.\n    #[must_use]\n",
            "    pub const fn type_name(&self) -> &'static str {{\n        match self {{"
        ),
        head = head
    );
    for n in nodes {
        let _ = writeln!(out, "            Self::{}(_) => {:?},", n.ty, n.name);
    }
    out.push_str("        }\n    }\n}\n\n");
}

// ---------------------------------------------------------------- converters

fn copy_kind(f: &Field) -> bool {
    matches!(
        f.kind,
        FieldKind::Int | FieldKind::Num | FieldKind::Bool | FieldKind::Enum(_)
    ) || f.json == "pos"
}

/// The expression that copies a field of an owned node into the arena `a`.
fn to_arena_expr(f: &Field) -> String {
    let x = format!("self.{}", f.rust);
    if copy_kind(f) {
        return x;
    }
    // `r` is a reference to the value, `m` the value as a method receiver.
    let one = |r: &str, m: &str| -> String {
        match &f.kind {
            FieldKind::Nodes => format!("a.nodes_from({r})"),
            FieldKind::NodeBox => format!("a.alloc({m}.in_arena(a))"),
            FieldKind::Str => format!("a.str({r})"),
            FieldKind::Strings => format!("a.strs({r})"),
            FieldKind::Json if f.json == "attrs" => format!("a.attrs({r})"),
            FieldKind::Json if f.json == "segments" => format!("a.copy_slice({r})"),
            FieldKind::Json => format!("a.value({r})"),
            _ => format!("{m}.in_arena(a)"),
        }
    };
    if f.required {
        one(&format!("&{x}"), &x)
    } else if f.rare {
        format!("{x}.as_deref().map(|v| {})", one("v", "v"))
    } else {
        format!("{x}.as_ref().map(|v| {})", one("v", "v"))
    }
}

/// The expression that copies a field of an arena node into an owned one.
fn to_owned_expr(f: &Field) -> String {
    let x = format!("self.{}", f.rust);
    if copy_kind(f) {
        return x;
    }
    let one = |v: &str| -> String {
        match &f.kind {
            FieldKind::Nodes => format!("{v}.iter().map(Node::owned).collect()"),
            FieldKind::NodeBox => format!("Box::new({v}.owned())"),
            FieldKind::Str => format!("{v}.to_string()"),
            FieldKind::Strings => format!("{v}.iter().map(|s| (*s).to_string()).collect()"),
            FieldKind::Json if f.json == "segments" => format!("{v}.to_vec()"),
            FieldKind::Json if f.json != "attrs" => format!("{v}.clone()"),
            _ => format!("{v}.owned()"),
        }
    };
    let wrap = |e: String| if f.rare { format!("Box::new({e})") } else { e };
    if f.required {
        wrap(one(&x))
    } else {
        let get = if matches!(f.kind, FieldKind::Nodes | FieldKind::NodeBox) {
            ".as_ref()"
        } else {
            ""
        };
        let e = wrap(one("v"));
        match e.as_str() {
            "v.to_string()" => format!("{x}.map(str::to_string)"),
            "v.clone()" => format!("{x}.cloned()"),
            _ => format!("{x}{get}.map(|v| {e})"),
        }
    }
}

fn write_conv(out: &mut String, n: &NodeDef) {
    let ty = &n.ty;
    let _ = writeln!(
        out,
        concat!(
            "impl owned::{ty} {{\n    /// The node copied into an arena.\n    #[must_use]\n",
            "    pub fn in_arena<'a>(&self, a: &'a crate::Arena) -> {ty}<'a> {{\n        {ty} {{"
        ),
        ty = ty
    );
    for f in &n.fields {
        let _ = writeln!(out, "            {}: {},", f.rust, to_arena_expr(f));
    }
    let _ = writeln!(
        out,
        concat!(
            "        }}\n    }}\n}}\n\nimpl {ty}<'_> {{\n    /// The node as an owned value.\n    #[must_use]\n",
            "    pub fn owned(&self) -> owned::{ty} {{\n        owned::{ty} {{"
        ),
        ty = ty
    );
    for f in &n.fields {
        let _ = writeln!(out, "            {}: {},", f.rust, to_owned_expr(f));
    }
    let _ = writeln!(out, "        }}\n    }}\n}}\n");
}

fn write_node_conv(out: &mut String, nodes: &[NodeDef]) {
    out.push_str(concat!(
        "impl owned::Node {\n    /// The tree copied into an arena.\n    #[must_use]\n",
        "    pub fn in_arena<'a>(&self, a: &'a crate::Arena) -> Node<'a> {\n        match self {\n"
    ));
    for n in nodes {
        let _ = writeln!(
            out,
            "            owned::Node::{0}(n) => Node::{0}(a.alloc(n.in_arena(a))),",
            n.ty
        );
    }
    out.push_str(concat!(
        "        }\n    }\n}\n\nimpl Node<'_> {\n    /// The tree as an owned value.\n    #[must_use]\n",
        "    pub fn owned(&self) -> owned::Node {\n        match self {\n"
    ));
    for n in nodes {
        let _ = writeln!(
            out,
            "            Node::{0}(n) => owned::Node::{0}(Box::new(n.owned())),",
            n.ty
        );
    }
    out.push_str("        }\n    }\n}\n");
}

// ---------------------------------------------------------------- slots

/// The walk over the node-bearing slots of one node type, shared and mutable.
fn walk_bodies(
    order: &[String],
    props: &Map<String, Value>,
    required: &BTreeSet<String>,
    fam: Fam,
) -> (String, String) {
    let arena = fam == Fam::Arena;
    let (r, m) = if arena {
        ("&*", "&mut *")
    } else {
        ("&", "&mut ")
    };
    let (cs, cs_mut) = if arena {
        ("&**cs", "&mut **cs")
    } else {
        ("cs", "cs")
    };
    let mut body = String::new();
    let mut body_mut = String::new();
    for slot in order {
        let Some(p) = props.get(slot) else { continue };
        let field = rust_ident(slot);
        let req = required.contains(slot);
        match field_kind(slot, p) {
            FieldKind::Nodes if req => {
                let _ = writeln!(body, "                for c in {r}n.{field} {{ f(c); }}");
                let _ = writeln!(
                    body_mut,
                    "                for c in {m}n.{field} {{ f(c); }}"
                );
            }
            FieldKind::Nodes => {
                let _ = writeln!(
                    body,
                    "                if let Some(cs) = &n.{field} {{ for c in {cs} {{ f(c); }} }}"
                );
                let _ = writeln!(
                    body_mut,
                    "                if let Some(cs) = &mut n.{field} {{ for c in {cs_mut} {{ f(c); }} }}"
                );
            }
            FieldKind::NodeBox => {
                let _ = writeln!(
                    body,
                    "                if let Some(c) = &n.{field} {{ f(c); }}"
                );
                let _ = writeln!(
                    body_mut,
                    "                if let Some(c) = &mut n.{field} {{ f(c); }}"
                );
            }
            _ => {}
        }
    }
    (body, body_mut)
}

/// The match arms gathered for the accessors of `Node`.
#[derive(Default)]
struct Arms {
    pos: String,
    pos_mut: String,
    attrs: String,
    attrs_mut: String,
    dir: String,
    children: String,
    children_mut: String,
    children_slot: String,
    walk: String,
    walk_mut: String,
}

/// The signatures of the accessors that differ between the families.
struct Sigs {
    head: &'static str,
    attrs: &'static str,
    attrs_slot: &'static str,
    children: &'static str,
    children_mut: &'static str,
    node: &'static str,
}

fn sigs(fam: Fam) -> Sigs {
    match fam {
        Fam::Arena => Sigs {
            head: "impl<'a> Node<'a>",
            attrs: "Option<&'a crate::ast::Attrs<'a>>",
            attrs_slot: "Option<&mut Option<&'a crate::ast::Attrs<'a>>>",
            children: "Option<&[Node<'a>]>",
            children_mut: "Option<&mut [Node<'a>]>",
            node: "Node<'a>",
        },
        Fam::Owned => Sigs {
            head: "impl Node",
            attrs: "Option<&crate::ast::owned::Attrs>",
            attrs_slot: "Option<&mut Option<Box<crate::ast::owned::Attrs>>>",
            children: "Option<&Vec<Node>>",
            children_mut: "Option<&mut Vec<Node>>",
            node: "Node",
        },
    }
}

fn emit_slots(out: &mut String, a: &Arms, fam: Fam) {
    let s = sigs(fam);
    let (head, node) = (s.head, s.node);
    let _ = write!(
        out,
        "#[allow(unreachable_patterns)]
{head} {{
    /// The source position, when the node carries one.
    #[must_use]
    pub fn pos(&self) -> Option<&crate::ast::Pos> {{
        match self {{
{}            _ => None,
        }}
    }}

    /// The position slot of the node type, when it has one.
    pub fn pos_slot(&mut self) -> Option<&mut Option<crate::ast::Pos>> {{
        match self {{
{}            _ => None,
        }}
    }}

    /// The authored attributes, when the node carries them.
    #[must_use]
    pub fn attrs(&self) -> {} {{
        match self {{
{}            _ => None,
        }}
    }}

    /// The direction field of the node, when present.
    #[must_use]
    pub fn dir(&self) -> Option<&str> {{
        match self {{
{}            _ => None,
        }}
    }}

    /// The attribute slot of the node type, when it has one.
    pub fn attrs_slot(&mut self) -> {} {{
        match self {{
{}            _ => None,
        }}
    }}

    /// The primary `children` slot (C2-A-022).
    #[must_use]
    pub fn children(&self) -> {} {{
        match self {{
{}            _ => None,
        }}
    }}

    /// The primary `children` slot, mutable.
    pub fn children_mut(&mut self) -> {} {{
        match self {{
{}            _ => None,
        }}
    }}

    /// Visits every node held in a node-bearing slot, in node-slots.json order.
    // generated from the schema
    #[allow(clippy::cognitive_complexity)]
    pub fn for_each_child(&self, f: &mut dyn FnMut(&{node})) {{
        match self {{
{}            _ => {{}}
        }}
    }}

    /// Visits every node held in a node-bearing slot, mutable.
    // generated from the schema
    #[allow(clippy::cognitive_complexity)]
    pub fn for_each_child_mut(&mut self, f: &mut dyn FnMut(&mut {node})) {{
        match self {{
{}            _ => {{}}
        }}
    }}
",
        a.pos,
        a.pos_mut,
        s.attrs,
        a.attrs,
        a.dir,
        s.attrs_slot,
        a.attrs_mut,
        s.children,
        a.children,
        s.children_mut,
        a.children_mut,
        a.walk,
        a.walk_mut,
    );
    if fam == Fam::Arena {
        let _ = write!(
            out,
            "
    /// Sets the primary `children` slot; false when the type has none.
    pub fn set_children(&mut self, kids: &'a mut [Node<'a>]) -> bool {{
        match self {{
{}            _ => return false,
        }}
        true
    }}
",
            a.children_slot
        );
    }
    out.push_str("}\n");
}

fn slot_arms(a: &mut Arms, ty: &str, def: &Value, fam: Fam) {
    let arena = fam == Fam::Arena;
    let props = def
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let required = def
        .get("required")
        .and_then(Value::as_array)
        .is_some_and(|r| r.iter().any(|x| x == "children"));
    if props.contains_key("pos") {
        let _ = writeln!(a.pos, "            Self::{ty}(n) => n.pos.as_ref(),");
        let _ = writeln!(a.pos_mut, "            Self::{ty}(n) => Some(&mut n.pos),");
    }
    if props.contains_key("attrs") {
        let get = if arena {
            "n.attrs"
        } else {
            "n.attrs.as_deref()"
        };
        let _ = writeln!(a.attrs, "            Self::{ty}(n) => {get},");
        let _ = writeln!(
            a.attrs_mut,
            "            Self::{ty}(n) => Some(&mut n.attrs),"
        );
    }
    if props.contains_key("dir") {
        let get = if arena {
            "n.dir"
        } else {
            "n.dir.as_deref().map(String::as_str)"
        };
        let _ = writeln!(a.dir, "            Self::{ty}(n) => {get},");
    }
    if props.get("children").is_some_and(is_node_list) {
        // C2-N-017: an include has `children` only once it is expanded.
        let (get, get_mut, set) = match (arena, required) {
            (true, true) => ("Some(&*n.children)", "Some(&mut *n.children)", "kids"),
            (false, true) => ("Some(&n.children)", "Some(&mut n.children)", "kids"),
            (true, false) => (
                "n.children.as_deref()",
                "n.children.as_deref_mut()",
                "Some(kids)",
            ),
            (false, false) => ("n.children.as_ref()", "n.children.as_mut()", "Some(kids)"),
        };
        let _ = writeln!(a.children, "            Self::{ty}(n) => {get},");
        let _ = writeln!(a.children_mut, "            Self::{ty}(n) => {get_mut},");
        let _ = writeln!(
            a.children_slot,
            "            Self::{ty}(n) => n.children = {set},"
        );
    }
}

/// Accessors for the common fields and the walk over node-bearing slots, in the
/// order of node-slots.json (C2-A-025).
fn write_slots(out: &mut String, defs: &Map<String, Value>, slots: &Value, fam: Fam) {
    let slot_map = slots
        .get("nodes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut a = Arms::default();
    for (name, def) in defs {
        let ty = camel(name);
        let props = def
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        slot_arms(&mut a, &ty, def, fam);
        let order: Vec<String> = slot_map
            .get(name)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let required = string_set(def.get("required"));
        let (body, body_mut) = walk_bodies(&order, &props, &required, fam);
        if !body.is_empty() {
            let _ = writeln!(
                a.walk,
                "            Self::{ty}(n) => {{\n{body}            }}"
            );
            let _ = writeln!(
                a.walk_mut,
                "            Self::{ty}(n) => {{\n{body_mut}            }}"
            );
        }
    }
    emit_slots(out, &a, fam);
}

// ---------------------------------------------------------------- tables

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn code_point(text: &str) -> char {
    let hex = text.trim_start_matches("U+");
    u32::from_str_radix(hex, 16)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or('\u{FFFD}')
}

fn gen_tables(dir: &Path) -> String {
    let mut out = String::from("// @generated by build.rs from spec-data/*.json.\n\n");
    gen_unicode(&mut out, &load(dir, "unicode-categories.json"));
    gen_limits(&mut out, &load(dir, "limits.json"));
    gen_tasks(&mut out, &load(dir, "task-states.json"));
    gen_divs(&mut out, &load(dir, "div-names.json"));
    gen_smart(&mut out, &load(dir, "smart-punctuation.json"));
    gen_slug(&mut out, &load(dir, "heading-slug.json"));
    gen_url(&mut out, &load(dir, "url-policy.json"));
    gen_options(&mut out, &load(dir, "options.json"));
    gen_delims(
        &mut out,
        &load(dir, "inline-delimiters.json"),
        &load(dir, "inline-braces.json"),
    );
    gen_includes(&mut out, &load(dir, "include-options.json"));
    gen_include_rules(
        &mut out,
        &load(dir, "includes.json"),
        &load(dir, "lint-rules.json"),
    );
    gen_dispatch(&mut out, &load(dir, "line-shapes.json"));
    gen_ordered(&mut out, &load(dir, "ordered-styles.json"));
    out
}

fn gen_unicode(out: &mut String, v: &Value) {
    for category in arr(v, "categories") {
        let name = s(category, "name").to_ascii_uppercase();
        let _ = writeln!(out, "pub const UNICODE_{name}: &[(u32, u32)] = &[");
        for range in arr(category, "ranges") {
            let Some(range) = range.as_str() else {
                continue;
            };
            let (start, end) = range.split_once('-').unwrap_or((range, range));
            let start = u32::from_str_radix(start, 16).unwrap_or(0);
            let end = u32::from_str_radix(end, 16).unwrap_or(0);
            let _ = writeln!(out, "    (0x{start:x}, 0x{end:x}),");
        }
        out.push_str("];\n\n");
    }
}

fn gen_ordered(out: &mut String, v: &Value) {
    out.push_str("/// (ordered list style, HTML `type` value)\npub const ORDERED_STYLES: &[(&str, Option<&str>)] = &[\n");
    for row in arr(v, "styles") {
        let t = row.get("html_type").and_then(Value::as_str);
        let _ = writeln!(out, "    ({:?}, {t:?}),", s(row, "style"));
    }
    out.push_str("];\n\n");
}

fn gen_limits(out: &mut String, v: &Value) {
    for row in arr(v, "limits") {
        let name = s(row, "name").to_ascii_uppercase();
        let value = row.get("value").and_then(Value::as_u64).unwrap_or(0);
        let _ = writeln!(out, "pub const {name}: usize = {value};");
    }
    out.push('\n');
}

/// C2-N-024, C2-N-061: the failure causes in test order, the conflict rows,
/// and every catalog message.
fn gen_include_rules(out: &mut String, includes: &Value, rules: &Value) {
    out.push_str("/// (cause, code) in the order they are tested\npub const INCLUDE_CAUSES: &[(&str, &str)] = &[\n");
    for row in arr(includes, "causes") {
        let _ = writeln!(out, "    ({:?}, {:?}),", s(row, "name"), s(row, "code"));
    }
    out.push_str(
        "];\n\n/// (rule, options)\npub const INCLUDE_CONFLICTS: &[(&str, &[&str])] = &[\n",
    );
    for row in arr(includes, "conflicts") {
        let names: Vec<String> = arr(row, "options")
            .iter()
            .filter_map(Value::as_str)
            .map(|n| format!("{n:?}"))
            .collect();
        let _ = writeln!(out, "    ({:?}, &[{}]),", s(row, "rule"), names.join(", "));
    }
    out.push_str("];\n\n/// (code, channel, severity, messages)\npub const MESSAGES: &[(&str, &str, &str, &[&str])] = &[\n");
    for row in arr(rules, "rules") {
        let channel = row.get("channel").and_then(Value::as_str).unwrap_or("lint");
        let messages: Vec<String> = arr(row, "messages")
            .iter()
            .filter_map(|message| {
                message
                    .as_str()
                    .or_else(|| message.get("text").and_then(Value::as_str))
            })
            .map(|m| format!("{m:?}"))
            .collect();
        let _ = writeln!(
            out,
            "    ({:?}, {channel:?}, {:?}, &[{}]),",
            s(row, "id"),
            s(row, "severity"),
            messages.join(", ")
        );
    }
    out.push_str("];\n\n");
}

fn gen_tasks(out: &mut String, v: &Value) {
    out.push_str(concat!(
        "/// (box character, tree mark, `data-task-state` value, checked)\npub const TASK_STATES: ",
        "&[(char, char, Option<char>, bool)] = &[\n",
    ));
    for row in arr(v, "states") {
        let c = s(row, "char").chars().next().unwrap_or(' ');
        let m = s(row, "mark").chars().next().unwrap_or(' ');
        let attr = row
            .get("state_attribute")
            .and_then(Value::as_str)
            .and_then(|a| a.chars().next());
        let checked = row.get("checked").and_then(Value::as_bool).unwrap_or(false);
        let _ = writeln!(out, "    ({c:?}, {m:?}, {attr:?}, {checked}),");
    }
    out.push_str("];\n\n");
}

fn gen_divs(out: &mut String, v: &Value) {
    out.push_str("/// (div name, kind: `admonition`, `directive` or `figure_group`)\npub const DIV_NAMES: &[(&str, &str)] = &[\n");
    for row in arr(v, "names") {
        let _ = writeln!(out, "    ({:?}, {:?}),", s(row, "name"), s(row, "kind"));
    }
    out.push_str("];\n\n/// (sigil, container name, class)\npub const DIV_SIGILS: &[(char, &str, &str)] = &[\n");
    for row in arr(v, "sigils") {
        let c = s(row, "sigil").chars().next().unwrap_or(' ');
        let _ = writeln!(
            out,
            "    ({c:?}, {:?}, {:?}),",
            s(row, "name"),
            s(row, "class")
        );
    }
    out.push_str("];\n\n");
}

fn gen_smart(out: &mut String, v: &Value) {
    out.push_str("/// (source run, replaced, kind, glyph)\npub const SMART_RUNS: &[(&str, bool, &str, &str)] = &[\n");
    for row in arr(v, "runs") {
        let replaced = row
            .get("replaced")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let _ = writeln!(
            out,
            "    ({:?}, {replaced}, {:?}, {:?}),",
            s(row, "source"),
            s(row, "kind"),
            s(row, "glyph")
        );
    }
    out.push_str(
        "];\n\n/// (kind, source, glyph)\npub const SMART_QUOTES: &[(&str, &str, &str)] = &[\n",
    );
    for row in arr(v, "quotes") {
        let _ = writeln!(
            out,
            "    ({:?}, {:?}, {:?}),",
            s(row, "kind"),
            s(row, "source"),
            s(row, "glyph")
        );
    }
    out.push_str(
        "];\n\n/// (kind, source, glyph)\npub const SMART_DASHES: &[(&str, &str, &str)] = &[\n",
    );
    for row in arr(v, "dashes") {
        let _ = writeln!(
            out,
            "    ({:?}, {:?}, {:?}),",
            s(row, "kind"),
            s(row, "source"),
            s(row, "glyph")
        );
    }
    out.push_str("];\n\n");
    let opens = v.get("quote_opens_after").cloned().unwrap_or(Value::Null);
    let chars: String = arr(&opens, "chars")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let _ = writeln!(out, "pub const QUOTE_OPENS_AFTER_CHARS: &str = {chars:?};");
    let closes = v
        .get("quote_closes_after_hyphen_dash_or_slash_before")
        .cloned()
        .unwrap_or(Value::Null);
    let chars: String = arr(&closes, "chars")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let _ = writeln!(
        out,
        "pub const QUOTE_CLOSES_AFTER_DASH_BEFORE_CHARS: &str = {chars:?};"
    );
    let words = v.get("apostrophe_words").cloned().unwrap_or(Value::Null);
    let list: Vec<String> = arr(&words, "words")
        .iter()
        .filter_map(Value::as_str)
        .map(|w| format!("{w:?}"))
        .collect();
    let _ = writeln!(
        out,
        "pub const APOSTROPHE_WORDS: &[&str] = &[{}];\n",
        list.join(", ")
    );
}

fn gen_slug(out: &mut String, v: &Value) {
    let removed: Vec<String> = arr(v, "removed")
        .iter()
        .map(|r| format!("{:?}", code_point(s(r, "code_point"))))
        .collect();
    let _ = writeln!(
        out,
        "pub const SLUG_REMOVED: &[char] = &[{}];",
        removed.join(", ")
    );
    out.push_str("pub const SLUG_REVERSED: &[(char, &str)] = &[\n");
    for row in arr(v, "reversed") {
        let g = s(row, "glyph").chars().next().unwrap_or(' ');
        let _ = writeln!(out, "    ({g:?}, {:?}),", s(row, "ascii"));
    }
    out.push_str("];\n\n");
    out.push_str("pub const SLUG_FOLDS: &[(char, &str)] = &[\n");
    for row in arr(&v["transliteration"], "folds") {
        let letter = s(row, "letter").chars().next().unwrap_or(' ');
        let _ = writeln!(out, "    ({letter:?}, {:?}),", s(row, "ascii"));
    }
    out.push_str("];\n\n");
}

fn gen_url(out: &mut String, v: &Value) {
    out.push_str("/// Inclusive code point ranges the scheme probe removes (C2-H-159).\npub const PROBE_REMOVES: &[(char, char)] = &[\n");
    for row in arr(v, "probe_removes") {
        let _ = writeln!(
            out,
            "    ({:?}, {:?}),",
            code_point(s(row, "from")),
            code_point(s(row, "to"))
        );
    }
    out.push_str("];\n\n/// (name, `name` or `prefix`)\npub const DROPPED_ATTRIBUTES: &[(&str, &str)] = &[\n");
    for row in arr(v, "dropped_attributes") {
        let _ = writeln!(out, "    ({:?}, {:?}),", s(row, "name"), s(row, "match"));
    }
    out.push_str("];\n\n/// (attribute, splits at commas too)\npub const URL_LIST_ATTRIBUTES: &[(&str, bool)] = &[\n");
    for row in arr(v, "url_list_attributes") {
        let comma = arr(row, "separators")
            .iter()
            .any(|x| x.as_str() == Some("comma"));
        let _ = writeln!(out, "    ({:?}, {comma}),", s(row, "name"));
    }
    out.push_str("];\n\n");
    let pats: Vec<String> = arr(v, "style_patterns")
        .iter()
        .map(|r| format!("{:?}", s(r, "text")))
        .collect();
    let _ = writeln!(
        out,
        "pub const STYLE_PATTERNS: &[&str] = &[{}];\n",
        pats.join(", ")
    );
    out.push_str(concat!(
        "pub struct ProfileRow {\n    pub name: &'static str,\n    pub denied_block: &'static ",
        "[&'static str],\n    pub denied_inline: &'static [&'static str],\n    pub allowed_block: ",
        "Option<&'static [&'static str]>,\n    pub allowed_inline: Option<&'static [&'static ",
        "str]>,\n    pub link_rel: &'static [&'static str],\n    pub max_nesting: Option<usize>,",
        "\n    pub max_length: Option<usize>,\n}\n\npub const PROFILES: &[ProfileRow] = &[\n",
    ));
    for row in arr(v, "profiles") {
        let list = |k: &str| {
            arr(row, k)
                .iter()
                .filter_map(Value::as_str)
                .map(|x| format!("{x:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let opt = |k: &str| {
            if row.get(k).is_some() {
                format!("Some(&[{}])", list(k))
            } else {
                "None".into()
            }
        };
        let num = |k: &str| {
            row.get(k)
                .and_then(Value::as_u64)
                .map_or("None".into(), |n| format!("Some({n})"))
        };
        let _ = writeln!(
            out,
            concat!(
                "    ProfileRow {{ name: {:?}, denied_block: &[{}], denied_inline: &[{}], allowed_block: ",
                "{}, allowed_inline: {}, link_rel: &[{}], max_nesting: {}, max_length: {} }},",
            ),
            s(row, "name"),
            list("denied_block"),
            list("denied_inline"),
            opt("allowed_block"),
            opt("allowed_inline"),
            list("link_rel"),
            num("max_nesting"),
            num("max_length")
        );
    }
    out.push_str("];\n\n");
}

fn gen_options(out: &mut String, v: &Value) {
    for row in arr(v, "options") {
        let name = s(row, "name");
        let default = row.get("default").cloned().unwrap_or(Value::Null);
        match name {
            "labels" => {
                out.push_str("pub const DEFAULT_LABELS: &[(&str, &str)] = &[\n");
                for (k, val) in default.as_object().into_iter().flatten() {
                    let _ = writeln!(out, "    ({k:?}, {:?}),", val.as_str().unwrap_or(""));
                }
                out.push_str("];\n");
            }
            "urlPolicy" => {
                let deny: Vec<String> = arr(&default, "deny")
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|x| format!("{x:?}"))
                    .collect();
                let _ = writeln!(
                    out,
                    "pub const DEFAULT_DENIED_SCHEMES: &[&str] = &[{}];",
                    deny.join(", ")
                );
            }
            _ => {
                if let Some(b) = default.as_bool() {
                    let _ = writeln!(
                        out,
                        "pub const DEFAULT_{}: bool = {b};",
                        snake(name).to_ascii_uppercase()
                    );
                } else if let Some(t) = default.as_str() {
                    let _ = writeln!(
                        out,
                        "pub const DEFAULT_{}: &str = {t:?};",
                        snake(name).to_ascii_uppercase()
                    );
                }
            }
        }
    }
    out.push('\n');
}

fn gen_delims(out: &mut String, v: &Value, braces: &Value) {
    out.push_str("/// (mark, node type, form `bare` or `braced`)\npub const DELIMITERS: &[(char, &str, &str)] = &[\n");
    for row in arr(v, "delimiters") {
        let c = s(row, "mark").chars().next().unwrap_or(' ');
        let _ = writeln!(
            out,
            "    ({c:?}, {:?}, {:?}),",
            s(row, "node"),
            s(row, "form")
        );
    }
    out.push_str("];\n");
    let marks: String = arr(braces, "order")
        .iter()
        .filter(|r| s(r, "name") == "braced_opener")
        .flat_map(|r| arr(r, "marks").iter().filter_map(Value::as_str))
        .collect();
    let _ = writeln!(out, "pub const BRACED_MARKS: &str = {marks:?};\n");
}

fn gen_includes(out: &mut String, v: &Value) {
    out.push_str(concat!(
        "/// (rank, key, value shape, needs value)\npub const ",
        "INCLUDE_OPTIONS: &[(u64, &str, &str, bool)] = &[\n",
    ));
    for row in arr(v, "options") {
        let rank = row.get("rank").and_then(Value::as_u64).unwrap_or(0);
        let _ = writeln!(
            out,
            "    ({rank}, {:?}, {:?}, {}),",
            s(row, "name"),
            s(row, "value"),
            row.get("needs_value")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        );
    }
    out.push_str("];\n\n");
}

/// The dispatch of the block pass: for each ASCII character, the line shapes
/// that may start with it (`name` and `first` of line-shapes.json).
fn gen_dispatch(out: &mut String, v: &Value) {
    let mut table: Vec<Vec<String>> = vec![Vec::new(); 128];
    for shape in arr(v, "shapes") {
        let name = s(shape, "name");
        for first in arr(shape, "first").iter().filter_map(Value::as_str) {
            let b = first.as_bytes();
            let (lo, hi) = if b.len() == 3 && b[1] == b'-' {
                (b[0], b[2])
            } else {
                (b[0], b[0])
            };
            for c in lo..=hi {
                table[usize::from(c)].push(name.to_string());
            }
        }
    }
    out.push_str("/// Line shapes by first character (line-shapes.json `first`).\npub const SHAPES_BY_FIRST: [&[&str]; 128] = [\n");
    for row in &table {
        let names: Vec<String> = row.iter().map(|n| format!("{n:?}")).collect();
        let _ = writeln!(out, "    &[{}],", names.join(", "));
    }
    out.push_str("];\n");
}
