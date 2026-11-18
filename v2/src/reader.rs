//! The 2.0 tree gate (C2-A-188): schema checks followed by reader checks.

use crate::ast::owned;
use serde::Deserialize as _;
use serde_json::Value;

fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).unwrap_or(&Value::Null)
}
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

mod schema;

/// A refused supplied tree, with its JSON path and the gate that refused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    /// The bytes are not a JSON value.
    Json(String),
    /// The tree fails `data/ast-schema.json`.
    Schema { path: String, reason: String },
    /// The tree passes the schema but fails a reader check.
    Reader { path: String, reason: String },
    /// The validated JSON cannot be copied into owned AST types.
    Decode { path: String, reason: String },
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(reason) => write!(f, "invalid JSON: {reason}"),
            Self::Schema { path, reason } => write!(f, "schema at {path}: {reason}"),
            Self::Reader { path, reason } => write!(f, "reader at {path}: {reason}"),
            Self::Decode { path, reason } => write!(f, "decode at {path}: {reason}"),
        }
    }
}

impl std::error::Error for TreeError {}

/// A validated tree with its owned AST and its wire fields, including `pos.file`.
#[derive(Debug, Clone)]
pub struct ReadTree {
    document: owned::Document,
    wire: Value,
}

impl ReadTree {
    /// The owned AST of the supplied tree.
    #[must_use]
    pub fn document(&self) -> &owned::Document {
        &self.document
    }

    /// Takes the owned AST when the caller no longer needs the original wire form.
    #[must_use]
    pub fn into_document(self) -> owned::Document {
        self.document
    }
}

