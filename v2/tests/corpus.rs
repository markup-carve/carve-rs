//! Fixed differential corpus from the pinned JavaScript reader.
mod common;

use flate2::read::GzDecoder;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};

fn tree(input: &str, opts: &carve2::Options) -> Value {
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, opts, &arena).unwrap();
    common::parse_json(&carve2::to_json(&doc))
}

#[test]
fn c2_fixed_oracle_corpus_and_clause_differences() {
    let data = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");
    let differences: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(format!("{data}corpus-differences.json")).unwrap(),
    )
    .unwrap();
    let by_id: BTreeMap<&str, &Value> = differences
        .iter()
        .map(|d| (d["id"].as_str().unwrap(), d))
        .collect();
    let file = std::fs::File::open(format!("{data}corpus.jsonl.gz")).unwrap();
    let reader = BufReader::new(GzDecoder::new(file));
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for line in reader.lines() {
        let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let id = row["id"].as_str().unwrap();
        let input = row["input"].as_str().unwrap();
        let opts = common::options_of(&row["opts"]);
        let difference = by_id.get(id).copied();
        let html = difference.map_or(&row["html"], |d| &d["expectedHtml"]);
        let expected_tree = difference.map_or(&row["tree"], |d| &d["expectedTree"]);
        assert_eq!(
            carve2::render(input, &opts),
            html.as_str().unwrap(),
            "HTML: {id}"
        );
        assert_eq!(
            common::sorted(&tree(input, &opts)),
            common::sorted(&common::parse_json(expected_tree.as_str().unwrap())),
            "tree: {id}"
        );
        if let Some(d) = difference {
            assert!(d["clause"].as_str().is_some_and(|s| s.starts_with("C2-")));
            let rewrite = d["rewrite"].as_str().unwrap();
            assert_eq!(
                carve2::render(rewrite, &opts),
                d["rewriteOracleHtml"],
                "rewrite HTML: {id}"
            );
            assert_eq!(
                common::sorted(&tree(rewrite, &opts)),
                common::sorted(&common::parse_json(
                    d["rewriteOracleTree"].as_str().unwrap()
                )),
                "rewrite tree: {id}"
            );
            seen.insert(id.to_owned());
        }
        count += 1;
    }
    assert_eq!(count, 3817);
    assert_eq!(seen.len(), differences.len());
}
