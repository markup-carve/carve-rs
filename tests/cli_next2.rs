//! C2-T command dispatch and error categories through the public binary.
use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> (i32, String, String) {
    let binary = std::env::var("CARVE_CLI_TEST_BIN")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_carve").to_string());
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn commands_use_the_same_file_and_error_contract() {
    assert_eq!(run(&["fmt", "-"], "a\n"), (0, "a\n".into(), "".into()));
    assert_eq!(
        run(&["import", "--from", "html", "-"], "<p>a</p>"),
        (0, "a\n".into(), "".into())
    );
    let (status, stdout, stderr) = run(&["migrate-ast", "-"], "not json\n");
    assert_eq!((status, stdout), (2, String::new()));
    assert!(stderr.starts_with("carve[ast-json-invalid]:"));
    let (status, stdout, stderr) = run(&["render", "--bogus", "-"], "a\n");
    assert_eq!((status, stdout), (2, String::new()));
    assert!(stderr.starts_with("carve[usage]:"));
}

#[test]
fn ast_and_reports_are_json_values() {
    let (status, stdout, stderr) = run(&["ast", "--no-positions", "--no-includes", "-"], "a\n");
    assert_eq!(status, 0);
    assert_eq!(stderr, "");
    let tree: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tree["astVersion"], "2.0");
    assert_eq!(tree["children"][0]["children"][0]["value"], "a");
    assert!(tree["children"][0].get("pos").is_none());

    let (status, stdout, stderr) = run(&["diagnostics", "--json", "--no-includes", "-"], "a\n");
    assert_eq!(status, 0);
    assert_eq!(stderr, "");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["files"][0]["file"], "<stdin>");
    assert_eq!(report["files"][0]["diagnostics"], serde_json::json!([]));
}
