//! C2-M-041 (carve-js#5574, ruling B): a `<col>` style read as CSS
//! declarations. The clause cases CC-2046 to CC-2053, with the written source
//! and the complete record that `import_cases.rs` does not compare.
mod common;

use carve::api::{self, ImportOptions};
use common::parse_json;

#[test]
fn col_style_clause_cases_write_and_record() {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let corpus = parse_json(&std::fs::read_to_string(path).unwrap_or_default());
    let ids: Vec<String> = (2046..=2053).map(|n| format!("CC-{n}")).collect();
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
fn col_style_declaration_spellings() {
    let width = |style: &str| {
        let html = format!(
            "<table><colgroup><col style=\"{style}\"><col></colgroup><tr><td>a</td><td>b</td></tr></table>"
        );
        let got = api::import_document(&html, &ImportOptions::default()).unwrap();
        let dropped = got
            .diagnostics
            .iter()
            .any(|d| d.code == "import-attribute-dropped");
        (got.source.starts_with("{widths=40"), dropped)
    };
    assert_eq!(width("width: 40%;"), (true, false));
    assert_eq!(width("\twidth\t:\t40%\t"), (true, false));
    assert_eq!(width(";;Width:40%;;"), (true, false));
    assert_eq!(width("width: 40%; width: 4px"), (true, true));
    assert_eq!(width("color: red; width: 40%"), (true, true));
    assert_eq!(width("width 40%"), (false, true));
    assert_eq!(width("width: 40 %"), (false, true));
    assert_eq!(width("width: 0%"), (false, true));
    assert_eq!(width("width: 100.5%"), (false, true));
    assert_eq!(width("min-width: 40%"), (false, true));
}
