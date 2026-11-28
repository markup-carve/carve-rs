//! CLI defaults and JSON-tree stdin diagnostics (carve-rs#4969, #4970).
use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn labels_keep_omitted_defaults_and_override_given_keys() {
    let input = b"::: tip\ntext\n:::\n";
    for (labels, expected) in [("{}", "Tip"), ("{\"admonitionTip\":\"Advice\"}", "Advice")] {
        let (status, stdout, stderr) = run(&["render", "--labels", labels], input);
        assert_eq!(status, 0, "{stderr}");
        assert!(
            stdout.contains(&format!("aria-label=\"{expected}\"")),
            "{stdout}"
        );
    }
}

#[test]
fn invalid_json_tree_stdin_uses_diagnostic_name() {
    for args in [
        &["migrate-ast"][..],
        &["render", "--from-ast"][..],
        &["fmt", "--from-ast"][..],
    ] {
        let (status, stdout, stderr) = run(args, b"\xff");
        assert_eq!(status, 2, "{args:?}: {stderr}");
        assert!(stdout.is_empty(), "{args:?}: {stdout}");
        assert_eq!(
            stderr,
            "carve[input-unreadable]: <stdin>: a tree in JSON is not UTF-8\n"
        );
    }
}
