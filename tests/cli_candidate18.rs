mod common;
#[path = "support/holds.rs"]
mod holds;

use common::parse_json;
use holds::{Gate, Holds};
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

/// `(case id, cause)`.
const HELD: Holds = &[(
    "CC-1354",
    "carve#6833: CC-1354 still expects the braced strike reported, which #6499 turned off",
)];

#[test]
fn candidate18_cli_clause_cases() {
    let corpus: Value = parse_json(include_str!("../spec-conformance/clause-cases.json"));
    let mut gate = Gate::new("candidate 18 CLI cases", HELD);
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap_or("");
        let number = id
            .strip_prefix("CC-")
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        if !(1352..=1365).contains(&number) {
            continue;
        }
        let dir =
            std::env::temp_dir().join(format!("carve-rs-candidate18-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(files) = case["files"].as_object() {
            for (name, file) in files {
                std::fs::write(dir.join(name), file["text"].as_str().unwrap()).unwrap();
            }
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
        child
            .stdin
            .take()
            .unwrap()
            .write_all(case["stdin"].as_str().unwrap_or("").as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let want = &case["expected"];
        let status = want["status"].as_i64();
        let mut problems = Vec::new();
        let code = status.map(|n| i32::try_from(n).unwrap());
        if out.status.code() != code {
            problems.push(format!("status {:?}, expected {code:?}", out.status.code()));
        }
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        if let Some(text) = want["stdout"].as_str()
            && stdout != text
        {
            problems.push(format!("stdout\n--- expected\n{text}--- got\n{stdout}"));
        }
        if let Some(category) = want["stderrCategory"].as_str()
            && !stderr.starts_with(&format!("carve[{category}]:"))
        {
            problems.push(format!("stderr {stderr}"));
        }
        if let Some(files) = want["filesAfter"].as_object() {
            for (name, file) in files {
                let actual = std::fs::read_to_string(dir.join(name)).ok();
                if actual.as_deref() != file["text"].as_str() {
                    problems.push(format!("{name}: {actual:?}"));
                }
            }
        }
        gate.problems(id, &problems);
    }
    gate.finish();
}

#[test]
fn output_can_name_the_input_without_erasing_it() {
    let dir = std::env::temp_dir().join(format!("carve-rs-output-input-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.crv"), "a **b**\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["fmt", "--output", "a.crv", "a.crv"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("a.crv")).unwrap(),
        "a \\*\\*b**\n"
    );
}

#[test]
fn fix_alias_reports_json_and_removes_closing_heading_space() {
    let dir = std::env::temp_dir().join(format!("carve-rs-fix-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.crv"), "# Title ##\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["fix", "--write", "--json", "a.crv"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("a.crv")).unwrap(),
        "# Title\n"
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["files"][0]["file"], "a.crv");
}
