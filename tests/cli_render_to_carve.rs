//! Source writing belongs to `fmt`; the removed render target is a usage error.
use std::io::Write;
use std::process::{Command, Stdio};

/// A head with no rows, which no source spells (C2-W-011).
const EMPTY_HEAD: &str = r#"{"type":"document","astVersion":"2.0","children":[{"type":"table","columns":[{"type":"table_column"}],"children":[{"type":"table_head","children":[]},{"type":"table_body","children":[{"type":"table_row","children":[{"type":"table_cell","header":false,"children":[{"type":"cell_paragraph","children":[{"type":"text","value":"H"}]}]}]}]}]}]}"#;
const REFUSAL: &str = "carve[writer-refused]: writer carve: the source cannot be returned without its loss report: structure-unspellable at /children/0/children/0\n";

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
    // The binary may exit before reading stdin (usage error), closing the pipe.
    if let Err(err) = child.stdin.take().unwrap().write_all(input.as_bytes()) {
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe, "{err}");
    }
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn fmt_from_ast_refuses_a_writer_loss() {
    let (status, stdout, stderr) = run(&["fmt", "--from-ast", "-"], EMPTY_HEAD);
    assert_eq!((status, stdout.as_str(), stderr.as_str()), (1, "", REFUSAL));
}

#[test]
fn fmt_from_ast_writes_a_clean_tree() {
    let (status, tree, stderr) = run(&["ast", "--no-positions", "-"], "| a |\n");
    assert_eq!((status, stderr.as_str()), (0, ""));
    let (status, stdout, stderr) = run(&["fmt", "--from-ast", "-"], &tree);
    assert_eq!(
        (status, stdout.as_str(), stderr.as_str()),
        (0, "| a |\n", "")
    );
}

#[test]
fn render_to_carve_is_a_usage_error() {
    let (status, stdout, stderr) = run(&["render", "--to", "carve", "-"], "| a |\n");
    assert_eq!((status, stdout.as_str()), (2, ""));
    assert!(stderr.starts_with("carve[usage]:"), "{stderr}");
    assert!(stderr.contains("carve flatten"), "{stderr}");
}