fn reader(path: &str, reason: &str) -> TreeError {
    TreeError::Reader {
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

fn schema(path: &str, reason: &str) -> TreeError {
    TreeError::Schema {
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

fn definitions() -> Result<&'static Value, TreeError> {
    static DEFINITIONS: OnceLock<Result<Value, String>> = OnceLock::new();
    let parsed = DEFINITIONS.get_or_init(|| {
        serde_json::from_str(include_str!("../spec-data/ast-schema.json"))
            .map_err(|e| e.to_string())
    });
    parsed.as_ref().map_err(|e| schema("", e))
}

fn check_schema(root: &Value) -> Result<(), TreeError> {
    if root["type"] != "document" {
        return Err(schema("/type", "root is not a document"));
    }
    if root.get("children").and_then(Value::as_array).is_none() {
        return Err(schema("/children", "document children are required"));
    }
    let version = root["astVersion"].as_str().unwrap_or("");
    let Some((major, minor)) = version.split_once('.') else {
        return Err(schema("/astVersion", "invalid AST version"));
    };
    if major != "2"
        || minor.is_empty()
        || minor.starts_with('0') && minor != "0"
        || !minor.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(schema("/astVersion", "invalid AST version"));
    }
    let defs = definitions()?;
    schema::validate(root, defs)?;
    let mut pending = vec![(root, String::new())];
    while let Some((value, path)) = pending.pop() {
        match value {
            Value::Array(items) => {
                pending.extend(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (v, format!("{path}/{i}"))),
                );
            }
            Value::Object(map) => {
                if value.get("type").is_some() {
                    schema_node(value, &path, defs)?;
                }
                schema_fields(value, &path)?;
                pending.extend(
                    map.iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "data" | "props"))
                        .map(|(k, v)| (v, format!("{path}/{k}"))),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn schema_node(value: &Value, path: &str, defs: &Value) -> Result<(), TreeError> {
    let kind = value["type"].as_str().unwrap_or("");
    let Some(def) = defs["$defs"].get(kind) else {
        return Err(schema(&format!("{path}/type"), "unknown core node type"));
    };
    for required in def["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if value.get(required).is_none() {
            return Err(schema(
                &format!("{path}/{required}"),
                "required node field is missing",
            ));
        }
    }
    for key in value.as_object().into_iter().flat_map(|m| m.keys()) {
        if def["properties"].get(key).is_none() {
            return Err(schema(&format!("{path}/{key}"), "unknown core node field"));
        }
    }
    match kind {
        "heading" if value["level"].as_i64().is_some_and(|n| n < 1) => Err(schema(
            &format!("{path}/level"),
            "heading level is below one",
        )),
        "paragraph" => schema_paragraph(value, path),
        "math" => schema_math(value, path),
        "include" => schema_include(value, path),
        "inline_extension" | "block_extension" => schema_extension(value, path),
        _ => Ok(()),
    }
}

fn schema_fields(value: &Value, path: &str) -> Result<(), TreeError> {
    if let Some(pos) = value.get("pos") {
        for key in ["startLine", "endLine", "startColumn", "endColumn"] {
            if pos[key].as_u64().is_none_or(|n| n < 1) {
                return Err(schema(
                    &format!("{path}/pos/{key}"),
                    "position is below one",
                ));
            }
        }
    }
    if let Some(attrs) = value.get("attrs") {
        schema_attrs(attrs, &format!("{path}/attrs"))?;
    }
    if let Some(marker) = value.get("marker") {
        schema_marker(marker, &format!("{path}/marker"))?;
    }
    if let Some(task) = value.get("task") {
        fields_only(task, &format!("{path}/task"), &["mark"])?;
    }
    for (name, allowed) in [
        ("payload", &["format", "value", "base64"] as &[&str]),
        ("resources", &["id", "mediaType", "digest", "href", "size"]),
    ] {
        for (i, entry) in value[name].as_array().into_iter().flatten().enumerate() {
            fields_only(entry, &format!("{path}/{name}/{i}"), allowed)?;
        }
    }
    for (i, decl) in value["extensions"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let place = format!("{path}/extensions/{i}");
        fields_only(decl, &place, &["name", "required", "version", "slots"])?;
        if decl["version"].as_str().is_none_or(str::is_empty) {
            return Err(schema(
                &format!("{place}/version"),
                "extension declaration needs version",
            ));
        }
    }
    Ok(())
}

fn schema_extension(node: &Value, path: &str) -> Result<(), TreeError> {
    if node["name"].as_str().is_some_and(|name| name.contains('.')) {
        if node.get("fallback").is_none() {
            return Err(schema(
                &format!("{path}/fallback"),
                "qualified extension needs fallback",
            ));
        }
        if node["version"].as_str().is_none_or(str::is_empty) {
            return Err(schema(
                &format!("{path}/version"),
                "qualified extension needs version",
            ));
        }
    }
    Ok(())
}

fn fields_only(value: &Value, path: &str, allowed: &[&str]) -> Result<(), TreeError> {
    for key in value.as_object().into_iter().flat_map(|m| m.keys()) {
        if !allowed.contains(&key.as_str()) {
            return Err(schema(&format!("{path}/{key}"), "unknown field"));
        }
    }
    Ok(())
}

fn schema_attrs(attrs: &Value, path: &str) -> Result<(), TreeError> {
    fields_only(attrs, path, &["id", "classes", "keyValues"])?;
    if attrs.as_object().is_some_and(serde_json::Map::is_empty)
        || attrs["classes"].as_array().is_some_and(Vec::is_empty)
        || attrs["keyValues"].as_array().is_some_and(Vec::is_empty)
    {
        return Err(schema(path, "default empty attributes have no encoding"));
    }
    for (i, pair) in attrs["keyValues"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let key = pair.get(0).and_then(Value::as_str).unwrap_or("");
        let mut chars = key.bytes();
        if !chars
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            || !chars.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(schema(
                &format!("{path}/keyValues/{i}/0"),
                "attribute key is not a key",
            ));
        }
    }
    Ok(())
}

fn schema_marker(marker: &Value, path: &str) -> Result<(), TreeError> {
    fields_only(
        marker,
        path,
        &["kind", "char", "style", "delimiter", "start"],
    )?;
    if marker["start"] == 1 {
        return Err(schema(
            &format!("{path}/start"),
            "default marker start is omitted",
        ));
    }
    if marker["kind"] == "ordered"
        && (marker.get("style").is_none() || marker.get("delimiter").is_none())
    {
        return Err(schema(path, "ordered marker needs style and delimiter"));
    }
    Ok(())
}

fn schema_paragraph(node: &Value, path: &str) -> Result<(), TreeError> {
    let role = node["role"].as_str();
    if node.get("caption").is_some() && !matches!(role, Some("image" | "math")) {
        return Err(schema(
            &format!("{path}/caption"),
            "plain paragraph cannot carry a caption",
        ));
    }
    if let Some(role @ ("image" | "math")) = role {
        let children = node["children"].as_array();
        let kind = if role == "image" { "image" } else { "math" };
        let valid = children
            .is_some_and(|c| c.len() == 1 && c.first().is_some_and(|child| child["type"] == kind));
        if !valid
            || role == "math"
                && children
                    .and_then(|c| c.first())
                    .is_some_and(|child| child["display"] != true)
        {
            return Err(schema(
                &format!("{path}/children"),
                "paragraph role does not match its child",
            ));
        }
    }
    Ok(())
}

fn schema_math(node: &Value, path: &str) -> Result<(), TreeError> {
    if node.get("number").is_some() && node.get("numberPrefix").is_none()
        || node["display"] == false
            && (node.get("number").is_some() || node.get("numberPrefix").is_some())
    {
        return Err(schema(path, "math numbering fields need a display prefix"));
    }
    Ok(())
}

fn schema_include(node: &Value, path: &str) -> Result<(), TreeError> {
    if node.get("children").is_some() && node.get("file").is_none() {
        return Err(schema(path, "expanded include needs a file id"));
    }
    if node.get("caption").is_some() && node["options"].get("code").is_none() {
        return Err(schema(path, "include caption needs code mode"));
    }
    if let Some(region) = field(node, "/options/region").as_str() {
        let valid = !region.is_empty()
            && region
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
        if !valid {
            return Err(schema(
                &format!("{path}/options/region"),
                "region is not an identifier",
            ));
        }
    }
    Ok(())
}

/// Reads a document tree from JSON into owned AST types.
///
/// # Errors
///
/// A typed syntax, schema, reader or owned-type error.
pub fn read_tree_json(input: &str) -> Result<ReadTree, TreeError> {
    check_json_depth(input)?;
    // wasm32 targets have no threads; the default stack of the host applies.
    if cfg!(target_family = "wasm") {
        return read_tree_json_inner(input);
    }
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn_scoped(scope, || read_tree_json_inner(input))
            .map_err(|e| TreeError::Json(e.to_string()))?
            .join()
            .map_err(|_| TreeError::Json("tree decoding failed".into()))?
    })
}

