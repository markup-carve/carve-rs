//! The include vectors of the JavaScript reference engine
//! (`carve-js` `v2/test/include-vectors`, copied to `tests/data/include-vectors`):
//! HTML, the parse and render-loss codes with their files, the dependencies
//! (C2-N-083, C2-N-084) and the positions of included nodes. `fmt` waits for
//! the writer.
mod common;

use carve2::include::{Cause, Includes, Request, Resolver, fs};
use carve2::options::Options;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Vectors whose expectation a clause shows wrong, with the clause.
const HELD: &[(&str, &str)] = &[];

fn vectors() -> Vec<Value> {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/include-vectors/vectors"
    );
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    names.sort();
    names
        .iter()
        .map(|p| common::parse_json(&std::fs::read_to_string(p).unwrap()))
        .collect()
}

fn virtual_resolver(files: &Value, strip_dot: bool) -> Resolver {
    let map: BTreeMap<String, String> = files
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
        .collect();
    Arc::new(move |r: &Request<'_>| {
        let path = if strip_dot {
            r.path.trim_start_matches("./")
        } else {
            r.path
        };
        map.get(path)
            .map(|t| (t.as_bytes().to_vec(), path.to_string()))
            .ok_or(Cause::NotFound)
    })
}

/// A directory for a filesystem vector, its files and symbolic links written.
fn write_tree(name: &str, tree: &Value) -> PathBuf {
    let base = std::env::temp_dir().join(format!("carve2-vec-{}-{name}", std::process::id()));
    let base = std::fs::canonicalize({
        std::fs::create_dir_all(&base).unwrap();
        &base
    })
    .unwrap();
    for (rel, v) in tree.as_object().into_iter().flatten() {
        let path = base.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Some(target) = v.get("symlink").and_then(Value::as_str) {
            #[cfg(unix)]
            std::os::unix::fs::symlink(base.join(target), &path).unwrap();
        } else {
            std::fs::write(&path, absolute(v.as_str().unwrap_or(""), &base)).unwrap();
        }
    }
    base
}

