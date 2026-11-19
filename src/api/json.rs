//! The JSON boundary: one request in, one response out, for bindings
//! (`PyO3`, `magnus`, `wasm-bindgen`) and for hosts that run the WASI command
//! `carve call`. Names are the `tooling.md` spellings.
//!
//! A request is an object: `call`, the call name; `source` (text) or
//! `sourceBytes` (a list of byte values) for calls that read Carve source;
//! `tree` (a tree as a JSON value) for calls that read a tree, and for
//! `render` instead of a source; `json` (text) for `decodeTree`; `options`,
//! an object of the options of `data/options.json`; `positions` (default
//! true) for `parse` and `parseWithDiagnostics`; `target` for `write`;
//! and, PROVISIONAL, `host` with `documentTransforms` for the render calls.
//!
//! The response is `{"ok": RESULT}` or `{"error": {"name", "category",
//! "status", "message"}}`, the error row of C2-T-040.
//!
//! ```
//! let out = carve::api::json::call(r#"{"call":"render","source":"Hi *there*"}"#);
//! assert_eq!(out, r#"{"ok":{"html":"<p>Hi <strong>there</strong></p>"}}"#);
//! ```

use super::{Carve, EncodeOptions, Entry, Error, Input, Tree, options::CallOptions};
use serde_json::{Map, Value, json};

