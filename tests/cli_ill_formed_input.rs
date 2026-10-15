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

#[test]
fn fmt_refuses_to_rewrite_ill_formed_file() {
    let path = std::env::temp_dir().join(format!(
        "carve-ill-formed-fmt-{}-{}.crv",
        std::process::id(),
        line!()
    ));
    std::fs::write(&path, SOURCE).unwrap();
    for mode in ["--check", "--write"] {
        let output = Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(["fmt", mode])
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{mode}");
        assert_eq!(std::fs::read(&path).unwrap(), SOURCE, "{mode}");
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn fmt_prints_an_ill_formed_file_with_replacements() {
    let path = std::env::temp_dir().join(format!(
        "carve-ill-formed-fmt-print-{}-{}.crv",
        std::process::id(),
        line!()
    ));
    std::fs::write(&path, SOURCE).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .arg("fmt")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "a\u{fffd}\u{fffd}\u{fffd}b *x*\n"
    );
}

/// One U+FFFD per maximal ill-formed subpart, the WHATWG decoder's count.
#[test]
fn each_maximal_ill_formed_subpart_becomes_one_replacement() {
    for (bytes, count) in [
        (&b"\xed\xa0\x80"[..], 3),
        (b"\xff", 1),
        (b"\xe2\x82", 1),
        (b"\xed\xa0\xbd\xed\xb8\x80", 6),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
            .arg("--plain")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn carve");
        let mut source = b"a".to_vec();
        source.extend_from_slice(bytes);
        source.extend_from_slice(b"b\n");
        child.stdin.take().unwrap().write_all(&source).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{bytes:02x?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            text,
            format!("a{}b\n", "\u{fffd}".repeat(count)),
            "{bytes:02x?}"
        );
    }
}
