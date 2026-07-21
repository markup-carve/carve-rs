//! Cross-engine include-conformance suite (spec PART 9 §19, rules I1–I14).
//!
//! This is the carve-rs half of the shared golden-vector corpus that lives in
//! the spec repo at `tests/spec/tests/include-conformance/`. The vectors and
//! their four goldens (`html`, `fmt`, `warnings`, `dependencies`) are generated
//! from carve-js, so a green run here asserts **rs == js** on every §19 rule,
//! permanently and completely - the machine version of the by-hand parity check
//! that first established it.
//!
//! Each vector is driven through rs's REAL code paths - `parse`,
//! `expand_includes`, `render_html`, `render_carve`, and `FileSystemResolver` -
//! never a reimplementation, so the suite exercises the engine a host actually
//! calls. Only the thin per-engine driver below (resolver construction, tmp-tree
//! materialization, and the normalization contract from the suite README) is
//! reproduced here; the contract itself is the portable part shared with js/php.
//!
//! ## Normalization contract (mirrors `include-conformance/README.md`)
//!
//! - **warnings** → ordered `{ rule, file? }`. `message`, `detail` and source
//!   offsets are deliberately dropped: message/detail are host-worded and I7
//!   forbids surfacing the raw resolver error; offsets are not a §19 contract
//!   and are the field most likely to diverge. Attribution travels through
//!   `file`, which is stable.
//! - **dependencies** → `{ id, resolved }` in first-encounter order (I11).
//! - **filesystem paths** → the whole materialized tree base folds to the
//!   `<TMP>` sentinel, so both in-root and out-of-root targets are stable.
//! - **I7 no-leak** → `forbiddenSubstrings` is checked against the RAW warning
//!   messages, so a processor that echoed an absolute path fails regardless of
//!   wording.

use std::fs;
use std::path::{Path, PathBuf};

use carve::{
    expand_includes, parse, render_carve, render_html, FileSystemResolver, IncludeContext,
    IncludeOptions, IncludeResolved, IncludeResolver,
};

// The helper lives in a subdirectory so cargo does not compile it as its own
// top-level integration-test binary; `#[path]` points `mod json` at it.
#[path = "include_conformance/json.rs"]
mod json;
use json::Value;

/// Documented per-engine expected differences, the include-suite analogue of
/// the HTML corpus's KNOWN_GAPS. A vector listed here is allowed to diverge for
/// the stated reason and is reported, never silently normalized away. Empty
/// unless a genuine, pre-existing cross-engine difference surfaces.
///
/// A real rs bug against the ruled behavior is fixed in rs, not parked here; a
/// golden that bakes a carve-js bug is escalated (fixed in js + regenerated),
/// never edited locally.
const KNOWN_DIFFERENCES: &[(&str, &str)] = &[];

// ---------------------------------------------------------------------------
// Vector location
// ---------------------------------------------------------------------------

fn vectors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/include-conformance/vectors")
}

// ---------------------------------------------------------------------------
// Tmp tree for filesystem-mode vectors
// ---------------------------------------------------------------------------

struct TmpTree(PathBuf);

impl TmpTree {
    fn new() -> Self {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "carve-ic-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&base).expect("tmp base");
        Self(base)
    }

    fn base(&self) -> &Path {
        &self.0
    }
}

impl Drop for TmpTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Materialize a filesystem vector's `tree` under a fresh tmp base. A string
/// value is file content; a `{ "symlink": target }` object is a symlink whose
/// target is resolved relative to the tree base (kept machine-independent).
/// Files first, links second, exactly like the reference driver.
fn materialize_tree(base: &Path, tree: &Value) {
    let mut links: Vec<(PathBuf, String)> = Vec::new();
    for (rel, val) in tree.entries() {
        let abs = base.join(rel);
        if let Some(target) = val.get("symlink").and_then(Value::as_str) {
            links.push((abs, target.to_string()));
        } else if let Some(content) = val.as_str() {
            if let Some(parent) = abs.parent() {
                fs::create_dir_all(parent).expect("tree parent");
            }
            fs::write(&abs, content).expect("tree file");
        } else {
            panic!("unexpected tree entry {rel}: {val:?}");
        }
    }
    for (abs, target) in links {
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).expect("link parent");
        }
        // Target is relative to the tree base, resolved to an absolute path
        // before linking - matching the reference driver's `resolve(base, t)`.
        std::os::unix::fs::symlink(base.join(&target), &abs).expect("symlink");
    }
}

// ---------------------------------------------------------------------------
// Path folding: <TMP> sentinel for filesystem paths
// ---------------------------------------------------------------------------