fn read_tree_json_inner(input: &str) -> Result<ReadTree, TreeError> {
    let mut parser = serde_json::Deserializer::from_str(input);
    parser.disable_recursion_limit();
    let value = Value::deserialize(&mut parser).map_err(|e| TreeError::Json(e.to_string()))?;
    parser.end().map_err(|e| TreeError::Json(e.to_string()))?;
    read_tree_value_inner(value, Some(input.len()))
}

fn check_json_depth(input: &str) -> Result<(), TreeError> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for byte in input.bytes() {
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if !quoted && matches!(byte, b'{' | b'[') {
            depth += 1;
            if depth > 32768 {
                return Err(TreeError::Json(
                    "JSON nesting exceeds the reader bound".into(),
                ));
            }
        } else if !quoted && matches!(byte, b'}' | b']') {
            depth = depth.saturating_sub(1);
        }
    }
    Ok(())
}

/// Reads a JSON value after applying the tree gate. The value overload also
/// accepts trees built by other JSON decoders with a higher parser depth cap.
///
/// # Errors
///
/// A typed schema, reader or owned-type error.
pub fn read_tree_value(value: Value) -> Result<ReadTree, TreeError> {
    check_depth(&value)?;
    if cfg!(target_family = "wasm") {
        return read_tree_value_inner(value, None);
    }
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn_scoped(scope, move || read_tree_value_inner(value, None))
            .map_err(|e| TreeError::Json(e.to_string()))?
            .join()
            .map_err(|_| TreeError::Json("tree decoding failed".into()))?
    })
}

