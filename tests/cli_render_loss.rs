use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"`x`{=latex}\n")
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn normal_mode_keeps_stdout_pure_and_warns_with_a_position() {
    let output = run(&["--html"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "<p></p>\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("<stdin>:1:1 raw-format-dropped"));
}

#[test]
fn strict_mode_refuses_before_stdout() {
    let output = run(&["--strict-losses"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn allow_loss_makes_the_intent_explicit() {
    let output = run(&["--strict-losses", "--allow-loss", "raw-format-dropped"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}

#[test]
fn report_is_machine_readable_and_bounded() {
    let path = std::env::temp_dir().join(format!("carve-render-loss-{}.json", std::process::id()));
    let output = run(&[
        "--report-losses",
        path.to_str().unwrap(),
        "--max-render-losses",
        "0",
    ]);
    assert!(output.status.success());
    let report = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(report.contains("\"totalLosses\":1"));
    assert!(report.contains("\"truncated\":true"));
    assert!(report.contains("\"losses\":[]"));
}
