//! C2-M-040 (carve#6607, ruling C): with no head, a column gives its cells'
//! alignment only where every column before it gives one, per axis. CC-2077
//! and CC-2078, with the written source and record `import_cases.rs` skips.
mod common;

use carve::api::{self, ImportOptions};
use common::parse_json;

fn import(html: &str) -> (String, Vec<String>) {
    let got = api::import_document(html, &ImportOptions::default()).unwrap();
    let codes = got.diagnostics.iter().map(|d| d.code.clone()).collect();
    (got.source, codes)
}

#[test]
fn headless_align_clause_cases_write_and_record() {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let corpus = parse_json(&std::fs::read_to_string(path).unwrap_or_default());
    let ids = ["CC-2077", "CC-2078"];
    let mut seen = 0;
    for c in corpus["cases"].as_array().into_iter().flatten() {
        let id = c["id"].as_str().unwrap_or("");
        if !ids.contains(&id) {
            continue;
        }
        seen += 1;
        let (source, codes) = import(c["html"].as_str().unwrap_or(""));
        assert_eq!(
            Some(source.as_str()),
            c["expected"]["carve"].as_str(),
            "{id}"
        );
        let want: Vec<&str> = c["expected"]["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d.as_str().or_else(|| d["code"].as_str()))
            .collect();
        assert_eq!(codes, want, "{id}");
    }
    assert_eq!(seen, ids.len(), "clause cases missing from the corpus");
}

#[test]
fn aligned_prefix_gives_columns_and_the_rest_keep_cell_marks() {
    let html = "<table><tr><td align=\"left\">a</td><td>b</td><td align=\"right\">c</td></tr>\
                <tr><td align=\"left\">d</td><td>e</td><td align=\"right\">f</td></tr></table>";
    let (source, codes) = import(html);
    assert_eq!(
        source,
        "{aligns=left}\n| a | b |{>} c |\n| d | e |{>} f |\n"
    );
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn baseline_column_ends_the_valigns_prefix() {
    let html = "<table><tr><td valign=\"top\">a</td><td valign=\"baseline\">b</td>\
                <td valign=\"bottom\">c</td></tr></table>";
    let (source, codes) = import(html);
    assert_eq!(source, "{valigns=top}\n| a | b |{?v} c |\n");
    assert_eq!(codes, ["field-unspellable"]);
}
