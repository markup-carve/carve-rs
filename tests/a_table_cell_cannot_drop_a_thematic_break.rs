use carve::{from_json, render_carve, RenderCarveError};
use std::io::Write;
use std::process::{Command, Stdio};

const REASON: &str = "a thematic break has no inline spelling in a table cell";

fn document(cells: &str) -> String {
    format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"table","rows":[{{"type":"table_row","cells":[{cells}]}}]}}]}}"#
    )
}

fn cell(blocks: &str) -> String {
    format!(r#"{{"type":"table_cell","header":false,"blocks":[{blocks}]}}"#)
}

fn paragraph(value: &str) -> String {
    format!(r#"{{"type":"paragraph","children":[{{"type":"text","value":"{value}"}}]}}"#)
}

fn assert_refused(json: &str) {
    let doc = from_json(json).expect("valid AST JSON");
    let err = render_carve(&doc).expect_err("thematic break must be refused");
    let RenderCarveError::SourceUnspellable(refusal) = err else {
        panic!("unexpected error: {err}");
    };
    assert_eq!(refusal.node_type(), "thematic_break");
    assert_eq!(refusal.reason(), REASON);

    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["--from-json", "--carve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run carve");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(json.as_bytes())
        .unwrap();
    let output = child.wait_with_output().expect("wait for carve");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&format!("cannot spell thematic_break: {REASON}")),
        "{stderr}"
    );
    assert!(!stderr.contains("stack backtrace"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn a_lone_thematic_break_is_refused() {
    assert_refused(&document(&cell(r#"{"type":"thematic_break"}"#)));
}

#[test]
fn a_thematic_break_inside_a_block_quote_is_refused() {
    let blocks = r#"{"type":"block_quote","children":[{"type":"thematic_break"}]}"#;
    assert_refused(&document(&cell(blocks)));
}

#[test]
fn a_break_between_paragraphs_is_refused() {
    let blocks = format!(
        r#"{},{{"type":"thematic_break"}},{}"#,
        paragraph("before"),
        paragraph("after")
    );
    assert_refused(&document(&cell(&blocks)));
}

#[test]
fn a_break_beside_a_nonempty_cell_is_refused() {
    let cells = format!(
        "{},{}",
        cell(r#"{"type":"thematic_break"}"#),
        cell(&paragraph("other"))
    );
    assert_refused(&document(&cells));
}
