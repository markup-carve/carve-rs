//! `render --quote-locale` (carve-rs#4838): the 0.1 engine flag for the
//! `locale` of `carve.smartQuotes`.
use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> (i32, String, String) {
    let binary = std::env::var("CARVE_CLI_TEST_BIN")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_carve").to_string());
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A usage error exits before reading stdin, so the write may hit a closed pipe.
    if let Err(err) = child.stdin.take().unwrap().write_all(input.as_bytes()) {
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe, "{err}");
    }
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn the_locale_gives_its_quote_glyphs() {
    assert_eq!(
        run(&["render", "--quote-locale", "de", "-"], "\"a\" b\n"),
        (0, "<p>„a“ b</p>\n".into(), String::new())
    );
    assert_eq!(
        run(&["render", "--quote-locale=fr-CA", "-"], "\"a\" 'b'\n").1,
        "<p>«\u{a0}a\u{a0}» ‹\u{a0}b\u{a0}›</p>\n".replace('\u{a0}', "&nbsp;")
    );
}

#[test]
fn an_unknown_locale_falls_back_to_english() {
    assert_eq!(
        run(&["render", "--quote-locale", "xx", "-"], "\"a\" b\n").1,
        "<p>“a” b</p>\n"
    );
}

#[test]
fn smart_typography_off_keeps_straight_quotes() {
    assert_eq!(
        run(
            &[
                "render",
                "--quote-locale",
                "de",
                "--no-smart-typography",
                "-"
            ],
            "\"a\" b\n"
        )
        .1,
        "<p>\"a\" b</p>\n"
    );
}

#[test]
fn a_missing_locale_and_an_export_target_are_usage_errors() {
    let (status, stdout, stderr) = run(&["render", "--quote-locale"], "\"a\"\n");
    assert_eq!((status, stdout.as_str()), (2, ""));
    assert!(stderr.starts_with("carve[usage]:"), "{stderr}");
    let (status, _, stderr) = run(
        &["render", "--quote-locale", "de", "--to", "markdown", "-"],
        "\"a\"\n",
    );
    assert_eq!(status, 2);
    assert!(stderr.starts_with("carve[usage]:"), "{stderr}");
    let (status, _, _) = run(
        &["render", "--quote-locale", "de", "--to", "carve", "-"],
        "\"a\"\n",
    );
    assert_eq!(status, 2);
    assert_eq!(
        run(
            &["render", "--quote-locale", "de", "--to", "html", "-"],
            "\"a\"\n"
        )
        .1,
        "<p>„a“</p>\n"
    );
    let (status, _, _) = run(&["ast", "--quote-locale", "de", "-"], "\"a\"\n");
    assert_eq!(status, 2);
}
