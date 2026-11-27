use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
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

#[test]
fn fix_and_lint_fix_report_applied_rules_only() {
    for command in ["fix", "lint"] {
        let args = if command == "fix" {
            vec!["fix", "-"]
        } else {
            vec!["lint", "--fix", "-"]
        };
        assert_eq!(
            run(&args, "**bold**\n"),
            (
                0,
                "*bold*\n".into(),
                "<stdin>: fixed markdown-strong-double-star 1\n".into()
            )
        );
        assert_eq!(run(&args, "plain\n"), (0, "plain\n".into(), String::new()));
    }
    assert_eq!(
        run(&["fix", "-"], "**one** **two**\n"),
        (
            0,
            "*one* *two*\n".into(),
            "<stdin>: fixed markdown-strong-double-star 2\n".into()
        )
    );
    assert_eq!(
        run(&["fix", "--upgrade", "-"], "a **b**\n\n# T ##\n"),
        (
            0,
            "a *b*\n\n# T\n".into(),
            "<stdin>: fixed markdown-strong-double-star 1, heading-closing-hashes 1\n".into()
        )
    );
}

#[test]
fn write_mode_reports_the_passed_file_name() {
    let path = std::env::temp_dir().join(format!("carve-fix-count-{}.crv", std::process::id()));
    std::fs::write(&path, "**bold**\n").unwrap();
    let name = path.to_str().unwrap();
    assert_eq!(
        run(&["fix", "--write", name], ""),
        (
            0,
            String::new(),
            format!("{name}: fixed markdown-strong-double-star 1\n")
        )
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "*bold*\n");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn failed_write_does_not_report_a_fix() {
    let path =
        std::env::temp_dir().join(format!("carve-fix-unwritable-{}.crv", std::process::id()));
    std::fs::write(&path, "**bold**\n").unwrap();
    let original_permissions = std::fs::metadata(&path).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    let (status, stdout, stderr) = run(&["fix", "--write", path.to_str().unwrap()], "");
    assert_eq!(status, 2);
    assert_eq!(stdout, "");
    assert!(stderr.contains("carve[output-unwritable]:"), "{stderr}");
    assert!(!stderr.contains(": fixed "), "{stderr}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "**bold**\n");
    std::fs::set_permissions(&path, original_permissions).unwrap();
    std::fs::remove_file(path).unwrap();
}
