use std::io::{ErrorKind, Write};
use std::process::{Child, ChildStdin, Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    run_input(args, "`x`{=latex}\n")
}

fn spawn(args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve")
}

/// An argument refusal returns before `carve` ever reads stdin, so this write
/// races the child's exit and gets a closed pipe whenever the child wins. Only
/// that one error is tolerated; a test whose input never arrived still fails on
/// its own assertions about the output.
fn write_input(stdin: &mut ChildStdin, input: &str) {
    match stdin.write_all(input.as_bytes()) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::BrokenPipe => {}
        Err(err) => panic!("write to carve stdin: {err}"),
    }
}

fn run_input(args: &[&str], input: &str) -> Output {
    let mut child = spawn(args);
    let mut stdin = child.stdin.take().expect("piped stdin");
    write_input(&mut stdin, input);
    drop(stdin);
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

/// CARVE-P12-034 sends a dropped section attributes field to the PART 11 §1d
/// channel, and CARVE-P11-046 keeps the render-loss flags out of it: a table
/// carrying section attributes is no render loss, so `--strict-losses` passes it
/// and `--allow-loss` never grew a name for it.
#[test]
fn table_section_attributes_are_not_a_render_loss() {
    let ast = r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[],"rowGroups":{"headRows":0,"footRows":0,"bodies":[],"headAttrs":{"id":"head"}}}]}"#;
    let path =
        std::env::temp_dir().join(format!("carve-section-attrs-{}.json", std::process::id()));
    let strict = run_input(
        &[
            "--from-json",
            "--plain",
            "--strict-losses",
            "--report-losses",
            path.to_str().unwrap(),
        ],
        ast,
    );
    assert!(strict.status.success(), "{:?}", strict.stderr);
    assert!(strict.stderr.is_empty(), "{:?}", strict.stderr);
    let report = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(report.contains("\"losses\":[]"), "{report}");
    assert!(report.contains("\"totalLosses\":0"), "{report}");
}

/// The render-loss vocabulary is closed at the two codes of CARVE-P2-024, so
/// `--allow-loss` offers those two names and refuses every other.
#[test]
fn allow_loss_offers_exactly_the_two_closed_codes() {
    for code in ["raw-format-dropped", "ruby-flattened"] {
        let accepted = run_input(&["--plain", "--allow-loss", code], "text\n");
        assert!(accepted.status.success(), "{code}: {:?}", accepted.stderr);
    }
    let refused = run_input(
        &[
            "--plain",
            "--allow-loss",
            "table-section-attributes-dropped",
        ],
        "text\n",
    );
    assert_eq!(refused.status.code(), Some(2), "{:?}", refused.stderr);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(
        stderr.trim_end(),
        "carve: --allow-loss expects raw-format-dropped or ruby-flattened"
    );
    let help = run_input(&["--help"], "");
    let help = String::from_utf8_lossy(&help.stdout);
    let line = help
        .lines()
        .find(|line| line.contains("--allow-loss"))
        .expect("--help documents --allow-loss");
    assert!(!line.contains("table-section-attributes-dropped"), "{line}");
    assert!(line.contains("raw-format-dropped"), "{line}");
    assert!(line.contains("ruby-flattened"), "{line}");
}

/// `--allow-loss` refuses an unknown code before `carve` reads stdin, so the
/// helper's write races the child's exit. This closes the race the other way
/// round - wait for the refusal first - so the closed pipe is certain rather
/// than occasional, and pins that the helper survives it.
#[test]
fn the_helper_survives_a_child_that_exits_before_reading_stdin() {
    let mut child = spawn(&[
        "--plain",
        "--allow-loss",
        "table-section-attributes-dropped",
    ]);
    let mut stdin = child.stdin.take().expect("piped stdin");
    assert_eq!(child.wait().unwrap().code(), Some(2));
    assert_eq!(
        stdin.write_all(b"text\n").unwrap_err().kind(),
        ErrorKind::BrokenPipe,
        "the refused child left its stdin readable, so this test pins nothing"
    );
    write_input(&mut stdin, "text\n");
}
