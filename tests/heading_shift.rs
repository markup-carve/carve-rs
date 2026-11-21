//! `headingShift` (C2-H-208, C2-H-209): the export and command line clause
//! cases, which the core gate does not run, and the paths a host reaches.
mod common;

use carve::api::{self, Carve, Options, extensions};
use common::parse_json;
use serde_json::{Value, json};
use std::process::Command;

fn cases(kind: &str) -> Vec<Value> {
    parse_json(include_str!("../spec-conformance/clause-cases.json"))["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| {
            c["kind"] == kind
                && c["clauses"]
                    .as_array()
                    .is_some_and(|cl| cl.iter().any(|x| x == "C2-H-208" || x == "C2-H-209"))
        })
        .cloned()
        .collect()
}

fn call(request: &Value) -> Value {
    parse_json(&api::json::call(&request.to_string()))
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1;
    (line, column)
}

#[test]
fn c2_h_209_export_clause_cases() {
    let cases = cases("export");
    assert_eq!(cases.len(), 4);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let parsed = call(&json!({"call": "parse", "source": input}));
        let out = call(&json!({
            "call": "writeWithDiagnostics",
            "target": case["to"],
            "tree": parsed["ok"]["tree"],
            "options": case["options"],
        }));
        let want = &case["expected"];
        if want.get("refusal").is_some() {
            assert_eq!(out["error"]["name"], "CarveOptionError", "{id}: {out}");
            continue;
        }
        assert_eq!(out["ok"]["output"], want["output"], "{id}: {out}");
        let got = out["ok"]["diagnostics"].as_array().unwrap();
        let expected = want["diagnostics"].as_array().unwrap();
        assert_eq!(got.len(), expected.len(), "{id}: {out}");
        for (g, w) in got.iter().zip(expected) {
            assert_eq!(g["code"], w["code"], "{id}");
            let start = usize::try_from(g["range"]["startOffset"].as_u64().unwrap()).unwrap();
            let (line, column) = line_column(input, start);
            assert_eq!(json!(line), w["line"], "{id}");
            assert_eq!(json!(column), w["column"], "{id}");
        }
    }
}

#[test]
fn c2_t_010_heading_shift_cli_clause_cases() {
    let cases = cases("cli");
    assert_eq!(cases.len(), 4);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let dir = std::env::temp_dir().join(format!("carve-rs-6325-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, file) in case["files"].as_object().unwrap() {
            std::fs::write(dir.join(name), file["text"].as_str().unwrap()).unwrap();
        }
        let args: Vec<&str> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let out = Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(&args)
            .current_dir(&dir)
            .output()
            .unwrap();
        let want = &case["expected"];
        assert_eq!(
            out.status.code().map(i64::from),
            want["status"].as_i64(),
            "{id}"
        );
        if let Some(text) = want["stdout"].as_str() {
            assert_eq!(String::from_utf8_lossy(&out.stdout), text, "{id}");
        }
        if let Some(category) = want["stderrCategory"].as_str() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                stderr.starts_with(&format!("carve[{category}]:")),
                "{id}: {stderr}"
            );
        }
    }
}

