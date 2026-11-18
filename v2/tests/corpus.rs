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

fn expected_tree(json: &str) -> String {
    common::sorted(&common::parse_json(json))
}

#[test]
#[ignore = "candidate 3 JavaScript oracle; candidate 12 specification corpus is the core gate"]
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
    let candidate3: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(format!("{data}corpus-candidate3.json")).unwrap(),
    )
    .unwrap();
    let candidate_by_id: BTreeMap<&str, &Value> = candidate3
        .iter()
        .map(|d| (d["id"].as_str().unwrap(), d))
        .collect();
    let file = std::fs::File::open(format!("{data}corpus.jsonl.gz")).unwrap();
    let reader = BufReader::new(GzDecoder::new(file));
    let mut seen = BTreeSet::new();
    let mut count = 0;
    let mut failures = Vec::new();
    for line in reader.lines() {
        let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let id = row["id"].as_str().unwrap();
        let input = row["input"].as_str().unwrap();
        let opts = common::options_of(&row["opts"]);
        let difference = by_id.get(id).copied();
        let html = candidate_by_id.get(id).map_or_else(
            || difference.map_or(&row["html"], |d| &d["expectedHtml"]),
            |d| {
                assert_eq!(d["clause"], "C2-H-133");
                &d["expectedHtml"]
            },
        );
        let expected_json = difference.map_or(&row["tree"], |d| &d["expectedTree"]);
        if carve2::render(input, &opts) != html.as_str().unwrap() {
            failures.push(format!("HTML: {id}"));
        }
        if common::sorted(&tree(input, &opts)) != expected_tree(expected_json.as_str().unwrap()) {
            failures.push(format!("tree: {id}"));
        }
        if let Some(d) = difference {
            assert!(d["clause"].as_str().is_some_and(|s| s.starts_with("C2-")));
            assert!(d["caseId"].as_str().is_some_and(|s| !s.is_empty()));
            let rewrite = d["rewrite"].as_str().unwrap();
            assert_eq!(
                carve2::render(rewrite, &opts),
                d["rewriteOracleHtml"],
                "rewrite HTML: {id}"
            );
            assert_eq!(
                common::sorted(&tree(rewrite, &opts)),
                expected_tree(d["rewriteOracleTree"].as_str().unwrap()),
                "rewrite tree: {id}"
            );
            seen.insert(id.to_owned());
        }
        count += 1;
    }
    let seeds: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{data}corpus-seeds.json")).unwrap())
            .unwrap();
    let indices: usize = seeds["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["indices"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(count, indices + differences.len());
    assert_eq!(seen.len(), differences.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
