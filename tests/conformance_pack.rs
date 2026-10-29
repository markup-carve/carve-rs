//! The spec repository's conformance pack (`resources/conformance-pack/` at the
//! spec pin): generated variants per cause family with the spec model's HTML.
//!
//! A case that differs must be declared in `tests/fixtures/conformance-pack-drift.json`
//! under the ticket for its cause; an undeclared difference fails, and so does a
//! declared case that now passes or no longer exists. `CARVE_CONFORMANCE_PACK=<dir>`
//! runs another pack, e.g. a carve checkout's `resources/conformance-pack`.
//! A spec pin older than the pack skips the test.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn pack_dir() -> PathBuf {
    std::env::var_os("CARVE_CONFORMANCE_PACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/resources/conformance-pack")
        })
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn declared() -> BTreeMap<String, String> {
    let ledger = read_json(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance-pack-drift.json"),
    );
    let mut cases = BTreeMap::new();
    for entry in ledger["entries"].as_array().expect("entries") {
        let ticket = entry["ticket"].as_str().expect("ticket");
        for id in entry["cases"].as_array().expect("cases") {
            let id = id.as_str().expect("case id").to_string();
            assert!(
                cases.insert(id.clone(), ticket.to_string()).is_none(),
                "{id} is declared twice"
            );
        }
    }
    cases
}

#[test]
fn every_pack_case_renders_as_the_spec_model_or_is_declared() {
    let dir = pack_dir();
    if std::env::var_os("CARVE_CONFORMANCE_PACK").is_none() && !dir.join("index.json").exists() {
        eprintln!(
            "conformance pack: none at the spec pin yet ({}), skipped",
            dir.display()
        );
        return;
    }
    let index = read_json(&dir.join("index.json"));
    let declared = declared();
    let mut seen = BTreeSet::new();
    let mut undeclared = Vec::new();
    let mut fixed = Vec::new();
    for (family, spec) in index["families"].as_object().expect("families") {
        let roundtrip = spec["mode"] == "roundtrip";
        for shard in spec["shards"].as_array().expect("shards") {
            let cases = read_json(&dir.join(shard.as_str().expect("shard name")));
            for case in cases.as_array().expect("cases") {
                let id = case["id"].as_str().expect("id");
                let source = case["source"].as_str().expect("source");
                let html = if roundtrip {
                    carve::to_html(&carve::to_carve(source))
                } else {
                    carve::to_html(source)
                };
                let passes = html.trim() == case["html"].as_str().expect("html");
                seen.insert(id.to_string());
                match (passes, declared.get(id)) {
                    (false, None) => undeclared.push(format!("{id} ({family})")),
                    (true, Some(ticket)) => fixed.push(format!("{id} ({ticket})")),
                    _ => {}
                }
            }
        }
    }
    let unknown: Vec<_> = declared
        .keys()
        .filter(|id| !seen.contains(*id))
        .cloned()
        .collect();
    assert!(
        undeclared.is_empty() && fixed.is_empty() && unknown.is_empty(),
        "conformance pack at {}:\n  differs from the spec model, not declared: {:?}\n  declared but passes now: {:?}\n  declared but not in the pack: {:?}\n\
         Fix the engine, or add the ids under their cause's ticket in tests/fixtures/conformance-pack-drift.json.",
        dir.display(),
        undeclared,
        fixed,
        unknown,
    );
}
