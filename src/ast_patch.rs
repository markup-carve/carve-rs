//! Position-independent patch replay for PART 12 exchange trees.

use std::collections::BTreeSet;
use std::fmt;

use crate::ast::Document;
use crate::ast_json::{from_json, parse_value, to_json, value_to_json, AstJsonError, Json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstPatchOperation {
    Add { path: String, value: String },
    Replace { path: String, value: String },
    Remove { path: String },
}

/// Encode operations using the shared `{op,path,value}` patch wire shape.
pub fn ast_patch_to_json(operations: &[AstPatchOperation]) -> Result<String, AstPatchError> {
    let mut encoded = Vec::with_capacity(operations.len());
    for operation in operations {
        let mut object = std::collections::BTreeMap::new();
        match operation {
            AstPatchOperation::Add { path, value } => {
                object.insert("op".into(), Json::String("add".into()));
                object.insert("path".into(), Json::String(path.clone()));
                object.insert("value".into(), parse_value(value)?);
            }
            AstPatchOperation::Replace { path, value } => {
                object.insert("op".into(), Json::String("replace".into()));
                object.insert("path".into(), Json::String(path.clone()));
                object.insert("value".into(), parse_value(value)?);
            }
            AstPatchOperation::Remove { path } => {
                object.insert("op".into(), Json::String("remove".into()));
                object.insert("path".into(), Json::String(path.clone()));
            }
        }
        encoded.push(Json::Object(object));
    }
    Ok(value_to_json(&Json::Array(encoded)))
}

/// Decode and validate the shared `{op,path,value}` patch wire shape.
pub fn ast_patch_from_json(input: &str) -> Result<Vec<AstPatchOperation>, AstPatchError> {
    let Json::Array(operations) = parse_value(input)? else {
        return Err(AstPatchError("patch JSON must be an array".into()));
    };
    operations
        .into_iter()
        .map(|operation| {
            let Json::Object(mut operation) = operation else {
                return Err(AstPatchError("patch operation must be an object".into()));
            };
            let op = match operation.remove("op") {
                Some(Json::String(value)) => value,
                _ => return Err(AstPatchError("patch operation requires a string op".into())),
            };
            let path = match operation.remove("path") {
                Some(Json::String(value)) => value,
                _ => {
                    return Err(AstPatchError(
                        "patch operation requires a string path".into(),
                    ))
                }
            };
            let value = operation.remove("value");
            if !operation.is_empty() {
                return Err(AstPatchError(
                    "patch operation has an unknown property".into(),
                ));
            }
            match (op.as_str(), value) {
                ("add", Some(value)) => Ok(AstPatchOperation::Add {
                    path,
                    value: value_to_json(&value),
                }),
                ("replace", Some(value)) => Ok(AstPatchOperation::Replace {
                    path,
                    value: value_to_json(&value),
                }),
                ("remove", None) => Ok(AstPatchOperation::Remove { path }),
                ("add" | "replace", None) => Err(AstPatchError(
                    "patch add and replace require a value".into(),
                )),
                ("remove", Some(_)) => {
                    Err(AstPatchError("patch remove must not carry a value".into()))
                }
                _ => Err(AstPatchError("unknown patch operation".into())),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstPatchError(String);

impl fmt::Display for AstPatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AstPatchError {}
impl From<AstJsonError> for AstPatchError {
    fn from(value: AstJsonError) -> Self {
        Self(value.to_string())
    }
}

fn clean(value: &Json, strip_metadata: bool) -> Json {
    match value {
        Json::Array(values) => Json::Array(
            values
                .iter()
                .map(|value| clean(value, strip_metadata))
                .collect(),
        ),
        Json::Object(values) => Json::Object(
            values
                .iter()
                .filter(|(key, _)| {
                    !strip_metadata || (key.as_str() != "pos" && key.as_str() != "srcByteLength")
                })
                .map(|(key, value)| {
                    (
                        key.clone(),
                        clean(value, strip_metadata && key.as_str() != "keyValues"),
                    )
                })
                .collect(),
        ),
        value => value.clone(),
    }
}

fn strip_metadata(path: &str) -> bool {
    !path.split('/').any(|part| part == "keyValues")
}

fn pointer(path: &str, key: impl ToString) -> String {
    format!(
        "{path}/{}",
        key.to_string().replace('~', "~0").replace('/', "~1")
    )
}

fn build(before: &Json, after: &Json, path: &str, out: &mut Vec<AstPatchOperation>) {
    let strip = strip_metadata(path);
    if clean(before, strip) == clean(after, strip) {
        return;
    }
    match (before, after) {
        (Json::Array(before), Json::Array(after)) if before.len() == after.len() => {
            for (index, (before, after)) in before.iter().zip(after).enumerate() {
                build(before, after, &pointer(path, index), out);
            }
        }
        (Json::Object(before), Json::Object(after)) => {
            let keys = before
                .keys()
                .chain(after.keys())
                .filter(|key| !strip || (key.as_str() != "pos" && key.as_str() != "srcByteLength"))
                .cloned()
                .collect::<BTreeSet<_>>();
            for key in keys {
                let child = pointer(path, &key);
                match (before.get(&key), after.get(&key)) {
                    (Some(_), None) => out.push(AstPatchOperation::Remove { path: child }),
                    (None, Some(value)) => {
                        let value = value_to_json(&clean(value, strip_metadata(&child)));
                        out.push(AstPatchOperation::Add { path: child, value });
                    }
                    (Some(before), Some(after)) => build(before, after, &child, out),
                    (None, None) => {}
                }
            }
        }
        _ => out.push(AstPatchOperation::Replace {
            path: path.into(),
            value: value_to_json(&clean(after, strip)),
        }),
    }
}

pub fn create_ast_patch(
    before: &Document,
    after: &Document,
) -> Result<Vec<AstPatchOperation>, AstPatchError> {
    let before = parse_value(&to_json(before))?;
    let after = parse_value(&to_json(after))?;
    let mut operations = Vec::new();
    build(&before, &after, "", &mut operations);
    Ok(operations)
}

fn decode(path: &str) -> Result<Vec<String>, AstPatchError> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    if !path.starts_with('/') {
        return Err(AstPatchError(format!("invalid JSON Pointer {path:?}")));
    }
    Ok(path[1..]
        .split('/')
        .map(|part| part.replace("~1", "/").replace("~0", "~"))
        .collect())
}

fn index(value: &str, length: usize, allow_end: bool) -> Result<usize, AstPatchError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(AstPatchError(format!(
            "array component {value:?} is not an index"
        )));
    }
    let index = value
        .parse::<usize>()
        .map_err(|_| AstPatchError(format!("array index {value:?} is out of range")))?;
    let valid = if allow_end {
        index <= length
    } else {
        index < length
    };
    if !valid {
        return Err(AstPatchError(format!(
            "array index {value:?} is out of range"
        )));
    }
    Ok(index)
}

fn apply_at(
    mut root: Json,
    parts: &[String],
    operation: &AstPatchOperation,
) -> Result<Json, AstPatchError> {
    let operation_path = match operation {
        AstPatchOperation::Add { path, .. }
        | AstPatchOperation::Replace { path, .. }
        | AstPatchOperation::Remove { path } => path,
    };
    let strip = strip_metadata(operation_path);
    let (key, rest) = parts
        .split_first()
        .ok_or_else(|| AstPatchError("patch path cannot be empty here".into()))?;
    if !rest.is_empty() {
        match &mut root {
            Json::Array(values) => {
                let i = index(key, values.len(), false)?;
                values[i] = apply_at(values[i].clone(), rest, operation)?;
            }
            Json::Object(values) => {
                let child = values.get(key).cloned().ok_or_else(|| {
                    AstPatchError(format!("path component {key:?} does not exist"))
                })?;
                values.insert(key.clone(), apply_at(child, rest, operation)?);
            }
            _ => {
                return Err(AstPatchError(format!(
                    "path component {key:?} does not exist"
                )))
            }
        }
        return Ok(root);
    }
    match &mut root {
        Json::Array(values) => match operation {
            AstPatchOperation::Add { value, .. } => {
                let i = index(key, values.len(), true)?;
                values.insert(i, clean(&parse_value(value)?, strip));
            }
            AstPatchOperation::Replace { value, .. } => {
                let i = index(key, values.len(), false)?;
                values[i] = clean(&parse_value(value)?, strip);
            }
            AstPatchOperation::Remove { .. } => {
                let i = index(key, values.len(), false)?;
                values.remove(i);
            }
        },
        Json::Object(values) => match operation {
            AstPatchOperation::Add { value, .. } => {
                values.insert(key.clone(), clean(&parse_value(value)?, strip));
            }
            AstPatchOperation::Replace { value, .. } => {
                if !values.contains_key(key) {
                    return Err(AstPatchError(format!(
                        "path component {key:?} does not exist"
                    )));
                }
                values.insert(key.clone(), clean(&parse_value(value)?, strip));
            }
            AstPatchOperation::Remove { .. } => {
                if values.remove(key).is_none() {
                    return Err(AstPatchError(format!(
                        "path component {key:?} does not exist"
                    )));
                }
            }
        },
        _ => return Err(AstPatchError("patch path parent is not a container".into())),
    }
    Ok(root)
}

pub fn apply_ast_patch(
    ast: &Document,
    operations: &[AstPatchOperation],
) -> Result<Document, AstPatchError> {
    let mut root = clean(&parse_value(&to_json(ast))?, true);
    for operation in operations {
        let path = match operation {
            AstPatchOperation::Add { path, .. }
            | AstPatchOperation::Replace { path, .. }
            | AstPatchOperation::Remove { path } => path,
        };
        let parts = decode(path)?;
        if parts.is_empty() {
            root = match operation {
                AstPatchOperation::Remove { .. } => {
                    return Err(AstPatchError("the document root cannot be removed".into()))
                }
                AstPatchOperation::Add { value, .. } | AstPatchOperation::Replace { value, .. } => {
                    clean(&parse_value(value)?, true)
                }
            };
        } else {
            root = apply_at(root, &parts, operation)?;
        }
    }
    let Json::Object(values) = &mut root else {
        return Err(AstPatchError(
            "patch result is not a PART 12 document root".into(),
        ));
    };
    if !matches!(values.get("type"), Some(Json::String(value)) if value == "document")
        || !matches!(values.get("children"), Some(Json::Array(_)))
    {
        return Err(AstPatchError(
            "patch result is not a PART 12 document root".into(),
        ));
    }
    values.insert("srcByteLength".into(), Json::Number(0));
    Ok(from_json(&value_to_json(&root))?)
}
