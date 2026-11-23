//! The Carve writer (`writer.md`) against the shared corpus, through
//! `carve::api`: the `write` and `format` clause cases, the `write` examples,
//! the writer trees and the gates C2-W-002 and C2-W-003 on the render examples.
//! A case this engine cannot pass is held below with its reason; a held case
//! that passes fails the run, so the hold goes when the writer is fixed.

use carve::api::{
    self, Carve, EncodeOptions, Entry, Error, Options, Profile, WriteOptions, Written,
};
use serde_json::Value;

/// `(entry id, reason)`.
const HELD: &[(&str, &str)] = &[];

fn load(path: &str) -> Value {
    let text = std::fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_default();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap_or(Value::Null)
}

fn options(v: &Value) -> WriteOptions {
    let mut o = WriteOptions {
        align_tables: v["alignTables"] == true,
        normalize: v["normalize"] == true,
        ..WriteOptions::default()
    };
    o.extensions = v["extensions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if v["profile"] == "comment" {
        o.render = Options {
            profile: Profile::Comment,
            ..Options::default()
        };
    }
    o
}

struct Group {
    name: &'static str,
    pass: usize,
    held: usize,
    failures: Vec<String>,
}

impl Group {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            pass: 0,
            held: 0,
            failures: Vec::new(),
        }
    }

    fn check(&mut self, id: &str, problems: &[String]) {
        let held = HELD.iter().any(|(h, _)| *h == id);
        match (problems.is_empty(), held) {
            (true, false) => self.pass += 1,
            (false, true) => self.held += 1,
            (true, true) => self
                .failures
                .push(format!("=== {id}: held but passes; drop the hold")),
            (false, false) => self
                .failures
                .push(format!("=== {id}\n{}", problems.join("\n"))),
        }
    }

    fn finish(self) -> Vec<String> {
        println!(
            "{}: {} pass, {} held, {} fail",
            self.name,
            self.pass,
            self.held,
            self.failures.len()
        );
        self.failures
    }
}

fn entry_agrees(want: &Value, got: &Entry) -> bool {
    let code = want.as_str().or_else(|| want["code"].as_str());
    code == Some(got.code.as_str())
        && want["channel"].as_str().is_none_or(|v| v == got.channel)
        && want["severity"].as_str().is_none_or(|v| v == got.severity)
        && want["path"]
            .as_str()
            .is_none_or(|v| Some(v) == got.path.as_deref())
}

/// The record checks of `expected` on `got`.
fn record(expected: &Value, got: &[Entry], out: &mut Vec<String>) {
    for d in expected["diagnostics"].as_array().into_iter().flatten() {
        if !got.iter().any(|g| entry_agrees(d, g)) {
            out.push(format!("missing {d}; reported {:?}", codes(got)));
        }
    }
    for code in expected["notReported"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if got.iter().any(|g| g.code == code) {
            out.push(format!("unexpected {code}; reported {:?}", codes(got)));
        }
    }
    if expected["diagnosticsComplete"] == true {
        let wanted = expected["diagnostics"].as_array().map_or(0, Vec::len);
        if got.len() != wanted {
            out.push(format!(
                "expected {wanted} entries; reported {:?}",
                codes(got)
            ));
        }
    }
}

fn codes(got: &[Entry]) -> Vec<String> {
    got.iter()
        .map(|e| format!("{} {}", e.code, e.path.as_deref().unwrap_or("")))
        .collect()
}

fn refusal(expected: &Value, result: &Result<String, Error>, out: &mut Vec<String>) {
    let want = &expected["refusal"];
    let ok = match (want["error"].as_str(), result) {
        (Some("CarveWriteError"), Err(Error::Write { .. })) => true,
        (Some("CarveOptionError"), Err(Error::Option { option, .. })) => want["path"]
            .as_str()
            .is_none_or(|p| p.trim_start_matches('/') == option),
        _ => false,
    };
    if !ok {
        out.push(format!("expected refusal {want}; got {result:?}"));
    }
}

