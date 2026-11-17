//! The shared conformance corpus of the specification (`spec-conformance/`, a
//! copy of `spec/0.2/conformance/` at `spec-data/SOURCE_COMMIT`) as the gate of
//! this engine. Every entry of a kind the engine implements passes, or is held
//! below with the clause that shows the corpus wrong; a held entry must keep
//! failing, so the hold goes when the corpus is corrected.
mod common;

use carve2::include::{Cause, Includes, Request};
use carve2::options::{HeadingIds, Options, Profile, UrlPolicy};
use common::{parse_json, sorted};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// `(entry id, checked field)`: the corpus is wrong there, by the clause in
/// the lane's `_EXAMPLE_FIXES.md`.
const HELD: &[(&str, &str)] = &[];

/// Kinds the engine does not implement yet: the writer, lint and the importer.
const LATER: &[&str] = &["write", "lint", "import"];

fn load(name: &str) -> Value {
    let path = format!("{}/spec-conformance/{name}", env!("CARGO_MANIFEST_DIR"));
    parse_json(&std::fs::read_to_string(path).unwrap_or_default())
}

fn lint_channel() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/spec-data/lint-rules.json"
    ))
    .unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    v["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            let id = r["id"].as_str().unwrap_or("").to_string();
            (id, r["channel"].as_str().unwrap_or("").to_string())
        })
        .collect()
}

/// The options of an entry, by the names of `data/options.json`.
#[must_use]
pub fn options(v: &Value, files: Option<&Value>) -> Options {
    let mut o = Options::default();
    if let Some(h) = v["headingIds"].as_str().and_then(HeadingIds::from_name) {
        o.heading_ids = h;
    }
    if let Some(p) = v["profile"].as_str().and_then(Profile::from_name) {
        o.profile = p;
    }
    for (key, slot) in [
        ("sourceLines", &mut o.source_lines),
        ("roundtrip", &mut o.roundtrip),
        ("rawHtml", &mut o.raw_html),
        ("sections", &mut o.sections),
        ("smartTypography", &mut o.smart_typography),
        ("strictLosses", &mut o.strict_losses),
    ] {
        if let Some(b) = v[key].as_bool() {
            *slot = b;
        }
    }
    if let Some(s) = v["social"].as_object() {
        let url = |k: &str| s.get(k).and_then(Value::as_str).map(str::to_owned);
        o.social = Some(carve2::options::Social {
            mention_url: url("mentionUrl"),
            tag_url: url("tagUrl"),
        });
    }
    let list = |k: &str| {
        v["urlPolicy"][k].as_array().map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
    };
    if let Some(a) = list("allow") {
        o.url_policy = UrlPolicy::Allow(a);
    } else if let Some(d) = list("deny") {
        o.url_policy = UrlPolicy::Deny(d);
    }
    if let Some(files) = files {
        let mut inc = Includes::new(serving(files));
        if let Some(d) = v["includes"]["depth"].as_u64() {
            inc.depth = usize::try_from(d).unwrap_or(0);
        }
        o.includes = Some(inc);
    }
    o
}

/// includes.md, conventions: a resolver that serves exactly `files`, each
/// under its name, which is also its id.
#[must_use]
pub fn serving(files: &Value) -> carve2::include::Resolver {
    let mut map: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (name, f) in files.as_object().into_iter().flatten() {
        let bytes = if let Some(t) = f["text"].as_str() {
            t.as_bytes().to_vec()
        } else {
            let hex = f["bytes"].as_str().unwrap_or("").replace(' ', "");
            (0..hex.len() / 2)
                .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
                .collect()
        };
        map.insert(name.clone(), bytes);
    }
    Arc::new(move |r: &Request<'_>| {
        map.get(r.path)
            .map(|b| (b.clone(), r.path.to_string()))
            .ok_or(Cause::NotFound)
    })
}

fn without_pos(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("segments");
            m.values_mut().for_each(without_pos);
        }
        Value::Array(a) => a.iter_mut().for_each(without_pos),
        _ => {}
    }
}

struct Run {
    pass: usize,
    failures: Vec<String>,
    held_failing: Vec<(String, String)>,
}

impl Run {
    fn check(&mut self, id: &str, field: &str, ok: bool, detail: impl FnOnce() -> String) {
        let held = HELD.iter().any(|&(h, f)| h == id && f == field);
        match (ok, held) {
            (true, false) => self.pass += 1,
            (false, true) => self.held_failing.push((id.into(), field.into())),
            (true, true) => self
                .failures
                .push(format!("=== {id} {field}: held but passes; drop the hold")),
            (false, false) => self
                .failures
                .push(format!("=== {id} {field}\n{}", detail())),
        }
    }

    fn finish(self, name: &str) {
        println!(
            "{name}: {} checks pass, {} held, {} fail",
            self.pass,
            self.held_failing.len(),
            self.failures.len()
        );
        if let Ok(dir) = std::env::var("CORPUS_FAILURES") {
            let _ = std::fs::write(format!("{dir}/{name}.txt"), self.failures.join("\n"));
        }
        assert!(self.failures.is_empty(), "{} failures", self.failures.len());
    }
}

