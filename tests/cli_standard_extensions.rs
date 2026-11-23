//! C2-T-010 (carve#6436): `--extensions` and `--extension-options` name the
//! standard extensions of `data/extensions.json` on `render`, `fmt`, `lint`
//! and `ast`.
mod common;

use common::parse_json;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("carve-rs-6436-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn carve(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn c2_t_010_standard_extension_cli_clause_cases() {
    let cases: Vec<Value> =
        parse_json(include_str!("../spec-conformance/clause-cases.json"))["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| {
                c["kind"] == "cli"
                    && c["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a == "--extensions" || a == "--extension-options")
            })
            .cloned()
            .collect();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let dir = dir(id);
        for (name, file) in case["files"].as_object().unwrap() {
            std::fs::write(dir.join(name), file["text"].as_str().unwrap()).unwrap();
        }
        let args: Vec<&str> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let out = carve(&dir, &args);
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
fn c2_t_010_every_standard_name_is_accepted_by_every_command() {
    let rows = parse_json(include_str!("../spec-data/extensions.json"));
    let dir = dir("names");
    std::fs::write(
        dir.join("a.crv"),
        ":::: tabs\n::: tab \"A\"\na\n:::\n::::\n",
    )
    .unwrap();
    for row in rows["extensions"].as_array().unwrap() {
        let list = format!("[{}]", row["name"]);
        for command in ["render", "ast", "fmt", "lint"] {
            let out = carve(&dir, &[command, "--extensions", &list, "a.crv"]);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{command} {list}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn c2_t_010_extension_options_refusals_are_usage_errors() {
    let dir = dir("refusals");
    std::fs::write(dir.join("a.crv"), "a\n").unwrap();
    for (extensions, options) in [
        ("\"carve.tabs\"", None),
        ("[1]", None),
        // The text leaves open options for an extension `--extensions` does not list.
        ("[]", Some(r#"{"carve.tabs":{"mode":"aria"}}"#)),
        (
            r#"["carve.tabs"]"#,
            Some(r#"{"carve.tabs":{"mode":"tabs"}}"#),
        ),
        (
            r#"["carve.tabs"]"#,
            Some(r#"{"carve.tabs":{"size":"css"}}"#),
        ),
        (
            r#"["carve.details"]"#,
            Some(r#"{"carve.details":{"mode":"aria"}}"#),
        ),
        (r#"["carve.tabs"]"#, Some(r#"{"carve.tabs":"aria"}"#)),
        (r#"["carve.tabs"]"#, Some("[]")),
    ] {
        let mut args = vec!["render", "--extensions", extensions];
        if let Some(options) = options {
            args.extend(["--extension-options", options]);
        }
        args.push("a.crv");
        let out = carve(&dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("carve[usage]:"),
            "{args:?}"
        );
    }
}

#[test]
fn c2_t_010_ast_reads_the_standard_matchers() {
    let dir = dir("ast");
    std::fs::write(dir.join("a.crv"), "::: details \"S\"\nx\n:::\n").unwrap();
    let plain = carve(&dir, &["ast", "a.crv"]);
    let with = carve(
        &dir,
        &["ast", "--extensions", r#"["carve.details"]"#, "a.crv"],
    );
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("carve.details"));
    assert!(String::from_utf8_lossy(&with.stdout).contains(r#""name":"carve.details""#));
}

#[test]
fn c2_t_010_help_lists_the_flags() {
    let out = carve(&std::env::temp_dir(), &["lint", "--help"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("  --extensions json\n"), "{text}");
    assert!(text.contains("  --extension-options json\n"), "{text}");
}
