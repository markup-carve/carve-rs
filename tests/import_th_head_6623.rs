//! C2-M-037 (carve#6623, ruling B): with no `<thead>`, the leading all-`<th>`
//! rows of the first body are the head. CC-1450 and CC-2054 to CC-2064, with
//! the written source and the record that `import_cases.rs` does not compare.
mod common;

use carve::api::{self, ImportOptions};
use common::parse_json;

#[test]
fn th_head_clause_cases_write_and_record() {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let corpus = parse_json(&std::fs::read_to_string(path).unwrap_or_default());
    let mut ids: Vec<String> = (2054..=2064).map(|n| format!("CC-{n}")).collect();
    ids.push("CC-1450".into());
    let mut seen = 0;
    let mut failures = Vec::new();
    for c in corpus["cases"].as_array().into_iter().flatten() {
        let id = c["id"].as_str().unwrap_or("");
        if !ids.iter().any(|i| i == id) {
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
        let want: Vec<&str> = expected["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["code"].as_str())
            .collect();
        let codes: Vec<&str> = got.diagnostics.iter().map(|d| d.code.as_str()).collect();
        if want != codes {
            failures.push(format!("{id}: record {codes:?}, expected {want:?}"));
        }
    }
    assert_eq!(seen, ids.len(), "clause cases missing from the corpus");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn emptied_first_body_attributes_are_reported() {
    let html = "<table><tbody class=\"x\"><tr><th>A</th></tr></tbody></table>";
    let got = api::import_document(html, &ImportOptions::default()).unwrap();
    assert_eq!(got.source, "|= A |\n");
    let codes: Vec<&str> = got.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["import-attribute-dropped"]);
}
