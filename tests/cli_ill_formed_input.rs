use std::io::Write;
use std::process::{Command, Stdio};

const SOURCE: &[u8] = b"a\xed\xa0\x80b *x*\n";
const EXPECTED: &str = "<p>a\u{fffd}\u{fffd}\u{fffd}b <strong>x</strong></p>\n";

#[test]
fn ill_formed_stdin_is_replaced_before_parsing() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn carve");
    child.stdin.take().unwrap().write_all(SOURCE).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), EXPECTED);
}

#[test]
fn ill_formed_file_is_replaced_before_parsing() {
    let path = std::env::temp_dir().join(format!(
        "carve-ill-formed-{}-{}.crv",
        std::process::id(),
        line!()
    ));
    std::fs::write(&path, SOURCE).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), EXPECTED);
}