/// Fold an absolute path under the canonical tree base to `<TMP>/…`. Values that
/// are not under the base (a directive path as written, e.g. `../secret.crv`)
/// are returned unchanged. Linux keeps `/` as the separator throughout.
fn fold_path(value: &str, base_real: Option<&str>) -> String {
    let Some(base) = base_real else {
        return value.to_string();
    };
    if value == base {
        return "<TMP>".to_string();
    }
    let prefix = format!("{base}/");
    if let Some(rest) = value.strip_prefix(&prefix) {
        return format!("<TMP>/{rest}");
    }
    value.to_string()
}

/// Fold any occurrence of the tree base embedded inside a larger string (the
/// html and fmt of the absolute-path filesystem vector embed a real path).
fn fold_text(text: &str, base_real: Option<&str>) -> String {
    match base_real {
        Some(base) => text.replace(base, "<TMP>"),
        None => text.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Normalized result
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct NormWarning {
    rule: String,
    file: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct NormDep {
    id: String,
    resolved: bool,
}

struct RunResult {
    html: String,
    fmt: String,
    warnings: Vec<NormWarning>,
    dependencies: Vec<NormDep>,
    raw_messages: Vec<String>,
    /// Present only for `checkFmtExpandEquivalence` vectors.
    formatted_run: Option<(String, Vec<NormDep>)>,
}

// ---------------------------------------------------------------------------
// Virtual resolver
// ---------------------------------------------------------------------------

/// Build the virtual-mode resolver a vector describes.
///
/// - `resolverIds`: strip a leading `./` and return a canonical id, so two
///   spellings of one file collapse (I6/I11 identity).
/// - `resolverThrows`: carve-js's resolver THROWS here; rs has no error channel
///   (`resolve` returns `Option`), so the faithful rs behavior is to report the
///   target unresolvable - `None`. That produces the same normalized output
///   (`include-unresolved` + an unresolved dependency) with a message rs worded
///   itself, so the I7 `forbiddenSubstrings` guard is satisfied by construction:
///   rs never touches the raw error at all.
fn make_virtual_resolver(
    files: Value,
    resolver_ids: bool,
    throws: bool,
) -> impl Fn(&str, &IncludeContext<'_>) -> Option<IncludeResolved> {
    move |path: &str, _ctx: &IncludeContext<'_>| {
        if throws {
            return None;
        }
        if resolver_ids {
            let id = path.strip_prefix("./").unwrap_or(path);
            return files
                .get(id)
                .and_then(Value::as_str)
                .map(|src| IncludeResolved::with_id(src.to_string(), id.to_string()));
        }
        files
            .get(path)
            .and_then(Value::as_str)
            .map(|src| IncludeResolved::from(src.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

fn base_options<'a>(opts: &Value, resolver: &'a dyn IncludeResolver) -> IncludeOptions<'a> {
    let mut io = IncludeOptions::new().with_resolver(resolver);
    if let Some(sp) = opts.get("sourcePath").and_then(Value::as_str) {
        io = io.with_source_path(sp.to_string());
    }
    if let Some(d) = opts.get("maxDepth").and_then(Value::as_u64) {
        io = io.with_max_depth(d as usize);
    }
    if let Some(b) = opts.get("maxBytes").and_then(Value::as_u64) {
        io = io.with_max_bytes(b as usize);
    }
    io
}

// ---------------------------------------------------------------------------
// Running one vector
// ---------------------------------------------------------------------------

fn norm_warnings(result: &carve::IncludeResult, base_real: Option<&str>) -> Vec<NormWarning> {
    result
        .warnings
        .iter()
        .map(|w| NormWarning {
            rule: w.rule.clone(),
            file: w.file.as_deref().map(|f| fold_path(f, base_real)),
        })
        .collect()
}

fn norm_deps(result: &carve::IncludeResult, base_real: Option<&str>) -> Vec<NormDep> {
    result
        .dependencies
        .iter()
        .map(|d| NormDep {
            id: fold_path(&d.id, base_real),
            resolved: d.resolved,
        })
        .collect()
}

fn run_vector(vector: &Value) -> RunResult {
    let mode = vector["mode"].as_str().expect("mode");
    let resolver_kind = vector["resolver"].as_str().expect("resolver");
    let opts = vector.get("options").cloned().unwrap_or(Value::Null);

    if mode == "filesystem" {
        let tree = vector["tree"].clone();
        assert!(tree.is_object(), "filesystem vector needs an object `tree`");
        let tmp = TmpTree::new();
        materialize_tree(tmp.base(), &tree);
        let base_real = fs::canonicalize(tmp.base()).expect("canonical base");
        let base_real_str = base_real.to_string_lossy().into_owned();

        let root_rel = vector.get("root").and_then(Value::as_str).unwrap_or(".");
        let root_real = fs::canonicalize(tmp.base().join(root_rel)).expect("canonical root");

        // Read the entry and bind any `<ABS:rel>` sentinel to the real tree
        // location, so the absolute-containment case (I10) needs no
        // machine-specific literal in the committed vector.
        let entry_path = vector["entryPath"].as_str().expect("entryPath");
        let raw_entry = fs::read_to_string(tmp.base().join(entry_path)).expect("read entry");
        let entry = bind_abs_sentinels(&raw_entry, &base_real);

        let allow_absolute = opts
            .get("allowAbsolute")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let resolver = FileSystemResolver::new(&root_real)
            .expect("fs resolver")
            .allow_absolute(allow_absolute);

        let mut io = IncludeOptions::new().with_resolver(&resolver);
        // sourcePath defaults to the real entry path (or the explicit `<ENTRY>`
        // request); a literal override is passed through verbatim.
        let source_path = match opts.get("sourcePath").and_then(Value::as_str) {
            Some("<ENTRY>") | None => fs::canonicalize(tmp.base().join(entry_path))
                .expect("canonical entry")
                .to_string_lossy()
                .into_owned(),
            Some(other) => other.to_string(),
        };
        io = io.with_source_path(source_path);
        if let Some(d) = opts.get("maxDepth").and_then(Value::as_u64) {
            io = io.with_max_depth(d as usize);
        }
        if let Some(b) = opts.get("maxBytes").and_then(Value::as_u64) {
            io = io.with_max_bytes(b as usize);
        }

        let doc = parse(&entry);
        let result = expand_includes(doc, &entry, &io);
        let html = fold_text(&render_html(&result.doc), Some(&base_real_str));
        let fmt = fold_text(&render_carve(&parse(&entry)), Some(&base_real_str));

        return RunResult {
            html,
            fmt,
            warnings: norm_warnings(&result, Some(&base_real_str)),
            dependencies: norm_deps(&result, Some(&base_real_str)),
            raw_messages: result.warnings.iter().map(|w| w.message.clone()).collect(),
            formatted_run: None, // filesystem vectors never set the equivalence flag
        };
        // `tmp` drops here, removing the tree.
    }

    // Virtual / none mode.
    let entry = vector["entry"].as_str().expect("entry").to_string();
    let fmt = render_carve(&parse(&entry));

    if resolver_kind == "none" {
        // No resolver configured (I3): every directive stays literal.
        let io = IncludeOptions::new();
        let result = expand_includes(parse(&entry), &entry, &io);
        return RunResult {
            html: render_html(&result.doc),
            fmt,
            warnings: norm_warnings(&result, None),
            dependencies: norm_deps(&result, None),
            raw_messages: result.warnings.iter().map(|w| w.message.clone()).collect(),
            formatted_run: None,
        };
    }

    let files = vector
        .get("files")
        .cloned()
        .unwrap_or(Value::Obj(Vec::new()));
    let resolver_ids = opts
        .get("resolverIds")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let throws = opts.get("resolverThrows").is_some();
    let resolver = make_virtual_resolver(files, resolver_ids, throws);
    let io = base_options(&opts, &resolver);

    let result = expand_includes(parse(&entry), &entry, &io);
    let html = render_html(&result.doc);

    let formatted_run = if vector
        .get("checkFmtExpandEquivalence")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        // Expanding the FORMATTED entry must yield the same html + dependency
        // set as expanding the original (I12 stronger invariant).
        let fres = expand_includes(parse(&fmt), &fmt, &io);
        Some((render_html(&fres.doc), norm_deps(&fres, None)))
    } else {
        None
    };

    RunResult {
        html,
        fmt,
        warnings: norm_warnings(&result, None),
        dependencies: norm_deps(&result, None),
        raw_messages: result.warnings.iter().map(|w| w.message.clone()).collect(),
        formatted_run,
    }
}

/// Replace every `<ABS:rel>` sentinel with the canonical absolute path of the
/// tree file `rel`, mirroring the reference driver.
fn bind_abs_sentinels(entry: &str, base_real: &Path) -> String {
    let mut out = String::with_capacity(entry.len());
    let mut rest = entry;
    while let Some(open) = rest.find("<ABS:") {
        out.push_str(&rest[..open]);
        let after = &rest[open + "<ABS:".len()..];
        let close = after.find('>').expect("unterminated <ABS:> sentinel");
        let rel = &after[..close];
        out.push_str(&base_real.join(rel).to_string_lossy());
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Expected-golden extraction + comparison
// ---------------------------------------------------------------------------

fn expected_warnings(expected: &Value) -> Vec<NormWarning> {
    expected["warnings"]
        .as_array()
        .expect("expected.warnings")
        .iter()
        .map(|w| NormWarning {
            rule: w["rule"].as_str().expect("rule").to_string(),
            file: w.get("file").and_then(Value::as_str).map(str::to_string),
        })
        .collect()
}

fn expected_deps(expected: &Value) -> Vec<NormDep> {
    expected["dependencies"]
        .as_array()
        .expect("expected.dependencies")
        .iter()
        .map(|d| NormDep {
            id: d["id"].as_str().expect("id").to_string(),
            resolved: d["resolved"].as_bool().expect("resolved"),
        })
        .collect()
}

/// Compare one vector's actual run against its goldens, returning a list of
/// human-readable field mismatches (empty when the vector fully passes).
fn compare(name: &str, vector: &Value, run: &RunResult) -> Vec<String> {
    let expected = &vector["expected"];
    let mut diffs = Vec::new();

    let exp_html = expected["html"].as_str().expect("expected.html");
    if run.html != exp_html {
        diffs.push(format!(
            "html:\n    expected {exp_html:?}\n    actual   {:?}",
            run.html
        ));
    }
    let exp_fmt = expected["fmt"].as_str().expect("expected.fmt");
    if run.fmt != exp_fmt {
        diffs.push(format!(
            "fmt:\n    expected {exp_fmt:?}\n    actual   {:?}",
            run.fmt
        ));
    }
    let exp_warnings = expected_warnings(expected);
    if run.warnings != exp_warnings {
        diffs.push(format!(
            "warnings:\n    expected {exp_warnings:?}\n    actual   {:?}",
            run.warnings
        ));
    }
    let exp_deps = expected_deps(expected);
    if run.dependencies != exp_deps {
        diffs.push(format!(
            "dependencies:\n    expected {exp_deps:?}\n    actual   {:?}",
            run.dependencies
        ));
    }

    // I7 no-leak: no forbidden substring may appear in any RAW warning message.
    if let Some(forbidden) = vector.get("forbiddenSubstrings").and_then(Value::as_array) {
        for f in forbidden {
            let needle = f.as_str().expect("forbidden substring");
            for msg in &run.raw_messages {
                if msg.contains(needle) {
                    diffs.push(format!(
                        "forbiddenSubstrings: message leaked {needle:?}: {msg:?}"
                    ));
                }
            }
        }
    }

    // I12 stronger invariant: expanding the formatted entry matches the original.
    if let Some((fhtml, fdeps)) = &run.formatted_run {
        if *fhtml != run.html {
            diffs.push(format!(
                "checkFmtExpandEquivalence html:\n    original  {:?}\n    formatted {fhtml:?}",
                run.html
            ));
        }
        if *fdeps != run.dependencies {
            diffs.push(format!(
                "checkFmtExpandEquivalence dependencies:\n    original  {:?}\n    formatted {fdeps:?}",
                run.dependencies
            ));
        }
    }

    if !diffs.is_empty() {
        let _ = name; // name is reported by the caller
    }
    diffs
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

#[test]
fn include_conformance_vectors_match_carve_js_goldens() {
    let dir = vectors_dir();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {dir:?}: {e}"))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    entries.sort();

    assert!(
        !entries.is_empty(),
        "no include-conformance vectors found under {dir:?} - did the tests/spec submodule init?",
    );

    let known: std::collections::HashMap<&str, &str> = KNOWN_DIFFERENCES.iter().copied().collect();

    let mut passed = 0usize;
    let mut failures: Vec<(String, Vec<String>)> = Vec::new();
    let mut documented: Vec<(String, String)> = Vec::new();

    for path in &entries {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let raw = fs::read_to_string(path).expect("read vector");
        let vector = Value::parse(&raw).unwrap_or_else(|e| panic!("{name}: {e}"));
        let run = run_vector(&vector);
        let diffs = compare(&name, &vector, &run);

        if diffs.is_empty() {
            passed += 1;
        } else if let Some(reason) = known.get(name.as_str()) {
            // A documented, expected cross-engine difference: reported, not failed.
            documented.push((name.clone(), (*reason).to_string()));
        } else {
            failures.push((name.clone(), diffs));
        }
    }

    eprintln!(
        "include-conformance: {passed}/{} vectors match carve-js goldens ({} documented difference(s), {} failure(s))",
        entries.len(),
        documented.len(),
        failures.len(),
    );
    for (name, reason) in &documented {
        eprintln!("  documented difference: {name} - {reason}");
    }

    if !failures.is_empty() {
        let mut report = String::new();
        for (name, diffs) in &failures {
            report.push_str(&format!("\n=== {name} ===\n"));
            for d in diffs {
                report.push_str("  ");
                report.push_str(d);
                report.push('\n');
            }
        }
        panic!(
            "{} include-conformance vector(s) diverged from the carve-js goldens:{}",
            failures.len(),
            report
        );
    }
}
