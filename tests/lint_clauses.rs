//! C2-T-005: exercise every lint clause case with the pinned catalog.
#[path = "support/holds.rs"]
mod holds;

use carve::diagnostic::Diagnostic;
use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use carve::options::HeadingIds;
use holds::{Gate, Holds};
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
    expected.as_str().or_else(|| expected["code"].as_str()) == Some(actual.code)
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
        && expected["includedBy"]
            .as_array()
            .is_none_or(|chain| included_by_agrees(chain, actual))
}

// C2-T-020: the include chain of an entry with `file`, from its first reach.
fn included_by_agrees(chain: &[Value], actual: &Diagnostic) -> bool {
    chain.len() == actual.included_by.len()
        && chain.iter().zip(&actual.included_by).all(|(site, got)| {
            site["file"].as_str() == got.file.as_deref()
                && site["line"].as_u64() == Some(u64::from(got.line))
                && site["column"].as_u64() == Some(u64::from(got.column))
        })
}

/// `(case id, cause)`: rulings this engine has not ported yet.
#[rustfmt::skip]
const HELD: Holds = &[
    ("CC-2241", "carve#6838: the lint-model row exempts a pipe table from the header-rows clamp"),
    ("CC-2359", "carve#6395: named snippets"),
    ("CC-2361", "carve#6395: named snippets"),
    ("CC-2362", "carve#6395: named snippets"),
    ("CC-2364", "carve#6395: named snippets"),
    ("CC-2385", "carve#6395: named snippets"),
    ("CC-2386", "carve#6395: named snippets"),
    ("CC-2387", "carve#6395: named snippets"),
];

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
    let mut gate = Gate::new("lint clause cases", HELD);
    for case in corpus["cases"].as_array().into_iter().flatten() {
        if case["kind"] != "lint" || case["expected"].get("refusal").is_some() {
            continue;
        }
        let input = case["input"].as_str().unwrap_or("");
        let actual = lint(input, &options(case));
        let id = case["id"].as_str().unwrap_or("");
        gate.check(id, true, String::new);
        for expected in case["expected"]["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
        {
            checked += 1;
            gate.check(id, actual.iter().any(|d| agrees(expected, d)), || {
                format!("expected {expected}; actual {actual:?}")
            });
        }
        for code in case["expected"]["notReported"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            checked += 1;
            gate.check(id, !actual.iter().any(|d| d.code == code), || {
                format!("unexpected {code}; actual {actual:?}")
            });
        }
        if case["expected"]["diagnosticsComplete"] == true {
            checked += 1;
            let wanted = case["expected"]["diagnostics"]
                .as_array()
                .map_or(0, Vec::len);
            gate.check(id, actual.len() == wanted, || {
                format!("incomplete list; actual {actual:?}")
            });
        }
    }
    println!("checked {checked} lint clause expectations");
    gate.finish();
}
