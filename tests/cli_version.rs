use std::process::Command;

fn carve(args: &[&str]) -> (String, String, Option<i32>) {
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .output()
        .expect("carve starts");
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
        output.status.code(),
    )
}

#[test]
fn version_flags_print_the_crate_version() {
    let expected = format!("carve-rs {}\n", env!("CARGO_PKG_VERSION"));
    for args in [
        &["--version"][..],
        &["-V"],
        &["fmt", "--version"],
        &["--static", "-V"],
    ] {
        let (stdout, stderr, code) = carve(args);
        assert_eq!(code, Some(0), "{args:?} stderr: {stderr}");
        assert_eq!(stdout, expected, "{args:?}");
        assert_eq!(stderr, "", "{args:?}");
    }
}

#[test]
fn help_lists_the_version_flag() {
    let (stdout, _stderr, code) = carve(&["--help"]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("-V, --version"), "help omits --version");
}

// carve-rs#2901, #2902: `{{` in the help format string collapsed to `{`, and
// lint's include root has no default, unlike render's.
#[test]
fn help_documents_the_include_root_for_render_and_lint() {
    let (stdout, _stderr, code) = carve(&["--help"]);
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("containment root for {{ path }} includes"),
        "{stdout}"
    );
    assert!(
        stdout.contains("carve lint [--include-root DIR] [files]"),
        "{stdout}"
    );
    assert!(stdout.contains("carve lint has no default"), "{stdout}");
    for line in stdout.lines() {
        assert!(
            line.is_empty()
                || line.starts_with(' ')
                || line.ends_with(':')
                || line.starts_with("carve")
                || line.starts_with("Spec:"),
            "help line is not indented: {line:?}"
        );
        assert!(
            !line.starts_with(&" ".repeat(31)),
            "help line is over-indented: {line:?}"
        );
    }
    let (lint, _stderr, code) = carve(&["lint", "--help"]);
    assert_eq!(code, Some(0));
    assert!(
        lint.contains("without it no include target is read"),
        "{lint}"
    );
}
