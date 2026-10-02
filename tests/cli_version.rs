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