fn read_tree_value_inner(
    mut value: Value,
    json_bytes: Option<usize>,
) -> Result<ReadTree, TreeError> {
    check_depth(&value)?;
    replace_nul(&mut value);
    check_known_types(&value)?;
    schema::validate(&value, definitions()?)?;
    let wire = value.clone();
    if let Some(document) = value.get("document") {
        envelope(&value)?;
        if value.get("astVersion") != document.get("astVersion") {
            return Err(reader(
                "/document/astVersion",
                "envelope and document versions differ",
            ));
        }
        let mut inside = document.clone();
        if inside.get("extensions").is_none()
            && let Some(external) = value.get("extensions")
        {
            inside
                .as_object_mut()
                .map(|map| map.insert("extensions".into(), external.clone()));
        }
        value = inside;
    }
    check_schema(&value)?;
    check_reader(&value)?;
    value.as_object_mut().map(|m| m.remove("type"));
    let mut document: owned::Document =
        serde_json::from_value(value).map_err(|e| TreeError::Decode {
            path: String::new(),
            reason: e.to_string(),
        })?;
    if let (Some(bytes), Some(source)) = (json_bytes, document.src_byte_length.as_mut()) {
        *source = (*source).min(i64::try_from(bytes).unwrap_or(i64::MAX));
    }
    Ok(ReadTree { document, wire })
}

fn replace_nul(value: &mut Value) {
    let mut pending = vec![value];
    while let Some(item) = pending.pop() {
        match item {
            Value::String(s) if s.contains('\0') => *s = s.replace('\0', "\u{fffd}"),
            Value::Array(items) => pending.extend(items.iter_mut()),
            Value::Object(items) => pending.extend(items.values_mut()),
            _ => {}
        }
    }
}

