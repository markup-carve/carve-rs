use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn german_quote_locale_changes_cli_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["--quote-locale", "de"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start carve");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\"Hello\" and 'bye'")
        .unwrap();
    let output = child.wait_with_output().expect("read carve output");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "<p>„Hello“ and ‚bye‘</p>\n"
    );
}
