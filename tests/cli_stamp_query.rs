//! CLI integration: `--stamp-info` and `--stamp-check`.
//!
//! Output and exit codes match carve-php and carve-js, because a provenance
//! marker is only worth recording if another engine can read it. The markers
//! below are the literal bytes those engines write.

use std::io::Write;
use std::process::{Command, Stdio};

/// Run the binary with `args` and `input` on stdin; return (stdout, stderr, code).
fn run(args: &[&str], input: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait carve binary");
    (
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
        out.status.code().unwrap_or(-1),
    )
}

const FROM_PHP: &str = "# Hi\n\n%% carve-version: 0.1; generated-by: carve-php 0.1.0\n";
const FROM_JS: &str = "# Hi\n\n%% carve-version: 0.1; generated-by: carve-js 0.1.0\n";
const OLD: &str = "# Hi\n\n%% carve-version: 0.0.9; generated-by: carve-rs 0.0.9\n";

#[test]
fn stamp_info_reports_a_marker_and_exits_zero() {
    let (out, _, code) = run(&["--stamp-info"], FROM_PHP);
    assert_eq!(code, 0);
    assert!(out.contains("carve-version: 0.1"), "{out}");
    assert!(out.contains("generated-by: carve-php 0.1.0"), "{out}");
    assert!(out.contains("this engine targets: 0.2"), "{out}");
}

#[test]
fn stamp_info_reads_a_carve_js_marker_too() {
    let (out, _, code) = run(&["--stamp-info"], FROM_JS);
    assert_eq!(code, 0);
    assert!(out.contains("generated-by: carve-js 0.1.0"), "{out}");
}

#[test]
fn stamp_info_says_so_when_there_is_no_marker() {
    let (out, _, code) = run(&["--stamp-info"], "# Hi\n");
    assert_eq!(code, 0);
    assert!(out.contains("unstamped"), "{out}");
}

#[test]
fn stamp_check_exits_one_for_an_older_or_unknown_document() {
    let (_, err, code) = run(&["--stamp-check"], OLD);
    assert_eq!(code, 1);
    assert!(err.contains("[behavior]"), "{err}");

    let (_, _, code) = run(&["--stamp-check"], "# Hi\n");
    assert_eq!(code, 1);
}

#[test]
fn stamp_check_exits_zero_for_a_current_document() {
    let current = FROM_PHP.replace("carve-version: 0.1", "carve-version: 0.2");
    let (_, err, code) = run(&["--stamp-check"], &current);
    assert_eq!(code, 0);
    assert_eq!(err, "");
}

#[test]
fn the_stamp_modes_render_nothing_whatever_format_is_requested() {
    // They answer a question ABOUT the document. If they also rendered, piping
    // --stamp-check into a file would silently write markup.
    for format in [
        vec!["--stamp-info"],
        vec!["--markdown", "--stamp-info"],
        vec!["--ansi", "--stamp-info"],
        vec!["--carve", "--stamp-info"],
    ] {
        let (out, _, code) = run(&format, OLD);
        assert_eq!(code, 0);
        assert!(!out.contains("<h1"), "{out}");
        assert!(!out.contains("Hi"), "{out}");
        assert!(out.contains("carve-version: 0.0.9"), "{out}");
    }
}