#[test]
fn c2_h_209_cli_refuses_the_shift_for_plain_and_ansi_and_reads_it_for_markdown() {
    let dir = std::env::temp_dir().join(format!("carve-rs-6325-to-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.crv"), "= A\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    for to in ["plain", "ansi"] {
        let out = run(&["render", "--to", to, "--heading-shift=0", "a.crv"]);
        assert_eq!(out.status.code(), Some(2), "{to}");
        assert!(String::from_utf8_lossy(&out.stderr).starts_with("carve[usage]:"));
    }
    let out = run(&["render", "--to", "markdown", "--heading-shift=1", "a.crv"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "## A\n");
}

#[test]
fn c2_h_208_transforms_see_the_shifted_levels_in_any_order() {
    let mut host = Carve::new();
    host.register(extensions::heading_numbers(
        extensions::HeadingNumbersOptions::default(),
    ))
    .unwrap();
    let opts = Options {
        heading_shift: 1,
        ..Options::default()
    };
    let out = host.render("= A\n\n== B\n", &opts).unwrap();
    assert!(out.contains("<h2>"), "{out}");
    assert!(out.contains("<h3>"), "{out}");
    let tree = host.parse("= A\n", &Options::default()).unwrap();
    let from_tree = host.render_tree(&tree, &opts).unwrap();
    assert!(from_tree.contains("<h2>"), "{from_tree}");
}

#[test]
fn c2_h_208_the_tree_and_the_formatter_keep_the_authored_levels() {
    let opts = Options {
        heading_shift: 2,
        ..Options::default()
    };
    let tree = api::parse("= A\n", &opts).unwrap();
    assert!(api::encode_tree(&tree).contains("\"level\":1"));
    let plain = api::render_tree(&tree, &opts).unwrap();
    assert_eq!(plain, "<section id=\"A\">\n  <h3>A</h3>\n</section>");
}

#[test]
fn c2_h_208_strict_losses_refuses_a_level_above_6_unless_allowed() {
    let strict = Options {
        heading_shift: 1,
        strict_losses: true,
        ..Options::default()
    };
    assert!(api::render("====== A\n", &strict).is_err());
    let allowed = Options {
        allow_losses: vec!["heading-shift-above-6".into()],
        ..strict
    };
    let out = api::render_with_diagnostics("====== A\n", &allowed).unwrap();
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(out.html.contains("aria-level=\"7\""), "{}", out.html);
}

#[test]
fn c2_h_208_extension_is_gone() {
    let out = call(&json!({
        "call": "render",
        "source": "= A\n",
        "options": {"extensions": ["carve.headingLevelShift"]},
    }));
    assert!(out.get("error").is_some(), "{out}");
}

#[test]
fn c2_h_208_a_host_render_places_the_loss_at_the_heading() {
    let mut host = Carve::new();
    host.register(extensions::heading_numbers(
        extensions::HeadingNumbersOptions::default(),
    ))
    .unwrap();
    let opts = Options {
        heading_shift: 1,
        ..Options::default()
    };
    let out = host
        .render_with_diagnostics("= A\n\n====== B\n", &opts)
        .unwrap();
    let loss: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.code == "heading-shift-above-6")
        .collect();
    assert_eq!(loss.len(), 1, "{:?}", out.diagnostics);
    assert_eq!(loss[0].range.map(|r| r.start_offset), Some(5));
}

#[test]
fn c2_h_208_a_heading_in_a_degraded_container_is_reported() {
    let opts = Options {
        heading_shift: 1,
        profile: carve::options::Profile::Comment,
        ..Options::default()
    };
    let out = api::render_with_diagnostics("> ====== A\n", &opts).unwrap();
    let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == "heading-shift-above-6")
            .count(),
        1,
        "{codes:?} {}",
        out.html
    );
}

#[test]
fn c2_h_208_a_document_transform_keeps_the_shift_fact_and_final_carrier() {
    let cases = [
        (
            "====== A\n",
            2,
            "<h2 data-carve-source-level=\"1\">A</h2>",
            vec!["heading-shift-above-6"],
        ),
        (
            "= A\n",
            7,
            "<div data-carve-source-level=\"6\" role=\"heading\" aria-level=\"7\">A</div>",
            vec![],
        ),
        (
            "= A\n",
            1,
            "<h1 data-carve-source-level=\"0\">A</h1>",
            vec![],
        ),
    ];
    for (source, level, html, codes) in cases {
        let out = call(&json!({
            "call": "renderWithDiagnostics",
            "source": source,
            "options": {"headingShift": 1, "roundtrip": true, "sections": false, "headingIds": "off"},
            "host": {"documentTransforms": [{"name": "sample.level", "after": "/children/0", "setLevel": level}]}
        }));
        assert_eq!(out["ok"]["html"], html, "{out}");
        let actual: Vec<_> = out["ok"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap())
            .collect();
        assert_eq!(actual, codes, "{out}");
    }
}

#[test]
fn c2_h_208_an_inserted_heading_gets_the_final_level_carrier() {
    let out = call(&json!({
        "call": "renderWithDiagnostics",
        "source": "= A\n",
        "options": {"headingShift": 1, "roundtrip": true, "sections": false, "headingIds": "off"},
        "host": {"documentTransforms": [{"name": "sample.add", "after": "/children/0",
            "insert": {"type": "heading", "level": 3, "children": [{"type": "text", "value": "B"}]}}]}
    }));
    assert_eq!(
        out["ok"]["html"],
        "<h2 data-carve-source-level=\"1\">A</h2>\n<h3 data-carve-source-level=\"2\">B</h3>",
        "{out}"
    );
}
