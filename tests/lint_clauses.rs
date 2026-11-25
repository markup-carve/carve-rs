//! C2-T-005: exercise every lint clause case with the pinned catalog.

use carve::diagnostic::Diagnostic;
use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use carve::options::HeadingIds;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

fn options(case: &Value) -> LintOptions {
    let mut options = LintOptions::default();
    if let Some(heading_ids) = case["options"]["headingIds"]
        .as_str()
        .and_then(HeadingIds::from_name)
    {
        options.heading_ids = heading_ids;
    }
    if let Some(rules) = case["options"]["rules"].as_object() {
        for (name, state) in rules {
            options
                .rules
                .insert(name.clone(), state.as_str().unwrap_or("").to_string());
        }
    }
    options.upgrade = case["options"]["upgrade"] == true;
    if let Some(platforms) = case["options"]["platforms"].as_array() {
        options.platforms = platforms
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    if let Some(extensions) = case["options"]["extensions"].as_array() {
        options.extensions = extensions
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    if let Some(files) = case.get("files") {
        let sources: HashMap<String, Vec<u8>> = files
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(name, file)| {
                file["text"]
                    .as_str()
                    .map(|text| (name.clone(), text.as_bytes().to_vec()))
            })
            .collect();
        let resolver = Arc::new(move |request: &Request<'_>| {
            sources
                .get(request.path)
                .cloned()
                .map(|bytes| (bytes, request.path.to_string()))
                .ok_or(Cause::NotFound)
        });
        options.includes = Some(Includes::new(resolver));
    }
    options
}

fn agrees(expected: &Value, actual: &Diagnostic) -> bool {
    expected["code"].as_str() == Some(actual.code)
        && expected["channel"]
            .as_str()
            .is_none_or(|v| v == actual.channel)
        && expected["severity"]
            .as_str()
            .is_none_or(|v| v == actual.severity)
        && expected["file"]
            .as_str()
            .is_none_or(|v| Some(v) == actual.file.as_deref())
        && expected["path"]
            .as_str()
            .is_none_or(|v| Some(v) == actual.path.as_deref())
        && expected["line"]
            .as_u64()
            .is_none_or(|v| actual.pos.is_some_and(|pos| u64::from(pos.start_line) == v))
        && expected["column"].as_u64().is_none_or(|v| {
            actual
                .pos
                .is_some_and(|pos| u64::from(pos.start_column) == v)
        })
        && expected["endLine"]
            .as_u64()
            .is_none_or(|v| actual.pos.is_some_and(|pos| u64::from(pos.end_line) == v))
        && expected["endColumn"]
            .as_u64()
            .is_none_or(|v| actual.pos.is_some_and(|pos| u64::from(pos.end_column) == v))
}

#[test]
fn pinned_lint_clause_cases() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/spec-conformance/clause-cases.json"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut parser = serde_json::Deserializer::from_str(&text);
    parser.disable_recursion_limit();
    let corpus: Value = serde::Deserialize::deserialize(&mut parser).unwrap();
    let mut checked = 0;
    let mut failures = Vec::new();
    for case in corpus["cases"].as_array().into_iter().flatten() {
        if case["kind"] != "lint" || case["expected"].get("refusal").is_some() {
            continue;
        }
        let input = case["input"].as_str().unwrap_or("");
        let actual = lint(input, &options(case));
        let id = case["id"].as_str().unwrap_or("");
        for expected in case["expected"]["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
        {
            checked += 1;
            if !actual.iter().any(|d| agrees(expected, d)) {
                failures.push(format!("{id}: expected {expected}; actual {actual:?}"));
            }
        }
        for code in case["expected"]["notReported"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            checked += 1;
            if actual.iter().any(|d| d.code == code) {
                failures.push(format!("{id}: unexpected {code}; actual {actual:?}"));
            }
        }
        if case["expected"]["diagnosticsComplete"] == true {
            checked += 1;
            if actual.len()
                != case["expected"]["diagnostics"]
                    .as_array()
                    .map_or(0, Vec::len)
            {
                failures.push(format!("{id}: incomplete list; actual {actual:?}"));
            }
        }
    }
    println!("checked {checked} lint clause expectations");
    // The pinned catalog has two older ranges that conflict with later cases
    // for the same diagnostics. Keep checking every other expectation while
    // the spec resolves CC-813 against CC-1174 and CC-891 against CC-1163.
    let unresolved = ["CC-813:", "CC-891:"];
    // C2-B-187, ruling of carve#6637 (carve-rs line, lane w6637-rs): not ported yet. A held
    // case that passes fails the run, so the hold goes with the port.
    let held = ["CC-2069:"];
    for id in held {
        assert!(
            failures.iter().any(|f| f.starts_with(id)),
            "{id} held but passes; drop the hold"
        );
    }
    failures.retain(|f| !held.iter().any(|id| f.starts_with(id)));
    let regressions: Vec<_> = failures
        .iter()
        .filter(|failure| !unresolved.iter().any(|id| failure.starts_with(id)))
        .collect();
    assert!(
        regressions.is_empty(),
        "{}",
        regressions
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        failures.len() <= unresolved.len(),
        "{}",
        failures.join("\n")
    );
}