/// `<ABS:rel>` stands for the absolute path of `rel` in the tree.
fn absolute(text: &str, base: &Path) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("<ABS:") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 5..];
        let end = tail.find('>').unwrap_or(tail.len());
        out.push_str(&base.join(&tail[..end]).display().to_string());
        rest = tail.get(end + 1..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

struct Setup {
    entry: String,
    opts: Options,
    tmp: Option<PathBuf>,
    root: Option<PathBuf>,
}

fn setup(v: &Value) -> Setup {
    let name = v["name"].as_str().unwrap_or("");
    let mut tmp = None;
    let mut root = None;
    let (entry, resolver): (String, Option<Resolver>) = match v["resolver"].as_str().unwrap_or("") {
        "none" => (v["entry"].as_str().unwrap_or("").into(), None),
        "throws" => (
            v["entry"].as_str().unwrap_or("").into(),
            Some(Arc::new(|_: &Request<'_>| Err(Cause::Unreadable))),
        ),
        "filesystem" => {
            let base = write_tree(name, &v["tree"]);
            let r = base.join(v["root"].as_str().unwrap_or("."));
            let entry =
                std::fs::read_to_string(base.join(v["entryPath"].as_str().unwrap_or(""))).unwrap();
            tmp = Some(base);
            root = Some(r.clone());
            (entry, Some(fs::resolver(&r).unwrap()))
        }
        kind => (
            v["entry"].as_str().unwrap_or("").into(),
            Some(virtual_resolver(&v["files"], kind == "virtual-ids")),
        ),
    };
    let mut opts = Options::default();
    if let Some(resolver) = resolver {
        let mut inc = Includes::new(resolver);
        let o = &v["options"];
        if let Some(d) = o["depth"].as_u64() {
            inc.depth = usize::try_from(d).unwrap_or(0);
        }
        if let Some(b) = o["bytes"].as_u64() {
            inc.bytes = usize::try_from(b).unwrap_or(0);
        }
        inc.file = o["file"].as_str().map(str::to_string);
        if let (Some(base), Some(r)) = (&tmp, &root) {
            let entry = base.join(v["entryPath"].as_str().unwrap_or(""));
            inc.file = entry
                .strip_prefix(r)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"));
        }
        opts.includes = Some(inc);
    }
    Setup {
        entry,
        opts,
        tmp,
        root,
    }
}

/// C2-N-084: a read node by its file, an unread one by the place its path
/// names from its including file, or by its path where that is outside the root.
fn dependency_ids(deps: &[carve2::include::Dependency], fs_root: bool) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for d in deps {
        let (id, read) = match &d.file {
            Some(f) => (f.clone(), true),
            None => (place(&d.path, d.from.as_deref(), fs_root), false),
        };
        let entry = json!({ "id": id, "read": read });
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    out
}

fn place(path: &str, from: Option<&str>, fs_root: bool) -> String {
    if !fs_root || path.starts_with('/') {
        return path.to_string();
    }
    let dir = from
        .and_then(|f| Path::new(f).parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    for c in dir.join(path).components() {
        match c {
            std::path::Component::ParentDir => {
                if parts.pop().is_none() {
                    return path.to_string();
                }
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    parts.join("/")
}

fn warnings(out: &carve2::Rendered) -> Vec<String> {
    let mut w: Vec<String> = out
        .diagnostics
        .iter()
        .map(|d| match &d.file {
            Some(f) => format!("{} {f}", d.code),
            None => d.code.to_string(),
        })
        .collect();
    w.sort();
    w
}

fn expected_warnings(v: &Value) -> Vec<String> {
    let mut w: Vec<String> = v["expected"]["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|x| {
            let code = x["code"].as_str().unwrap_or("");
            match x["file"].as_str() {
                Some(f) => format!("{code} {f}"),
                None => code.to_string(),
            }
        })
        .collect();
    w.sort();
    w
}

fn positions(tree: &Value, types: &[&str], out: &mut Vec<Value>) {
    match tree {
        Value::Object(m) => {
            let ty = m.get("type").and_then(Value::as_str).unwrap_or("");
            if let Some(pos) = m.get("pos").filter(|p| p.get("file").is_some()) {
                if types.contains(&ty) {
                    out.push(json!({ "type": ty, "pos": pos }));
                }
            }
            for (k, c) in m {
                if k != "pos" {
                    positions(c, types, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|c| positions(c, types, out)),
        _ => {}
    }
}

fn check(v: &Value) -> Vec<String> {
    let s = setup(v);
    let tmp = s
        .tmp
        .as_ref()
        .map(|t| t.display().to_string())
        .unwrap_or_default();
    let out = carve2::render_with_diagnostics(&s.entry, &s.opts);
    let mut fails = Vec::new();
    let html = v["expected"]["html"]
        .as_str()
        .unwrap_or("")
        .replace("<TMP>", &tmp);
    if out.html != html {
        fails.push(format!("html\n--- expected\n{html}\n--- got\n{}", out.html));
    }
    let (want, got) = (expected_warnings(v), warnings(&out));
    if want != got {
        fails.push(format!("warnings: expected {want:?}, got {got:?}"));
    }
    let deps = dependency_ids(&carve2::dependencies(&s.entry, &s.opts), s.root.is_some());
    let want: Vec<Value> = v["expected"]["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| json!({ "id": d["id"].as_str().unwrap_or("").replace("<TMP>", &tmp), "read": d["read"] }))
        .collect();
    if deps != want {
        fails.push(format!("dependencies: expected {want:?}, got {deps:?}"));
    }
    if let Some(expected) = v["expected"]["positions"].as_array() {
        let mut types: Vec<&str> = expected.iter().filter_map(|p| p["type"].as_str()).collect();
        types.dedup();
        let mut got = Vec::new();
        positions(
            &common::parse_json(&carve2::tree_json(&s.entry, &s.opts)),
            &types,
            &mut got,
        );
        if common::sorted(&Value::Array(got.clone()))
            != common::sorted(&Value::Array(expected.clone()))
        {
            fails.push(format!("positions: expected {expected:?}, got {got:?}"));
        }
    }
    for bad in v["forbiddenSubstrings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let messages: String = out.diagnostics.iter().map(|d| d.message.as_str()).collect();
        if out.html.contains(bad) || messages.contains(bad) {
            fails.push(format!("leaks {bad}"));
        }
    }
    if let Some(t) = s.tmp {
        let _ = std::fs::remove_dir_all(t);
    }
    fails
}

#[test]
fn c2_n_include_vectors_of_the_reference_engine() {
    let all = vectors();
    assert_eq!(all.len(), 187, "vectors not read");
    let mut failures = Vec::new();
    for v in &all {
        let name = v["name"].as_str().unwrap_or("");
        let fails = check(v);
        let held = HELD.iter().any(|(h, _)| *h == name);
        match (fails.is_empty(), held) {
            (true, true) => failures.push(format!("=== {name}: held but passes")),
            (false, false) => failures.push(format!("=== {name}\n{}", fails.join("\n"))),
            _ => {}
        }
    }
    println!(
        "include vectors: {} of {} pass",
        all.len() - failures.len(),
        all.len()
    );
    if let Ok(path) = std::env::var("VECTOR_FAILURES") {
        let _ = std::fs::write(path, failures.join("\n"));
    }
    assert!(failures.is_empty(), "{} failures", failures.len());
}
