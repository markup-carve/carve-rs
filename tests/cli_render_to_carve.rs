//! `render --to carve` refuses a writer loss as `fmt --from-ast` does
//! (C2-W-134, C2-T-028).
use std::io::Write;
use std::process::{Command, Stdio};

const HEAD_OF_DATA_CELLS: &str = r#"{"type":"document","astVersion":"2.0","children":[{"type":"table","columns":[{"type":"table_column"}],"children":[{"type":"table_head","children":[{"type":"table_row","children":[{"type":"table_cell","header":false,"children":[{"type":"cell_paragraph","children":[{"type":"text","value":"H"}]}]}]}]}]}]}"#;
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

fn report_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("carve-rs-{}-{name}.json", std::process::id()))
}

fn report(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_writer_loss_refuses_the_source_like_fmt_from_ast() {
    let (status, stdout, stderr) = run(&["fmt", "--from-ast", "-"], HEAD_OF_DATA_CELLS);
    assert_eq!((status, stdout.as_str(), stderr.as_str()), (1, "", REFUSAL));

    let (status, stdout, stderr) = run(
        &["render", "--from-ast", "--to", "carve", "-"],
        HEAD_OF_DATA_CELLS,
    );
    assert_eq!((status, stdout.as_str()), (1, ""));
    assert!(stderr.starts_with(REFUSAL), "{stderr}");
    assert!(stderr.contains("structure-unspellable"), "{stderr}");
}

#[test]
fn report_losses_lists_the_writer_losses() {
    let path = report_path("losses");
    let (status, stdout, _) = run(
        &[
            "render",
            "--from-ast",
            "--to",
            "carve",
            "--report-losses",
            path.to_str().unwrap(),
            "-",
        ],
        HEAD_OF_DATA_CELLS,
    );
    assert_eq!((status, stdout.as_str()), (1, ""));
    let file = &report(&path)["files"][0];
    assert_eq!(file["diagnostics"][0]["code"], "structure-unspellable");
    assert_eq!(file["diagnostics"][0]["path"], "/children/0/children/0");
    assert_eq!(file["suppressed"], 0);
}

#[test]
fn strict_losses_fails_a_writer_loss_as_a_render_loss() {
    let (status, stdout, stderr) = run(
        &[
            "render",
            "--from-ast",
            "--to",
            "carve",
            "--strict-losses",
            "-",
        ],
        HEAD_OF_DATA_CELLS,
    );
    assert_eq!((status, stdout.as_str()), (1, ""));
    assert!(stderr.starts_with("carve[render-loss]: "), "{stderr}");
    assert!(stderr.contains(" error structure-unspellable "), "{stderr}");

    let (status, stdout, stderr) = run(
        &[
            "render",
            "--from-ast",
            "--to",
            "carve",
            "--strict-losses",
            "--max-render-losses",
            "0",
            "-",
        ],
        HEAD_OF_DATA_CELLS,
    );
    assert_eq!((status, stdout.as_str()), (1, ""));
    assert!(stderr.starts_with("carve[render-loss]: "), "{stderr}");
}

#[test]
fn an_allowed_writer_loss_still_refuses_the_source() {
    let path = report_path("allowed");
    let (status, stdout, stderr) = run(
        &[
            "render",
            "--from-ast",
            "--to",
            "carve",
            "--allow-loss",
            "structure-unspellable",
            "--report-losses",
            path.to_str().unwrap(),
            "-",
        ],
        HEAD_OF_DATA_CELLS,
    );
    assert_eq!((status, stdout.as_str()), (1, ""));
    assert!(stderr.starts_with(REFUSAL), "{stderr}");
    assert_eq!(
        report(&path)["files"][0]["diagnostics"],
        serde_json::json!([])
    );
}

#[test]
fn a_tree_without_a_loss_is_written_as_before() {
    let (status, tree, stderr) = run(&["ast", "--no-positions", "-"], "| a |\n");
    assert_eq!((status, stderr.as_str()), (0, ""));
    let path = report_path("clean");
    let (status, stdout, stderr) = run(
        &[
            "render",
            "--from-ast",
            "--to",
            "carve",
            "--strict-losses",
            "--report-losses",
            path.to_str().unwrap(),
            "-",
        ],
        &tree,
    );
    assert_eq!(
        (status, stdout.as_str(), stderr.as_str()),
        (0, "| a |\n", "")
    );
    assert_eq!(
        report(&path)["files"][0]["diagnostics"],
        serde_json::json!([])
    );

    let (status, stdout, stderr) = run(&["render", "--to", "carve", "-"], "| a |\n");
    assert_eq!(
        (status, stdout.as_str(), stderr.as_str()),
        (0, "| a |\n", "")
    );
}
