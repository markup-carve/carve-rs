//! C2-T-049: `fix` keeps the lint default of every rule (carve#6523).
mod common;
use common::parse_json;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn fix_clause_cases() {
    let corpus: Value = parse_json(include_str!("../spec-conformance/clause-cases.json"));
    let mut ran = 0;
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap_or("");
        let number = id
            .strip_prefix("CC-")
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        if !((1926..=1951).contains(&number) || [528, 573, 1352, 1354].contains(&number)) {
            continue;
        }
        ran += 1;
        let dir =
            std::env::temp_dir().join(format!("carve-rs-fix6523-{}-{id}", std::process::id()));
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
        assert_eq!(
            out.status.code(),
            status.map(|n| i32::try_from(n).unwrap()),
            "{id}: status"
        );
        if let Some(text) = want["stdout"].as_str() {
            assert_eq!(String::from_utf8(out.stdout).unwrap(), text, "{id}: stdout");
        }
    }
    assert_eq!(ran, 30);
}