fn envelope(value: &Value) -> Result<(), TreeError> {
    extension_names(value)?;
    fields_only(
        value,
        "",
        &["astVersion", "document", "vocabulary", "extensions"],
    )?;
    if !value["vocabulary"].is_null() && !value["vocabulary"].is_object() {
        return Err(schema("/vocabulary", "vocabulary is not an object"));
    }
    if let Some(external) = value["extensions"].as_array() {
        let internal = field(value, "/document/extensions").as_array();
        if let Some(internal) = internal {
            for declaration in external {
                if let Some(other) = internal
                    .iter()
                    .find(|item| item["name"] == declaration["name"])
                    && other != declaration
                {
                    return Err(reader(
                        "/extensions",
                        "envelope and document extension declarations differ",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// C2-A-165: writes the accepted wire tree without dropping fields absent from the owned AST.
#[must_use]
pub fn write_tree_json(tree: &ReadTree) -> String {
    tree.wire.to_string()
}

fn check_depth(root: &Value) -> Result<(), TreeError> {
    let mut pending = vec![(root, 0usize, 0usize, 0usize)];
    while let Some((value, blocks, inlines, json_depth)) = pending.pop() {
        let json_depth = json_depth + usize::from(value.is_object() || value.is_array());
        if json_depth > 32768 {
            return Err(reader("", "JSON value nesting exceeds the reader bound"));
        }
        match value {
            Value::Array(items) => {
                pending.extend(items.iter().map(|v| (v, blocks, inlines, json_depth)));
            }
            Value::Object(map) => {
                let kind = value["type"].as_str().unwrap_or("");
                let block = is_block(kind);
                let blocks = blocks + usize::from(block);
                let inlines = inlines + usize::from(is_inline_container(kind));
                if blocks > 200 || inlines > 200 {
                    return Err(reader("", "tree nesting exceeds 200 levels"));
                }
                pending.extend(map.iter().map(|(key, child)| {
                    let restart = kind == "include" && key == "children";
                    (
                        child,
                        if restart { 0 } else { blocks },
                        if restart { 0 } else { inlines },
                        json_depth,
                    )
                }));
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_inline_container(kind: &str) -> bool {
    matches!(
        kind,
        "citation_group"
            | "delete"
            | "emphasis"
            | "highlight"
            | "inline_extension"
            | "inline_footnote"
            | "insert"
            | "link"
            | "ruby"
            | "ruby_pair"
            | "small_caps"
            | "span"
            | "strike"
            | "strong"
            | "subscript"
            | "superscript"
            | "underline"
    )
}

fn is_block(kind: &str) -> bool {
    matches!(
        kind,
        "block_quote"
            | "definition_description"
            | "definition_list"
            | "div"
            | "footnote"
            | "list"
            | "list_item"
    )
}

fn check_known_types(root: &Value) -> Result<(), TreeError> {
    let defs = definitions()?;
    let mut pending = vec![(root, String::new())];
    while let Some((value, path)) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (item, format!("{path}/{i}"))),
            ),
            Value::Object(map) => {
                if let Some(kind) = value["type"].as_str()
                    && defs["$defs"].get(kind).is_none()
                {
                    return Err(schema(&format!("{path}/type"), "unknown node type"));
                }
                pending.extend(
                    map.iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "data" | "props"))
                        .map(|(key, item)| (item, format!("{path}/{key}"))),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_reader(root: &Value) -> Result<(), TreeError> {
    let declarations = extension_names(root)?;
    check_annotation_ids(root)?;
    check_unqualified_depth(root)?;
    let resources = resource_ids(root)?;
    let header_ids = header_ids(root);
    let mut pending = vec![(root, String::new())];
    while let Some((value, path)) = pending.pop() {
        match value {
            Value::Array(items) => {
                pending.extend(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (v, format!("{path}/{i}"))),
                );
            }
            Value::Object(map) => {
                check_object(value, &path, &declarations, &resources, &header_ids)?;
                pending.extend(
                    map.iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "data" | "props"))
                        .map(|(k, v)| (v, format!("{path}/{k}"))),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_unqualified_depth(root: &Value) -> Result<(), TreeError> {
    let mut pending = vec![(root, 0usize, String::new())];
    while let Some((value, depth, path)) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (item, depth, format!("{path}/{i}"))),
            ),
            Value::Object(map) => {
                let here = value["type"] == "inline_extension"
                    && value["name"]
                        .as_str()
                        .is_some_and(|name| !name.contains('.'));
                let depth = depth + usize::from(here);
                if depth > 32 {
                    return Err(reader(
                        &path,
                        "unqualified extension fallback exceeds 32 levels",
                    ));
                }
                pending.extend(map.iter().filter(|(key, _)| key.as_str() != "data").map(
                    |(key, child)| {
                        (
                            child,
                            if key == "children" { depth } else { 0 },
                            format!("{path}/{key}"),
                        )
                    },
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_annotation_ids(root: &Value) -> Result<(), TreeError> {
    let mut seen = HashSet::new();
    let mut pending = vec![(root, String::new())];
    while let Some((value, path)) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (item, format!("{path}/{i}"))),
            ),
            Value::Object(map) => {
                if let Some(ids) = value
                    .pointer("/data/carve.annotation/ids")
                    .and_then(Value::as_array)
                {
                    for id in ids.iter().filter_map(Value::as_str) {
                        if !seen.insert(id) {
                            return Err(reader(
                                &format!("{path}/data/carve.annotation"),
                                "annotation id repeats",
                            ));
                        }
                    }
                }
                pending.extend(
                    map.iter()
                        .filter(|(key, _)| key.as_str() != "data")
                        .map(|(key, item)| (item, format!("{path}/{key}"))),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn extension_names(root: &Value) -> Result<HashMap<String, Value>, TreeError> {
    let mut names = HashMap::new();
    for (i, decl) in root["extensions"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let name = decl["name"].as_str().unwrap_or("");
        if names.insert(name.to_string(), decl.clone()).is_some() {
            return Err(reader(
                &format!("/extensions/{i}"),
                "duplicate extension declaration",
            ));
        }
        if decl["required"].as_bool() == Some(true) && !known_extension(decl) {
            return Err(reader(
                &format!("/extensions/{i}"),
                "unknown required extension",
            ));
        }
    }
    Ok(names)
}

fn known_extension(declaration: &Value) -> bool {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../spec-data/extensions.json")).unwrap_or(Value::Null)
    });
    catalog["extensions"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            declaration["name"] == row["name"]
                && declaration["version"] == row["version"]
                && declaration["slots"] == row["slotOrder"]
        })
    })
}

fn check_object(
    value: &Value,
    path: &str,
    declarations: &HashMap<String, Value>,
    resources: &HashSet<String>,
    header_ids: &HashSet<String>,
) -> Result<(), TreeError> {
    if let Some(attrs) = value.get("attrs") {
        check_attrs(attrs, &format!("{path}/attrs"), value["type"] == "span")?;
    }
    if let Some(data) = value.get("data") {
        check_data(data, &format!("{path}/data"), declarations)?;
    }
    if let Some(props) = value.get("props") {
        check_opaque(props, &format!("{path}/props"))?;
    }
    check_position(value, path)?;
    check_resources(value, path, resources)?;
    check_header_refs(value, path, header_ids)?;
    match value["type"].as_str() {
        Some("document") => check_frontmatter(value, path),
        Some("table") => check_groups(value, path),
        Some("table_cell") => check_cell(value, path),
        Some("autolink")
            if value.get("text") == value.get("href") && value.get("text").is_some() =>
        {
            Err(reader(
                &format!("{path}/text"),
                "default autolink text equals href",
            ))
        }
        Some("smart_punctuation") => check_glyph(value, path),
        Some("include") => {
            check_frontmatter(value, path)?;
            check_range(value, path)
        }
        Some("inline_extension" | "block_extension") => check_extension(value, path, declarations),
        _ => Ok(()),
    }
}

fn check_attrs(attrs: &Value, path: &str, span: bool) -> Result<(), TreeError> {
    let mut keys: Vec<&str> = Vec::new();
    for (i, pair) in attrs["keyValues"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let key = pair.get(0).and_then(Value::as_str).unwrap_or("");
        if keys
            .iter()
            .any(|previous| same_attr_key(previous, key, span))
        {
            return Err(reader(
                &format!("{path}/keyValues/{i}"),
                "duplicate attribute key",
            ));
        }
        keys.push(key);
    }
    Ok(())
}

fn same_attr_key(left: &str, right: &str, span: bool) -> bool {
    if span
        && ["abbr", "time", "kbd"].contains(&left.to_ascii_lowercase().as_str())
        && (left == left.to_ascii_lowercase()) != (right == right.to_ascii_lowercase())
    {
        return false;
    }
    left.eq_ignore_ascii_case(right)
}

fn check_cell(cell: &Value, path: &str) -> Result<(), TreeError> {
    let children = cell["children"]
        .as_array()
        .ok_or_else(|| reader(path, "cell children are required"))?;
    if children.is_empty() && cell.get("span").is_none() {
        return Err(reader(
            &format!("{path}/children"),
            "cell without a span needs content",
        ));
    }
    if cell.get("span").is_some() && !children.is_empty() {
        return Err(reader(
            &format!("{path}/children"),
            "span mark cell has no content",
        ));
    }
    if children
        .iter()
        .any(|child| child["type"] == "cell_paragraph")
        && (children.len() != 1
            || children
                .first()
                .is_none_or(|child| child["type"] != "cell_paragraph"))
    {
        return Err(reader(
            &format!("{path}/children"),
            "cell paragraph must be the only child",
        ));
    }
    Ok(())
}

fn check_data(
    data: &Value,
    path: &str,
    declarations: &HashMap<String, Value>,
) -> Result<(), TreeError> {
    for (key, value) in data.as_object().into_iter().flat_map(|m| m.iter()) {
        if !declarations.contains_key(key) {
            return Err(reader(&format!("{path}/{key}"), "undeclared data key"));
        }
        check_opaque(value, &format!("{path}/{key}"))?;
    }
    Ok(())
}

fn check_opaque(root: &Value, path: &str) -> Result<(), TreeError> {
    let mut pending = vec![(root, path.to_string())];
    while let Some((value, path)) = pending.pop() {
        match value {
            Value::Object(map) => {
                if value.get("type").is_some() {
                    return Err(reader(&path, "node-shaped object inside opaque data"));
                }
                pending.extend(
                    map.iter()
                        .map(|(key, child)| (child, format!("{path}/{key}"))),
                );
            }
            Value::Array(items) => pending.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, child)| (child, format!("{path}/{i}"))),
            ),
            _ => {}
        }
    }
    Ok(())
}

fn check_position(node: &Value, path: &str) -> Result<(), TreeError> {
    for pos in node
        .get("pos")
        .into_iter()
        .chain(node["segments"].as_array().into_iter().flatten())
    {
        let start = pos["startOffset"].as_u64().unwrap_or(0);
        let end = pos["endOffset"].as_u64().unwrap_or(0);
        let first = pos["startLine"].as_u64().unwrap_or(0);
        let last = pos["endLine"].as_u64().unwrap_or(0);
        if start > end
            || first > last
            || first == last && pos["startColumn"].as_u64() > pos["endColumn"].as_u64()
        {
            return Err(reader(
                &format!("{path}/pos"),
                "position ends before it starts",
            ));
        }
    }
    Ok(())
}

fn check_frontmatter(node: &Value, path: &str) -> Result<(), TreeError> {
    for (i, child) in node["children"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if i > 0 && child["type"] == "frontmatter" {
            return Err(reader(
                &format!("{path}/children/{i}"),
                "frontmatter is not first",
            ));
        }
    }
    Ok(())
}

fn resource_ids(root: &Value) -> Result<HashSet<String>, TreeError> {
    let mut ids = HashSet::new();
    for (i, entry) in root["resources"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let id = entry["id"].as_str().unwrap_or("");
        if !ids.insert(id.to_string()) {
            return Err(reader(
                &format!("/resources/{i}/id"),
                "duplicate resource id",
            ));
        }
    }
    Ok(ids)
}

fn check_resources(node: &Value, path: &str, resources: &HashSet<String>) -> Result<(), TreeError> {
    if let Some(id) = node["resourceId"].as_str()
        && !resources.contains(id)
    {
        return Err(reader(
            &format!("{path}/resourceId"),
            "resource id is not in the manifest",
        ));
    }
    for (i, id) in node["resourceIds"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if !id.as_str().is_some_and(|id| resources.contains(id)) {
            return Err(reader(
                &format!("{path}/resourceIds/{i}"),
                "resource id is not in the manifest",
            ));
        }
    }
    Ok(())
}

fn header_ids(root: &Value) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut pending = vec![root];
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(map) => {
                if value["type"] == "table_cell" && value["header"] == true {
                    if let Some(id) = field(value, "/attrs/id").as_str() {
                        ids.insert(id.to_string());
                    }
                }
                pending.extend(map.values());
            }
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    ids
}

fn check_header_refs(node: &Value, path: &str, ids: &HashSet<String>) -> Result<(), TreeError> {
    for (i, id) in node["headerIds"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if !id.as_str().is_some_and(|id| ids.contains(id)) {
            return Err(reader(
                &format!("{path}/headerIds/{i}"),
                "header id is missing or is not a header cell",
            ));
        }
    }
    Ok(())
}

fn check_extension(
    node: &Value,
    path: &str,
    declarations: &HashMap<String, Value>,
) -> Result<(), TreeError> {
    let name = node["name"].as_str().unwrap_or("");
    if !name.contains('.') {
        let allowed = [
            "type", "name", "children", "attrs", "data", "pos", "segments", "metadata", "dir",
            "lang",
        ];
        if let Some(key) = node
            .as_object()
            .into_iter()
            .flat_map(|map| map.keys())
            .find(|key| !allowed.contains(&key.as_str()))
        {
            return Err(reader(
                &format!("{path}/{key}"),
                "unqualified extension has a noncore field",
            ));
        }
    } else if !declarations.contains_key(name) {
        return Err(reader(path, "qualified extension is not declared"));
    }
    check_extension_source(node, path)?;
    if let Some(decl) = declarations.get(name) {
        if node["version"] != decl["version"] {
            return Err(reader(
                &format!("{path}/version"),
                "extension version differs from declaration",
            ));
        }
        for slot in node["slots"].as_object().into_iter().flat_map(|m| m.keys()) {
            if !decl["slots"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item == slot))
            {
                return Err(reader(
                    &format!("{path}/slots/{slot}"),
                    "extension slot is not declared",
                ));
            }
        }
    }
    if name.contains('.') {
        let mut pending = vec![&node["fallback"]];
        while let Some(value) = pending.pop() {
            if matches!(
                value["type"].as_str(),
                Some("inline_extension" | "block_extension")
            ) {
                return Err(reader(
                    &format!("{path}/fallback"),
                    "qualified fallback holds an extension node",
                ));
            }
            match value {
                Value::Object(map) => pending.extend(map.values()),
                Value::Array(items) => pending.extend(items),
                _ => {}
            }
        }
    }
    Ok(())
}

fn check_extension_source(node: &Value, path: &str) -> Result<(), TreeError> {
    let Some(source) = node.get("source") else {
        return Ok(());
    };
    let name = source["name"].as_str().unwrap_or("");
    let form = source["form"].as_str().unwrap_or("");
    let valid = match form {
        "inline" => {
            name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        }
        "fence" => {
            !name.starts_with('=')
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        }
        "div" => {
            !name.is_empty()
                && ![
                    "note",
                    "tip",
                    "warning",
                    "danger",
                    "info",
                    "success",
                    "example",
                    "quote",
                    "figure",
                    "bibliography",
                    "footnotes",
                    "glossary",
                    "index",
                    "references",
                    "toc",
                ]
                .contains(&name)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(reader(
            &format!("{path}/source"),
            "source name cannot claim this form",
        ))
    }
}

fn check_groups(table: &Value, path: &str) -> Result<(), TreeError> {
    let mut phase = 0;
    let mut heads = 0;
    let mut feet = 0;
    for (i, group) in table["children"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        match group["type"].as_str() {
            Some("table_head") => {
                heads += 1;
                if phase > 0 || heads > 1 {
                    return Err(reader(&format!("{path}/children/{i}"), "table group order"));
                }
            }
            Some("table_body") => {
                if phase > 1 {
                    return Err(reader(&format!("{path}/children/{i}"), "table group order"));
                }
                phase = 1;
            }
            Some("table_foot") => {
                feet += 1;
                if feet > 1 {
                    return Err(reader(&format!("{path}/children/{i}"), "table group order"));
                }
                phase = 2;
            }
            _ => {}
        }
    }
    check_rowspans(table, path)
}

fn check_rowspans(table: &Value, path: &str) -> Result<(), TreeError> {
    for (g, group) in table["children"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(rows) = group["children"].as_array() else {
            continue;
        };
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row["children"].as_array().into_iter().flatten().enumerate() {
                if cell["rowspan"]
                    .as_u64()
                    .is_some_and(|span| span > (rows.len() - r) as u64)
                {
                    return Err(reader(
                        &format!("{path}/children/{g}/children/{r}/children/{c}"),
                        "rowspan crosses group end",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn check_glyph(node: &Value, path: &str) -> Result<(), TreeError> {
    let kind = node["kind"].as_str().unwrap_or("");
    let glyph = node["glyph"].as_str().unwrap_or("");
    let expected = crate::tables::SMART_RUNS
        .iter()
        .find(|row| row.2 == kind)
        .map(|row| (row.0, row.3))
        .or_else(|| {
            crate::tables::SMART_QUOTES
                .iter()
                .find(|row| row.0 == kind)
                .map(|row| (row.1, row.2))
        })
        .or_else(|| {
            crate::tables::SMART_DASHES
                .iter()
                .find(|row| row.0 == kind)
                .map(|row| (row.1, row.2))
        });
    if expected
        .is_none_or(|(source, expected_glyph)| node["source"] != source || glyph != expected_glyph)
    {
        return Err(reader(
            path,
            "smart punctuation source or glyph differs from its kind",
        ));
    }
    Ok(())
}

fn check_range(node: &Value, path: &str) -> Result<(), TreeError> {
    for key in ["lines", "rows", "cols"] {
        let Some(range) = node
            .get("options")
            .and_then(|options| options.get(key))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some((from, to)) = range.split_once('-') else {
            continue;
        };
        if from.len() > to.len() || from.len() == to.len() && from > to {
            return Err(reader(
                &format!("{path}/options/{key}"),
                "include range is reversed",
            ));
        }
    }
    Ok(())
}