fn run() -> Run {
    Run {
        pass: 0,
        failures: Vec::new(),
        held_failing: Vec::new(),
    }
}

fn diagnostics(run: &mut Run, id: &str, want: &Value, got: &[String]) {
    let channels = lint_channel();
    for code in want
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if channels.get(code).map(String::as_str) == Some("lint") {
            continue;
        }
        run.check(
            id,
            &format!("diagnostics:{code}"),
            got.iter().any(|g| g == code),
            || format!("missing {code}; reported {got:?}"),
        );
    }
}

#[test]
fn c2_conformance_examples() {
    let corpus = load("examples.json");
    let mut run = run();
    let mut later = 0;
    for e in corpus["examples"].as_array().into_iter().flatten() {
        let id = e["id"].as_str().unwrap_or("");
        let kind = e["kind"].as_str().unwrap_or("");
        if LATER.contains(&kind) {
            later += 1;
            continue;
        }
        let files = (kind == "include").then(|| &e["files"]);
        let opts = options(&e["options"], files);
        let input = e["input"].as_str().unwrap_or("");
        let out = carve2::render_with_diagnostics(input, &opts);
        let expected = &e["expected"];
        if let Some(html) = expected["html"].as_str() {
            run.check(id, "html", out.html == html, || {
                format!("{input}\n--- expected\n{html}\n--- got\n{}", out.html)
            });
        }
        if let Some(piece) = expected["htmlIncludes"].as_str() {
            run.check(id, "htmlIncludes", out.html.contains(piece), || {
                format!(
                    "{input}\n--- expected a piece\n{piece}\n--- got\n{}",
                    out.html
                )
            });
        }
        if let Some(tree) = expected.get("tree") {
            let got = parse_json(&carve2::tree_json(input, &opts));
            let (want, got) = (sorted(tree), sorted(&got));
            run.check(id, "tree", want == got, || {
                format!("{input}\n--- expected\n{want}\n--- got\n{got}")
            });
        }
        if let Some(d) = expected.get("diagnostics") {
            diagnostics(&mut run, id, d, &out.codes());
        }
    }
    println!("later stage (write, lint, import): {later}");
    run.finish("examples");
}

#[test]
fn c2_conformance_clause_cases() {
    let corpus = load("clause-cases.json");
    let mut run = run();
    for c in corpus["cases"].as_array().into_iter().flatten() {
        let id = c["id"].as_str().unwrap_or("");
        let input = c["input"].as_str().unwrap_or("");
        let opts = options(&c["options"], None);
        if let Some(html) = c["expected"]["html"].as_str() {
            let got = carve2::render(input, &opts);
            run.check(id, "html", got == html, || {
                format!("{input}\n--- expected\n{html}\n--- got\n{got}")
            });
        }
        if let Some(children) = c["expected"].get("children") {
            let mut got = parse_json(&carve2::tree_json(input, &opts));
            without_pos(&mut got);
            let (want, got) = (sorted(children), sorted(&got["children"]));
            run.check(id, "children", want == got, || {
                format!("{input}\n--- expected\n{want}\n--- got\n{got}")
            });
        }
    }
    run.finish("clause-cases");
}

/// The fixtures of `spec/0.2/trees/` (copied to `spec-trees/`), as the tree
/// gate of `ast.md` reads them: each source gives its tree with positions.
#[test]
fn c2_a_tree_gate_fixtures() {
    let mut run = run();
    for name in [
        "examples-syntax-blocks",
        "examples-syntax-inline",
        "examples-html",
        "examples-writer",
        "examples-includes",
    ] {
        let path = format!("{}/spec-trees/{name}.json", env!("CARGO_MANIFEST_DIR"));
        let v = parse_json(&std::fs::read_to_string(path).unwrap_or_default());
        for t in v["trees"].as_array().into_iter().flatten() {
            let id = t["id"].as_str().unwrap_or("");
            let files = t.get("files").map(|f| {
                let wrapped: serde_json::Map<String, Value> = f
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| (k.clone(), serde_json::json!({ "text": v })))
                    .collect();
                Value::Object(wrapped)
            });
            let files =
                files.or_else(|| (name == "examples-includes").then(|| serde_json::json!({})));
            let opts = options(&t["options"], files.as_ref());
            let input = t["source"].as_str().unwrap_or("");
            let got = parse_json(&carve2::tree_json(input, &opts));
            let (want, got) = (sorted(&t["tree"]), sorted(&got));
            run.check(id, "tree", want == got, || {
                format!("{input}\n--- expected\n{want}\n--- got\n{got}")
            });
        }
    }
    run.finish("trees");
}

/// C2-L-023: blocks Examples 267 and 268 have equal trees (C2-A-162).
#[test]
fn c2_l_023_examples_267_and_268_have_equal_trees() {
    let tree = |s: &str| {
        let mut v = parse_json(&carve2::tree_json(s, &Options::default()));
        without_pos(&mut v);
        v.as_object_mut().map(|m| m.remove("srcByteLength"));
        sorted(&v)
    };
    assert_eq!(
        tree("| a | b |\n| 1 | 2 |"),
        tree("{table header-rows=0}\n```csv\na,b\n1,2\n```")
    );
}