fn boundary_refusal(case: &Value, input: &str, out: &mut Vec<String>) {
    let request =
        serde_json::json!({"call": "format", "source": input, "options": case["options"]});
    let reply: Value =
        serde_json::from_str(&api::json::call(&request.to_string())).unwrap_or_default();
    let want = &case["expected"]["refusal"];
    let path = want["path"].as_str().unwrap_or("").trim_start_matches('/');
    if reply["error"]["name"] != want["error"]
        || !reply["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains(path))
    {
        out.push(format!("expected refusal {want}; got {reply}"));
    }
}

fn written(expected: &Value, result: &Result<Written, Error>, out: &mut Vec<String>) {
    match result {
        Ok(w) => {
            if let Some(carve) = expected["carve"].as_str() {
                if w.output != carve {
                    out.push(format!("--- expected\n{carve}--- got\n{}", w.output));
                }
            }
            record(expected, &w.diagnostics, out);
        }
        Err(e) => out.push(format!("refused: {e}")),
    }
}

/// The tree of `src` as compared by C2-A-161: no positions, no source length.
fn bare_tree(src: &str) -> Value {
    let tree = api::parse(src, &Options::default()).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    let mut v: Value = serde_json::from_str(&api::encode_tree_with(
        &tree,
        EncodeOptions { positions: false },
    ))
    .unwrap_or_default();
    strip(&mut v);
    v
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("segments");
            m.remove("srcByteLength");
            m.values_mut().for_each(strip);
        }
        Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

#[test]
fn delimited_table_body_keys_round_trip() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for input in [
        // C2-L-036: a table without rows stays delimited, whatever its bodies.
        "{table body-rows=0,0}\n```csv\n```\n",
        // C2-L-057: `body-header-rows` declares no `header-rows`.
        "{table body-rows=1 body-header-rows=1}\n```csv\n```\n",
        "{table x-header-rows=0}\n```csv\na\nb\n```\n",
        "{table header-rows=x}\n```csv\na\nb\n```\n",
        "{table header-rows=1}\n```csv\na\nb\n```\n",
        // C2-W-066, C2-B-227: bodies and a foot the attributes spell keep the delimited form.
        "{table body-rows=1,1}\n```csv\nh\na\n b\n```\n",
        "{table body-rows=1,1 body-header-rows=0,1}\n```csv\nh\n\"a\nb\"\nc\nd\n```\n",
        "{table footer-rows=1}\n```csv\nh\n a\nc\n```\n",
        // C2-B-226: an empty body after the last row leaves its lone empty cell quoted.
        "{table body-rows=0}\n```csv\n\"\"\n```\n",
    ] {
        assert_eq!(carve.format(input, &opts).unwrap(), input);
    }
}

#[test]
fn include_code_mode_writes_before_selection_and_transform_options() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    let expected = "{{ parts/plans.csv @code:csv @rows:3-4 @cols:\"Plan,Price\" }}\n";
    for input in [
        "{{ parts/plans.csv @rows:3-4 @cols:\"Plan,Price\" @code:csv }}\n",
        expected,
    ] {
        assert_eq!(carve.format(input, &opts).unwrap(), expected);
        let tree = api::parse(input, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &opts).unwrap(), expected);
    }
}

