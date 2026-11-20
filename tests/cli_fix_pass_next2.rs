use std::io::Write;
use std::process::{Command, Stdio};

fn fix(source: &str) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["fix", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

#[test]
fn fix_terminates_and_keeps_invalid_caret_fragments() {
    for source in ["{^{^^", "{^{^^\n", "{^{^^x\n", "a{^{^^b\n", "{^{^x^\n"] {
        assert_eq!(fix(source).1, source);
    }
    assert_eq!(fix("^valid^\n").1, "{^valid^}\n");
    assert_eq!(fix("^a{b}^\n").1, "{^a{b}^}\n");
}
