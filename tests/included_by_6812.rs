//! C2-T-020, C2-N-087: every entry with `file` carries `includedBy`, the
//! chain of its first reach (carve-js#5056, carve#6812 G6).

use carve::api::{self, Entry};
use carve::include::{Cause, Includes, Request};
use carve::lint::LintOptions;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

fn includes(files: &[(&str, &str)], top: Option<&str>) -> Includes {
    let sources: HashMap<String, Vec<u8>> = files
        .iter()
        .map(|(name, text)| ((*name).to_string(), text.as_bytes().to_vec()))
        .collect();
    let mut includes = Includes::new(Arc::new(move |request: &Request<'_>| {
        sources
            .get(request.path)
            .cloned()
            .map(|bytes| (bytes, request.path.to_string()))
            .ok_or(Cause::NotFound)
    }));
    includes.file = top.map(str::to_string);
    includes
}

fn chains(source: &str, files: &[(&str, &str)], top: Option<&str>) -> Vec<(String, Value)> {
    let options = LintOptions {
        includes: Some(includes(files, top)),
        ..LintOptions::default()
    };
    api::lint(source, &options)
        .unwrap()
        .iter()
        .filter(|e: &&Entry| e.file.is_some())
        .map(|e| {
            let v = serde_json::to_value(e).unwrap();
            (e.code.clone(), v["includedBy"].clone())
        })
        .collect()
}

#[test]
fn cc_2335_lint_findings_carry_the_chain_of_the_first_reach() {
    let got = chains(
        "{{ b.crv }}\n\n{{ b.crv }}\n",
        &[
            ("b.crv", "See [x][missing].\n\n{{ c.crv }}\n"),
            ("c.crv", "Go [y](#nope).\n"),
        ],
        None,
    );
    assert_eq!(
        got,
        vec![
            (
                "unresolved-reference-link".into(),
                json!([{"range": {"startOffset": 0, "endOffset": 11}}])
            ),
            (
                "broken-fragment-link".into(),
                json!([
                    {"range": {"startOffset": 0, "endOffset": 11}},
                    {"file": "b.crv", "range": {"startOffset": 19, "endOffset": 30}}
                ])
            ),
        ]
    );
}

#[test]
fn cc_2336_an_include_failure_in_an_included_file_carries_the_chain() {
    let got = chains(
        "{{ b.crv }}\n",
        &[("b.crv", "x\n\n{{ nothere.crv }}\n")],
        None,
    );
    assert_eq!(
        got,
        vec![(
            "include-not-found".into(),
            json!([{"range": {"startOffset": 0, "endOffset": 11}}])
        )]
    );
}

#[test]
fn a_parse_finding_two_levels_down_carries_both_sites() {
    let got = chains(
        "{{ b.crv }}\n",
        &[
            ("b.crv", "{{ c.crv }}\n"),
            ("c.crv", "[a]: /x\n[a]: /y\n\nSee [a].\n"),
        ],
        None,
    );
    assert_eq!(
        got,
        vec![(
            "duplicate-link-reference-definition".into(),
            json!([
                {"range": {"startOffset": 0, "endOffset": 11}},
                {"file": "b.crv", "range": {"startOffset": 0, "endOffset": 11}}
            ])
        )]
    );
}

#[test]
fn a_position_only_the_second_reach_selects_carries_that_reach() {
    let got = chains(
        "{{ s.crv @lines:1-1 }}\n\n{{ s.crv @lines:3-4 }}\n",
        &[("s.crv", "x\n\n[a]: /x\n[a]: /y\n")],
        None,
    );
    assert_eq!(
        got.iter()
            .find(|(code, _)| code == "duplicate-link-reference-definition")
            .map(|(_, chain)| chain.clone()),
        Some(json!([{"range": {"startOffset": 24, "endOffset": 46}}]))
    );
}

#[test]
fn a_site_in_a_named_top_level_document_names_no_file() {
    let got = chains(
        "{{ b.crv }}\n",
        &[("b.crv", "x\n\n{{ nothere.crv }}\n")],
        Some("top.crv"),
    );
    assert_eq!(
        got,
        vec![(
            "include-not-found".into(),
            json!([{"range": {"startOffset": 0, "endOffset": 11}}])
        )]
    );
}

#[test]
fn the_command_line_adds_line_and_column_to_each_site() {
    let dir = std::env::temp_dir().join(format!("carve-included-by-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.crv"), "{{ b.crv }}\n").unwrap();
    std::fs::write(dir.join("b.crv"), "x\n\n{{ c.crv }}\n").unwrap();
    std::fs::write(dir.join("c.crv"), "Go [y](#nope).\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["lint", "--json", "a.crv"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    let entry = &report["files"][0]["diagnostics"][0];
    assert_eq!(entry["code"], "broken-fragment-link");
    assert_eq!(
        entry["includedBy"],
        json!([
            {"line": 1, "column": 1, "range": {"startOffset": 0, "endOffset": 11}},
            {"file": "b.crv", "line": 3, "column": 1, "range": {"startOffset": 3, "endOffset": 14}}
        ])
    );
}

#[test]
fn a_heading_clamp_carries_the_include_that_shifted_it() {
    let got = chains(
        "{{ h.crv }}\n\n{{ h.crv @shift:10 }}\n",
        &[("h.crv", "# H\n")],
        None,
    );
    assert_eq!(
        got,
        vec![(
            "include-heading-clamp".into(),
            json!([{"range": {"startOffset": 13, "endOffset": 34}}])
        )]
    );
}

#[test]
fn an_export_loss_in_an_included_file_carries_the_chain() {
    let options = api::Options {
        includes: Some(includes(
            &[
                ("n.crv", "{{ m.crv }}\n"),
                ("m.crv", "``` =latex\n\\x\n```\n"),
            ],
            None,
        )),
        ..api::Options::default()
    };
    let tree = api::parse("top\n\n{{ n.crv }}\n", &options).unwrap();
    let written =
        carve::export::to_markdown(&tree, &carve::export::ExportOptions::default()).unwrap();
    let entry = written
        .diagnostics
        .iter()
        .find(|e| e.code == "raw-format-dropped")
        .unwrap();
    assert_eq!(entry.file.as_deref(), Some("m.crv"));
    assert_eq!(
        serde_json::to_value(entry).unwrap()["includedBy"],
        json!([
            {"range": {"startOffset": 5, "endOffset": 16}},
            {"file": "n.crv", "range": {"startOffset": 0, "endOffset": 11}}
        ])
    );
}