/// Runs one request and returns the response JSON text.
#[must_use]
pub fn call(request: &str) -> String {
    let response = match serde_json::from_str::<Value>(request) {
        Ok(req) => host(&req).and_then(|h| dispatch(&h, &req)),
        Err(e) => Err(Error::option("request", format!("invalid JSON: {e}"))),
    };
    match response {
        Ok(Reply::Value(result)) => json!({ "ok": result }).to_string(),
        Ok(Reply::Tree(fields, tree)) => with_tree(fields, &tree),
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

/// A result, or the members of a parse result and its tree as the JSON text
/// the engine wrote: reading that text into a `Value` only to write it again
/// cost several times the parse (carve-rs#4336).
enum Reply {
    Value(Value),
    Tree(Map<String, Value>, String),
}

fn with_tree(fields: Map<String, Value>, tree: &str) -> String {
    let head = Value::Object(fields).to_string();
    let mut out = String::with_capacity(tree.len() + head.len() + 16);
    out.push_str(r#"{"ok":"#);
    out.push_str(&head[..head.len() - 1]);
    if head.len() > 2 {
        out.push(',');
    }
    // `tree` sorts after every other member, as the map would place it.
    out.push_str(r#""tree":"#);
    out.push_str(tree);
    out.push_str("}}");
    out
}

fn dispatch(host: &Carve, req: &Value) -> Result<Reply, Error> {
    let name = req
        .get("call")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::option("call", "the request names no call"))?;
    let opts = super::options::from_json(name, req.get("options"))?;
    let enabled;
    let host = if opts.extensions.is_empty() && opts.renderers.is_empty() {
        host
    } else {
        enabled = with_extensions(host, &opts)?;
        &enabled
    };
    if matches!(name, "parse" | "parseWithDiagnostics" | "parseWithLayout") {
        return parse_call(host, name, req, &opts);
    }
    match name {
        "render" | "renderWithDiagnostics" => render_call(host, name, req, &opts),
        "lint" => lint_call(req, &opts),
        "format" | "formatWithDiagnostics" => format_call(host, name, req, &opts),
        "write" | "writeWithDiagnostics" => write_call(host, req, &opts),
        "importDocument" => import_call(host, req, &opts),
        "migrateTree" => Err(Error::Unsupported {
            call: "migrateTree",
        }),
        _ => tree_call(name, req),
    }
    .map(Reply::Value)
}

fn with_extensions(host: &Carve, opts: &CallOptions) -> Result<Carve, Error> {
    if let Some(ext) = opts.renderers.iter().find(|r| !opts.extensions.contains(r)) {
        return Err(Error::option(
            "renderers",
            format!("{ext} is not among the enabled extensions"),
        ));
    }
    let mut enabled = host.clone();
    for ext in &opts.extensions {
        enabled.enable_standard(ext, opts.renderers.contains(ext))?;
    }
    Ok(enabled)
}

fn lint_call(req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    let bytes = source(req)?;
    Ok(json!({ "diagnostics": entries(&super::lint(input(&bytes), &opts.lint)?) }))
}

fn format_call(host: &Carve, name: &str, req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    let bytes = source(req)?;
    let written = host.format_with_diagnostics(input(&bytes), &opts.write)?;
    if name == "format" {
        super::refuse_losses(written.clone())?;
    }
    Ok(json!({ "source": written.output, "diagnostics": entries(&written.diagnostics) }))
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

fn parse_call(host: &Carve, name: &str, req: &Value, opts: &CallOptions) -> Result<Reply, Error> {
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
        let parsed = host.parse_with_diagnostics(input(&bytes), &opts.options)?;
        out.insert("diagnostics".into(), entries(&parsed.diagnostics));
        out.insert(
            "suppressedDiagnostics".into(),
            parsed.suppressed_diagnostics.into(),
        );
        parsed.tree
    } else {
        host.parse(input(&bytes), &opts.options)?
    };
    let encoded = super::encode_tree_with(&tree, EncodeOptions { positions });
    Ok(Reply::Tree(out, encoded))
}

fn render_call(host: &Carve, name: &str, req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    let o = &opts.options;
    if req.get("tree").is_some() {
        let tree = tree(req)?;
        if name == "render" {
            return Ok(json!({ "html": host.render_tree(&tree, o)? }));
        }
        return Ok(rendered(&host.render_tree_with_diagnostics(&tree, o)?));
    }
    let bytes = source(req)?;
    if name == "render" {
        return Ok(json!({ "html": host.render(input(&bytes), o)? }));
    }
    Ok(rendered(&host.render_with_diagnostics(input(&bytes), o)?))
}

/// PROVISIONAL: `host.documentTransforms`, a list of `{name, after, insert}`
/// that inserts the node `insert` after the node at the pointer `after`, in
/// order (C2-X-053). `name` is the qualified name of the inserted extension.
fn host(req: &Value) -> Result<Carve, Error> {
    let mut host = Carve::default();
    let Some(spec) = req.get("host") else {
        return Ok(host);
    };
    let Some(spec) = spec.as_object() else {
        return Err(Error::option("host", "the host is not an object"));
    };
    for (key, list) in spec {
        if key != "documentTransforms" {
            return Err(Error::option("host", format!("unknown field {key}")));
        }
        let Some(list) = list.as_array() else {
            return Err(Error::option("documentTransforms", "not a list"));
        };
        for item in list {
            let text = |k: &str| {
                item.get(k)
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        Error::option("documentTransforms", format!("{k} is missing or not text"))
                    })
            };
            let node = item
                .get("insert")
                .filter(|n| n.is_object())
                .cloned()
                .ok_or_else(|| {
                    Error::option("documentTransforms", "insert is missing or not a node")
                })?;
            let version = node
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("1")
                .to_string();
            host.register_extension(super::Extension {
                declaration: super::ExtensionDeclaration {
                    name: text("name")?,
                    version,
                    required: false,
                    slots: Vec::new(),
                },
                renderers: Vec::new(),
                transform: Some(Box::new(super::extension::InsertTransform {
                    after: text("after")?,
                    node,
                })),
            })?;
        }
    }
    Ok(host)
}

/// C2-M-001: `source` is the HTML, `from` must be `html`; `rawHtml` and
/// `strictLosses` are options.
fn import_call(host: &Carve, req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    match req.get("from").and_then(Value::as_str) {
        Some("html") => {}
        Some(other) => return Err(Error::option("from", format!("unknown value \"{other}\""))),
        None => return Err(Error::option("from", "importDocument takes from: \"html\"")),
    }
    let html = req
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::option("source", "importDocument takes the HTML as source text"))?;
    let imported = host.import_document(
        html,
        &super::ImportOptions {
            raw_html: opts.options.raw_html,
            strict_losses: opts.options.strict_losses,
        },
    )?;
    Ok(json!({
        "tree": tree_value(&imported.tree),
        "source": imported.source,
        "diagnostics": entries(&imported.diagnostics),
    }))
}

fn write_call(host: &Carve, req: &Value, opts: &CallOptions) -> Result<Value, Error> {
    let tree = tree(req)?;
    let written = match req.get("target").and_then(Value::as_str) {
        Some(target) => host.write_to(target, &tree, &opts.write)?,
        None => host.write_with_diagnostics(&tree, &opts.write)?,
    };
    if req.get("call").and_then(Value::as_str) == Some("write") {
        super::refuse_losses(written.clone())?;
    }
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