#[test]
fn writer_clause_cases() {
    let carve = Carve::new();
    let mut failures = Vec::new();

    let cases = load("spec-conformance/clause-cases.json");
    let mut writes = Group::new("write cases");
    let mut formats = Group::new("format cases");
    for c in cases["cases"].as_array().into_iter().flatten() {
        let id = c["id"].as_str().unwrap_or("");
        let expected = &c["expected"];
        let mut opts = options(&c["options"]);
        let mut problems = Vec::new();
        match c["kind"].as_str() {
            Some("write") => {
                opts.layout = c.get("layout").cloned();
                let tree = api::decode_tree(&c["tree"].to_string());
                match tree {
                    Err(e) => problems.push(format!("tree refused: {e}")),
                    Ok(tree) => {
                        if expected.get("refusal").is_some() {
                            refusal(expected, &carve.write(&tree, &opts), &mut problems);
                            let with = carve.write_with_diagnostics(&tree, &opts);
                            if let Ok(w) = &with {
                                record(expected, &w.diagnostics, &mut problems);
                            }
                        } else {
                            written(
                                expected,
                                &carve.write_with_diagnostics(&tree, &opts),
                                &mut problems,
                            );
                        }
                    }
                }
                writes.check(id, &problems);
            }
            Some("format") => {
                let input = c["input"].as_str().unwrap_or("");
                if c["options"].get("headingShift").is_some() {
                    // C2-T-005: a render option has no typed format field; the JSON boundary refuses it.
                    boundary_refusal(c, input, &mut problems);
                } else if expected.get("refusal").is_some() {
                    refusal(expected, &carve.format(input, &opts), &mut problems);
                } else {
                    written(
                        expected,
                        &carve.format_with_diagnostics(input, &opts),
                        &mut problems,
                    );
                }
                formats.check(id, &problems);
            }
            _ => {}
        }
    }
    failures.extend(writes.finish());
    failures.extend(formats.finish());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn writer_examples() {
    let carve = Carve::new();
    let examples = load("spec-conformance/examples.json");
    let mut group = Group::new("write examples");
    for e in examples["examples"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["kind"] == "write")
    {
        let id = e["id"].as_str().unwrap_or("");
        let input = e["input"].as_str().unwrap_or("");
        let mut problems = Vec::new();
        written(
            &e["expected"],
            &carve.format_with_diagnostics(input, &options(&e["options"])),
            &mut problems,
        );
        if let Some(carve) = e["expected"]["carve"].as_str() {
            if bare_tree(carve) != bare_tree(input) {
                problems.push("the expected source does not read back as the input".into());
            }
        }
        group.check(id, &problems);
    }
    let failures = group.finish();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn writer_trees() {
    let carve = Carve::new();
    let trees = load("spec-trees/examples-writer.json");
    let mut group = Group::new("writer trees");
    for t in trees["trees"].as_array().into_iter().flatten() {
        let id = t["id"].as_str().unwrap_or("");
        let source = t["source"].as_str().unwrap_or("");
        let mut problems = Vec::new();
        match api::decode_tree(&t["tree"].to_string()) {
            Err(e) => problems.push(format!("tree refused: {e}")),
            Ok(tree) => match carve.write_with_diagnostics(&tree, &WriteOptions::default()) {
                Err(e) => problems.push(format!("refused: {e}")),
                // C2-W-002: with no loss the written source reads back as the tree.
                Ok(w) if w.diagnostics.is_empty() => {
                    if bare_tree(&w.output) != bare_tree(source) {
                        problems.push(format!("write does not read back:\n{}", w.output));
                    }
                }
                Ok(w) => problems.push(format!("losses {:?}", codes(&w.diagnostics))),
            },
        }
        group.check(id, &problems);
    }
    let failures = group.finish();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C2-W-002 and C2-W-003 on every render example: `parse(fmt(x)) == parse(x)`
/// and `fmt(fmt(x)) == fmt(x)` where the writer reports no loss (C2-W-131,
/// C2-W-133).
#[test]
fn format_round_trips_render_examples() {
    let carve = Carve::new();
    let examples = load("spec-conformance/examples.json");
    let mut group = Group::new("round-trip");
    let mut refused = 0;
    for e in examples["examples"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["kind"] == "render")
    {
        let id = e["id"].as_str().unwrap_or("");
        let input = e["input"].as_str().unwrap_or("");
        let mut problems = Vec::new();
        let result = carve.format_with_diagnostics(input, &WriteOptions::default());
        let lossy = result.as_ref().is_ok_and(|w| {
            w.diagnostics.iter().any(|d| {
                matches!(
                    d.code.as_str(),
                    "structure-unspellable" | "field-unspellable" | "writer-refused"
                )
            })
        });
        match result {
            Ok(_) if lossy => refused += 1,
            Err(err) => problems.push(format!("error {err}")),
            Ok(Written { output: once, .. }) => {
                if bare_tree(&once) != bare_tree(input) {
                    problems.push(format!(
                        "{input}\n--- formatted\n{once}--- reads back as another tree"
                    ));
                }
                match carve.format(once.as_str(), &WriteOptions::default()) {
                    Ok(twice) if twice == once => {}
                    other => {
                        problems.push(format!("{input}\n--- once\n{once}--- twice\n{other:?}"));
                    }
                }
            }
        }
        group.check(id, &problems);
    }
    println!("round-trip: {refused} refused with a loss report");
    let failures = group.finish();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
