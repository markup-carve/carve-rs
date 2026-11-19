mod common;
use common::parse_json;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn candidate18_cli_clause_cases() {
    let corpus: Value = parse_json(include_str!("../spec-conformance/clause-cases.json"));
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
        assert_eq!(
            out.status.code(),
            want["status"].as_i64().map(|n| n as i32),
            "{id}: status"
        );
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        if let Some(text) = want["stdout"].as_str() {
            assert_eq!(stdout, text, "{id}: stdout");
        }
        if let Some(category) = want["stderrCategory"].as_str() {
            assert!(
                stderr.starts_with(&format!("carve[{category}]:")),
                "{id}: stderr {stderr}"
            );
        }
        if let Some(files) = want["filesAfter"].as_object() {
            for (name, file) in files {
                let actual = std::fs::read_to_string(dir.join(name)).ok();
                assert_eq!(actual.as_deref(), file["text"].as_str(), "{id}: {name}");
            }
        }
    }
}
