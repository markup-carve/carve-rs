//! The HTML table import cases of carve#6812 G1 (carve#6663, #6664, #6623,
//! carve-js#5574): the written source and the complete record, which
//! `import_cases.rs` does not compare.
mod common;

use carve::api::{self, ImportOptions};
use common::parse_json;
use serde_json::Value;

const CASES: &[u32] = &[
    2063, 2100, 2101, 2102, 2103, 2104, 2244, 2245, 2250, 2252, 2253, 2254, 2255, 2256, 2257,
];

fn line_column(input: &str, offset: usize) -> (u64, u64) {
    let before = &input[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line as u64, column as u64)
}

fn agrees(want: &Value, got: &api::Entry, html: &str) -> bool {
    let at = got.range.map(|r| line_column(html, r.start_offset));
    want["code"].as_str() == Some(got.code.as_str())
        && want["channel"].as_str().is_none_or(|v| v == got.channel)
        && want["severity"].as_str().is_none_or(|v| v == got.severity)
        && want["path"]
            .as_str()
            .is_none_or(|v| got.path.as_deref() == Some(v))
        && want["line"]
            .as_u64()
            .is_none_or(|v| at.is_some_and(|a| a.0 == v))
        && want["column"]
            .as_u64()
            .is_none_or(|v| at.is_some_and(|a| a.1 == v))
}

#[test]
fn g1_table_import_cases_write_and_record() {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let corpus = parse_json(&std::fs::read_to_string(path).unwrap_or_default());
    let mut seen = 0;
    let mut failures = Vec::new();
    for c in corpus["cases"].as_array().into_iter().flatten() {
        let id = c["id"].as_str().unwrap_or("");
        let n: u32 = id
            .strip_prefix("CC-")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if !CASES.contains(&n) {
            continue;
        }
        seen += 1;
        let html = c["html"].as_str().unwrap_or("");
        let expected = &c["expected"];
        let got = match api::import_document(html, &ImportOptions::default()) {
            Ok(g) => g,
            Err(e) => {
                failures.push(format!("{id}: refused: {e}"));
                continue;
            }
        };
        if expected["carve"].as_str() != Some(got.source.as_str()) {
            failures.push(format!(
                "{id}: source\n--- expected\n{}--- got\n{}",
                expected["carve"].as_str().unwrap_or(""),
                got.source
            ));
        }
        let want = expected["diagnostics"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let complete = want.len() == got.diagnostics.len()
            && want
                .iter()
                .all(|w| got.diagnostics.iter().any(|g| agrees(w, g, html)));
        if !complete {
            failures.push(format!(
                "{id}: record {:?}, expected {want:?}",
                got.diagnostics
                    .iter()
                    .map(|d| (d.code.as_str(), d.path.as_deref(), d.range))
                    .collect::<Vec<_>>()
            ));
        }
    }
    assert_eq!(seen, CASES.len(), "clause cases missing from the corpus");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// CC-2237 to CC-2240, CC-2246 and CC-2248 with the final line feed their
/// expected HTML carries (carve#6837) taken off.
#[test]
fn g1_table_render_cases() {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let corpus = parse_json(&std::fs::read_to_string(path).unwrap_or_default());
    let ids = [2237, 2238, 2239, 2240, 2246, 2248].map(|n| format!("CC-{n}"));
    let mut seen = 0;
    for c in corpus["cases"].as_array().into_iter().flatten() {
        let id = c["id"].as_str().unwrap_or("");
        if !ids.iter().any(|i| i == id) {
            continue;
        }
        seen += 1;
        let want = c["expected"]["html"].as_str().unwrap_or("");
        let want = want.strip_suffix('\n').unwrap_or(want);
        let out = carve::render_with_diagnostics(
            c["input"].as_str().unwrap_or(""),
            &carve::options::Options::default(),
        );
        assert_eq!(out.html, want, "{id}");
    }
    assert_eq!(seen, ids.len(), "clause cases missing from the corpus");
}

/// CC-2251: `carve import` refuses the file whose unscoped `<th>` takes `scope` `col`.
#[test]
fn cc_2251_import_refuses_a_derived_scope() {
    let dir = std::env::temp_dir().join(format!("carve-rs-g1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html =
        "<table><tr><th>A</th><th scope=\"row\">B</th></tr><tr><td>1</td><td>2</td></tr></table>\n";
    std::fs::write(dir.join("t.html"), html).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["import", "--from", "html", "t.html"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("carve[writer-refused]:"), "{stderr}");
}
