//! carve#6804: the three alt states of an image (C2-A-115) through the CLI
//! import (C2-T-034) and a render, import and render again (C2-H-080, C2-M-050).
mod common;

use carve::api::{self, ImportOptions};
use carve::options::Options;
use common::parse_json;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

const CLI_CASES: &[&str] = &[
    "CC-570", "CC-2214", "CC-2215", "CC-2290", "CC-2291", "CC-2292",
];

fn run(case: &Value) -> Vec<String> {
    let id = case["id"].as_str().unwrap();
    let dir = std::env::temp_dir().join(format!("carve-rs-6804-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, file) in case["files"].as_object().into_iter().flatten() {
        std::fs::write(dir.join(name), file["text"].as_str().unwrap()).unwrap();
    }
    let args: Vec<&str> = case["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The binary may exit before reading stdin (usage error), closing the pipe.
    if let Err(err) = child
        .stdin
        .take()
        .unwrap()
        .write_all(case["stdin"].as_str().unwrap_or("").as_bytes())
    {
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe, "{err}");
    }
    let out = child.wait_with_output().unwrap();
    let (stdout, stderr) = (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    );
    let want = &case["expected"];
    let mut problems = Vec::new();
    if out.status.code().map(i64::from) != want["status"].as_i64() {
        problems.push(format!("status {:?}: {stderr}", out.status.code()));
    }
    if let Some(text) = want["stdout"].as_str()
        && stdout != text
    {
        problems.push(format!("stdout {stdout:?}, expected {text:?}"));
    }
    match &want["stderrCategory"] {
        Value::String(c) if !stderr.starts_with(&format!("carve[{c}]:")) => {
            problems.push(format!("stderr {stderr:?}, expected category {c}"));
        }
        Value::Null if want.get("stderrCategory").is_some() && stderr.starts_with("carve[") => {
            problems.push(format!("stderr {stderr:?}, expected no category"));
        }
        _ => {}
    }
    if let Some(entries) = want["stderrDiagnostics"].as_array() {
        let lines: Vec<&str> = stderr
            .lines()
            .filter(|l| !l.starts_with("carve["))
            .collect();
        if lines.len() != entries.len() {
            problems.push(format!("stderr lines {lines:?}, expected {entries:?}"));
        }
        for (line, entry) in lines.iter().zip(entries) {
            let head = format!(
                "{}: warning {} ",
                entry["file"].as_str().unwrap(),
                entry["code"].as_str().unwrap()
            );
            if !line.starts_with(&head) {
                problems.push(format!("stderr line {line:?}, expected {head:?}"));
            }
        }
    }
    problems
}

#[test]
fn c2_t_034_image_alt_cli_cases() {
    let corpus: Value = parse_json(include_str!("../spec-conformance/clause-cases.json"));
    let mut failures = Vec::new();
    let mut seen = 0;
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap_or("");
        if !CLI_CASES.contains(&id) {
            continue;
        }
        seen += 1;
        failures.extend(run(case).into_iter().map(|p| format!("{id}: {p}")));
    }
    assert_eq!(seen, CLI_CASES.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn import(html: &str) -> Value {
    let imported = api::import_document(html, &ImportOptions::default()).unwrap();
    parse_json(&api::encode_tree_with(
        &imported.tree,
        api::EncodeOptions { positions: false },
    ))["children"][0]["children"][0]
        .clone()
}

#[test]
fn c2_h_080_render_import_render_keeps_each_state() {
    for (html, image) in [
        (r#"<p><img src="x"></p>"#, r#"{"type":"image","src":"x"}"#),
        (
            r#"<p><img src="x" alt=""></p>"#,
            r#"{"type":"image","src":"x","decorative":true}"#,
        ),
        (
            r#"<p><img src="x" alt="t"></p>"#,
            r#"{"type":"image","src":"x","alt":"t"}"#,
        ),
    ] {
        let node = import(html);
        assert_eq!(node, parse_json(image), "{html}");
        let tree = format!(
            r#"{{"type":"document","astVersion":"2.0","children":[{{"type":"paragraph","children":[{image}]}}]}}"#
        );
        let rendered =
            api::render_tree(&api::decode_tree(&tree).unwrap(), &Options::default()).unwrap();
        assert_eq!(rendered.trim_end(), html, "{image}");
        assert_eq!(import(&rendered), node, "{image}");
    }
}
