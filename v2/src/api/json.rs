//! The JSON boundary: one request in, one response out, for bindings
//! (`PyO3`, `magnus`, `wasm-bindgen`) and for hosts that run the WASI command
//! `carve2 call`. Names are the `tooling.md` spellings.
//!
//! A request is an object: `call`, the call name; `source` (text) or
//! `sourceBytes` (a list of byte values) for calls that read Carve source;
//! `tree` (a tree as a JSON value) for calls that read a tree, and for
//! `render` instead of a source; `json` (text) for `decodeTree`; `options`,
//! an object of the options of `data/options.json`; `positions` (default
//! true) for `parse` and `parseWithDiagnostics`; `target` for `write`.
//!
//! The response is `{"ok": RESULT}` or `{"error": {"name", "category",
//! "status", "message"}}`, the error row of C2-T-040.
//!
//! ```
//! let out = carve2::api::json::call(r#"{"call":"render","source":"Hi *there*"}"#);
//! assert_eq!(out, r#"{"ok":{"html":"<p>Hi <strong>there</strong></p>"}}"#);
//! ```

use super::{Carve, EncodeOptions, Entry, Error, Input, Tree, options::CallOptions};
use serde_json::{Map, Value, json};

/// Runs one request and returns the response JSON text.
#[must_use]
pub fn call(request: &str) -> String {
    let response = match serde_json::from_str::<Value>(request) {
        Ok(req) => dispatch(&Carve::default(), &req),
        Err(e) => Err(Error::option("request", format!("invalid JSON: {e}"))),
    };
    match response {
        Ok(result) => json!({ "ok": result }).to_string(),
        Err(e) => json!({ "error": {
            "name": e.spec_name(),
            "category": e.category(),
            "status": e.status(),
            "message": e.to_string(),
        }})
        .to_string(),
    }
}

/// The exit status of a response of [`call`]: 0 for `ok`, else the status
/// of its category.
#[must_use]
pub fn status(response: &str) -> i32 {
    serde_json::from_str::<Value>(response)
        .ok()
        .and_then(|v| v.pointer("/error/status").and_then(Value::as_i64))
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0)
}

fn dispatch(host: &Carve, req: &Value) -> Result<Value, Error> {
    let name = req
        .get("call")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::option("call", "the request names no call"))?;
    let opts = super::options::from_json(name, req.get("options"))?;
    match name {
        "parse" | "parseWithDiagnostics" | "parseWithLayout" => parse_call(name, req, &opts),
        "render" | "renderWithDiagnostics" => render_call(name, req, &opts),
        "lint" => {
            let bytes = source(req)?;
            Ok(json!({ "diagnostics": entries(&super::lint(input(&bytes), &opts.lint)?) }))
        }
        "format" | "formatWithDiagnostics" => {
            let bytes = source(req)?;
            let written = host.format_with_diagnostics(input(&bytes), &opts.write)?;
            Ok(json!({ "source": written.output, "diagnostics": entries(&written.diagnostics) }))
        }
        "write" | "writeWithDiagnostics" => write_call(host, req, &opts),
        "importDocument" => Err(Error::Unsupported {
            call: "importDocument",
        }),
        "migrateTree" => Err(Error::Unsupported {
            call: "migrateTree",
        }),
        _ => tree_call(name, req),
    }
}

fn tree_call(name: &str, req: &Value) -> Result<Value, Error> {
    if name == "decodeTree" {
        let text = req
            .get("json")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::option("json", "decodeTree takes json text"))?;
        return Ok(json!({ "tree": tree_value(&super::decode_tree(text)?) }));
    }
    let mut tree = tree(req)?;
    tree.set_file(req.get("file").and_then(Value::as_str).map(str::to_string));
    Ok(match name {
        "validateTree" => json!({}),
        "encodeTree" => json!({ "json": super::encode_tree(&tree) }),
        "encodeEnvelope" => json!({ "json": super::encode_envelope(&tree) }),
        _ => json!({ "dependencies": dependencies(&tree) }),
    })
}

