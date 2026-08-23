//! The `carve lint` subcommand.
//!
//! The exit code is the whole interface for a CI gate, so every path through it
//! is asserted here rather than only the happy one. The three-way split matters:
//! **0** clean, **1** findings, **2** could not run. A gate that collapsed 2 into
//! 1 would report an unreadable file as a lint failure; one that collapsed it
//! into 0 would pass a build whose documents were never read.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

/// A `{#id .cls}` above a blank line attaches to nothing, so the id and the
/// class vanish and nothing anywhere says so - the clearest instance of the
/// silent defect class this subcommand exists to surface.
const ORPHAN: &str = "{#orphan .cls}\n\n";

fn fixture(contents: &str) -> std::path::PathBuf {
    let index = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("carve-lint-{}-{index}.crv", std::process::id()));
    fs::write(&path, contents).unwrap();
    path
}

fn lint(args: &[&std::ffi::OsStr]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .arg("lint")
        .args(args)
        .output()
        .expect("carve lint runs");
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
        output.status.code().unwrap_or(-1),
    )
}

fn lint_path(path: &std::path::Path) -> (String, String, i32) {
    lint(&[path.as_os_str()])
}

#[test]
fn a_clean_document_reports_nothing_and_exits_zero() {
    let path = fixture("# Title\n\nA paragraph.\n");
    let (stdout, _stderr, code) = lint_path(&path);
    assert_eq!(stdout, "", "a clean document produced output");
    assert_eq!(code, 0);
}

#[test]
fn an_unattached_block_attribute_is_reported_and_exits_one() {
    let path = fixture(ORPHAN);
    let (stdout, _stderr, code) = lint_path(&path);
    assert!(
        stdout.contains("unattached-block-attribute"),
        "expected the rule id, got {stdout:?}"
    );
    assert_eq!(code, 1, "findings must exit 1 so a CI gate fails");
}

#[test]
fn the_line_carries_file_line_column_and_rule() {
    // The format is carve-js's, exactly: `path:line:col rule — message`. A
    // script that parses one CLI has to parse the other, which is what makes
    // this a contract rather than a formatting preference.
    let path = fixture(ORPHAN);
    let (stdout, _stderr, _code) = lint_path(&path);
    let line = stdout.lines().next().expect("one warning line");
    let prefix = format!("{}:1:1 unattached-block-attribute — ", path.display());
    assert!(
        line.starts_with(&prefix),
        "expected {prefix:?} at the start of {line:?}"
    );
    assert!(
        line.len() > prefix.len(),
        "the message after the separator is empty"
    );
}

#[test]
fn stdin_is_read_when_no_path_is_given() {
    for args in [vec![], vec!["-"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
            .arg("lint")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("carve lint spawns");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(ORPHAN.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.starts_with("<stdin>:1:1 unattached-block-attribute"),
            "args {args:?} gave {stdout:?}"
        );
        assert_eq!(output.status.code(), Some(1));
    }
}

#[test]
fn an_unreadable_path_exits_two_not_one() {
    // Distinct from a finding on purpose: "could not run" and "ran and found
    // problems" are different answers, and a gate needs to tell them apart.
    let missing = std::env::temp_dir().join("carve-lint-does-not-exist.crv");
    let _ = fs::remove_file(&missing);
    let (stdout, stderr, code) = lint(&[missing.as_os_str()]);
    assert_eq!(stdout, "");
    assert!(stderr.contains("cannot read"), "got {stderr:?}");
    assert_eq!(code, 2);
}

#[test]
fn one_unreadable_path_does_not_stop_the_others() {
    // The whole point of accepting several paths is running over a tree, so a
    // single bad entry must not hide every other document's findings.
    let good = fixture(ORPHAN);
    let missing = std::env::temp_dir().join("carve-lint-also-missing.crv");
    let _ = fs::remove_file(&missing);
    let (stdout, stderr, code) = lint(&[good.as_os_str(), missing.as_os_str()]);
    assert!(
        stdout.contains("unattached-block-attribute"),
        "the readable file was skipped: {stdout:?}"
    );
    assert!(stderr.contains("cannot read"));
    assert_eq!(code, 2, "an unreadable path outranks findings");
}

#[test]
fn several_files_each_report_under_their_own_name() {
    let first = fixture(ORPHAN);
    let second = fixture(ORPHAN);
    let (stdout, _stderr, code) = lint(&[first.as_os_str(), second.as_os_str()]);
    assert!(stdout.contains(&format!("{}:1:1", first.display())));
    assert!(stdout.contains(&format!("{}:1:1", second.display())));
    assert_eq!(stdout.lines().count(), 2);
    assert_eq!(code, 1);
}

#[test]
fn extensions_reaches_the_linter() {
    // `--extensions` is the only render flag the linter reads, which is why it
    // is the only one plumbed through. Asserted by a document whose warning
    // exists either way: the flag must not CHANGE this result, and must not be
    // rejected as an unknown option.
    let path = fixture(ORPHAN);
    let (plain, _, plain_code) = lint_path(&path);
    let (with_ext, _, ext_code) = lint(&[std::ffi::OsStr::new("--extensions"), path.as_os_str()]);
    assert_eq!(plain, with_ext);
    assert_eq!(plain_code, 1);
    assert_eq!(ext_code, 1);
}

#[test]
fn lint_does_not_render() {
    // A subcommand that fell through to the render path would print HTML and
    // exit 0, which looks like a clean document.
    let path = fixture("# Title\n");
    let (stdout, _stderr, code) = lint_path(&path);
    assert!(!stdout.contains("<h1"), "lint rendered HTML: {stdout:?}");
    assert_eq!(stdout, "");
    assert_eq!(code, 0);
}

#[test]
fn an_unknown_option_exits_two_not_one() {
    // Found by review, and the failure was silent in the worst way: `lint`
    // shared the render loop's argument parsing, so an unknown option returned
    // 1 - which a CI gate reads as "found problems" rather than "could not
    // run", and a typo in a pipeline would look like a lint failure forever.
    let (stdout, stderr, code) = lint(&[std::ffi::OsStr::new("--bogus")]);
    assert_eq!(stdout, "");
    assert!(stderr.contains("unknown option"), "got {stderr:?}");
    assert_eq!(code, 2);
}

#[test]
fn a_render_only_flag_is_refused_rather_than_ignored() {
    // The same shared-parser defect, in its quieter form: `--static` was
    // ACCEPTED and dropped, so the command exited 0 having linted with a flag
    // the caller believed was doing something.
    let path = fixture(ORPHAN);
    for flag in ["--static", "--profile", "--html", "--markdown"] {
        let (stdout, stderr, code) = lint(&[std::ffi::OsStr::new(flag), path.as_os_str()]);
        assert_eq!(stdout, "", "{flag} produced output");
        assert!(stderr.contains("unknown option"), "{flag}: {stderr:?}");
        assert_eq!(code, 2, "{flag} was not refused");
    }
}

#[test]
fn help_exits_zero_and_names_the_exit_codes() {
    // A three-way exit contract nobody can read is a contract nobody honours.
    let (stdout, _stderr, code) = lint(&[std::ffi::OsStr::new("--help")]);
    assert_eq!(code, 0);
    for expected in ["--extensions", "0", "1", "2"] {
        assert!(stdout.contains(expected), "help omits {expected:?}");
    }
}

#[test]
fn the_help_text_names_the_subcommand() {
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .arg("--help")
        .output()
        .expect("carve --help runs");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("carve lint"),
        "--help does not mention lint, so nobody can find it"
    );
}