fn parse_call(name: &str, req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    let bytes = source(req)?;
    let positions = req
        .get("positions")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if name == "parseWithLayout" {
        super::parse_with_layout(input(&bytes), &opts.options)?;
    }
    let mut out = Map::new();
    let tree = if name == "parseWithDiagnostics" {
        let parsed = super::parse_with_diagnostics(input(&bytes), &opts.options)?;
        out.insert("diagnostics".into(), entries(&parsed.diagnostics));
        out.insert(
            "suppressedDiagnostics".into(),
            parsed.suppressed_diagnostics.into(),
        );
        parsed.tree
    } else {
        super::parse(input(&bytes), &opts.options)?
    };
    let encoded = super::encode_tree_with(&tree, EncodeOptions { positions });
    out.insert(
        "tree".into(),
        serde_json::from_str(&encoded).unwrap_or(Value::Null),
    );
    Ok(Value::Object(out))
}

fn render_call(name: &str, req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    let o = &opts.options;
    if req.get("tree").is_some() {
        let tree = tree(req)?;
        if name == "render" {
            return Ok(json!({ "html": super::render_tree(&tree, o)? }));
        }
        return Ok(rendered(&super::render_tree_with_diagnostics(&tree, o)?));
    }
    let bytes = source(req)?;
    if name == "render" {
        return Ok(json!({ "html": super::render(input(&bytes), o)? }));
    }
    Ok(rendered(&super::render_with_diagnostics(input(&bytes), o)?))
}

fn write_call(host: &Carve, req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    let tree = tree(req)?;
    let written = match req.get("target").and_then(Value::as_str) {
        Some(target) => host.write_to(target, &tree, &opts.write)?,
        None => host.write_with_diagnostics(&tree, &opts.write)?,
    };
    Ok(json!({ "output": written.output, "diagnostics": entries(&written.diagnostics) }))
}

fn rendered(r: &super::Rendered) -> Value {
    json!({
        "html": r.html,
        "diagnostics": entries(&r.diagnostics),
        "suppressedDiagnostics": r.suppressed_diagnostics,
    })
}

/// The source of a request: `source` text or `sourceBytes`.
fn source(req: &Value) -> Result<Source, Error> {
    if let Some(text) = req.get("source").and_then(Value::as_str) {
        return Ok(Source::Text(text.to_string()));
    }
    let Some(items) = req.get("sourceBytes").and_then(Value::as_array) else {
        return Err(Error::option(
            "source",
            "the call takes source or sourceBytes",
        ));
    };
    items
        .iter()
        .map(|b| b.as_u64().and_then(|n| u8::try_from(n).ok()))
        .collect::<Option<Vec<u8>>>()
        .map(Source::Bytes)
        .ok_or_else(|| Error::option("sourceBytes", "an item is not a byte value"))
}

enum Source {
    Text(String),
    Bytes(Vec<u8>),
}

fn input(s: &Source) -> Input<'_> {
    match s {
        Source::Text(t) => Input::Text(t),
        Source::Bytes(b) => Input::Bytes(b),
    }
}

fn tree(req: &Value) -> Result<Tree, Error> {
    let value = req
        .get("tree")
        .ok_or_else(|| Error::option("tree", "the call takes a tree"))?;
    Tree::from_value(value)
}

fn tree_value(tree: &Tree) -> Value {
    serde_json::from_str(&super::encode_tree(tree)).unwrap_or(Value::Null)
}

fn entries(list: &[Entry]) -> Value {
    serde_json::to_value(list).unwrap_or_else(|_| Value::Array(Vec::new()))
}

/// C2-T-022: `path`, `from`, `file` and `read` of each dependency.
fn dependencies(tree: &Tree) -> Value {
    super::include_dependencies(tree)
        .into_iter()
        .map(|d| {
            let mut m = Map::new();
            m.insert("path".into(), d.path.into());
            if let Some(from) = d.from {
                m.insert("from".into(), from.into());
            }
            m.insert("read".into(), d.file.is_some().into());
            if let Some(file) = d.file {
                m.insert("file".into(), file.into());
            }
            Value::Object(m)
        })
        .collect()
}
