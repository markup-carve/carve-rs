//! CLI integration: `carve migrate --from` reaches every importer the crate
//! ships, not only the HTML one the subcommand started with. `djot_to_carve`
//! was library-only, so the sole way to run it was to link the crate.

use std::io::Write;
use std::process::{Command, Stdio};

/// Run `carve` with `args`, feeding `input` on stdin; returns (stdout, stderr,
/// success).
fn run(args: &[&str], input: &str) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    // A BROKEN PIPE HERE IS NOT A FAILURE. A run that rejects its arguments
    // prints usage and exits WITHOUT reading stdin, so this write races that
    // exit: it lands in the pipe buffer when the child is still alive and hits
    // a closed pipe when it is not. Both are the behavior under test, and
    // panicking on the second made `rejects_an_unknown_source_format` and
    // `names_every_source_format_when_from_is_missing` fail intermittently on
    // main. The exit status and stderr are the assertions; delivery of stdin
    // to a process that does not want it is not.
    let mut stdin = child.stdin.take().expect("stdin");
    match stdin.write_all(input.as_bytes()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => panic!("write stdin: {error}"),
    }
    // Explicit, because the child waits on EOF and the borrow above no longer
    // ends the moment the write does.
    drop(stdin);
    let out = child.wait_with_output().expect("wait carve binary");
    (
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
        out.status.success(),
    )
}

/// The same run, reported as the EXIT CODE rather than as a boolean.
///
/// `success()` cannot see this file's subject: every code below is non-zero, and
/// which non-zero code it is is the whole question. The four assertions that
/// existed before this only checked `!ok`, which is why exit 1 sat on the usage
/// paths unnoticed.
fn exit_code(args: &[&str], input: &str) -> i32 {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    let mut stdin = child.stdin.take().expect("stdin");
    match stdin.write_all(input.as_bytes()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => panic!("write stdin: {error}"),
    }
    drop(stdin);
    let out = child.wait_with_output().expect("wait carve binary");
    out.status
        .code()
        .expect("the process exits rather than signalling")
}

#[test]
fn migrates_djot() {
    let (out, err, ok) = run(&["migrate", "--from", "djot"], "*bold* and _em_\n");
    assert!(ok, "djot migration should succeed: {err}");
    assert!(out.contains("*bold* and /em/"), "unexpected output: {out}");
}

#[test]
fn migrates_markdown_under_both_names() {
    for name in ["markdown", "md"] {
        let (out, err, ok) = run(&["migrate", "--from", name], "**bold** and _em_\n");
        assert!(ok, "--from {name} should succeed: {err}");
        assert!(out.contains("*bold* and /em/"), "--from {name}: {out}");
    }
}

#[test]
fn migrates_html() {
    let (out, _, ok) = run(&["migrate", "--from", "html"], "<p><b>bold</b></p>");
    assert!(ok, "html migration should succeed");
    assert!(out.contains("*bold*"), "unexpected output: {out}");
}

#[test]
fn rejects_an_unknown_source_format() {
    let (_, err, ok) = run(&["migrate", "--from", "rst"], "x");
    assert!(!ok, "an unknown format should fail");
    assert!(
        err.contains("unknown source format rst"),
        "unexpected stderr: {err}"
    );
}

#[test]
fn names_every_source_format_when_from_is_missing() {
    let (_, err, ok) = run(&["migrate"], "x");
    assert!(!ok, "a missing --from should fail");
    assert!(
        err.contains("html, markdown or djot"),
        "unexpected stderr: {err}"
    );
}

/// The loss report is the HTML importer's alone: Markdown and Djot each parse
/// their source whole and have nothing to report as dropped, so a migration
/// from either ignores those options rather than failing on them.
#[test]
fn ignores_the_html_only_options_for_other_formats() {
    let (out, _, ok) = run(
        &["migrate", "--from", "djot", "--check-loss", "--report", "-"],
        "*bold*\n",
    );
    assert!(ok, "html-only options should not fail a djot migration");
    assert_eq!(out, "*bold*\n");
}

// ------------------------------------------------------- exit codes (#1276)

/// A USAGE ERROR EXITS 2, NOT 1.
///
/// Exit 1 on this subcommand already means something else: `--check-loss` uses
/// it for a run that CONVERTED THE DOCUMENT and found the importer had dropped
/// content. With usage errors on 1 as well, the gate
///
/// ```text
/// carve migrate --from html --check-loss in.html > out.crv || echo "content was dropped"
/// ```
///
/// printed "content was dropped" when the real problem was a typo in a flag
/// name, and nothing told the operator the conversion had never run.
///
/// carve-js and carve-php both exit 2 for every row here, and `carve lint` and
/// `carve merge` in this same binary already do - only `migrate` was out of
/// step.
#[test]
fn a_usage_error_exits_two() {
    for args in [
        // An unknown source format.
        &["migrate", "--from", "rst"][..],
        // No --from at all.
        &["migrate"][..],
        // A flag whose VALUE is not in the vocabulary.
        &["migrate", "--from", "html", "--mode", "bogus"][..],
        &["migrate", "--from", "html", "--adapter", "bogus"][..],
        // A flag whose value is MISSING entirely.
        &["migrate", "--from", "html", "--mode"][..],
        &["migrate", "--from", "html", "--adapter"][..],
        // A flag this subcommand does not have.
        &["migrate", "--from", "html", "--bogusflag"][..],
        // More input files than it takes.
        &["migrate", "--from", "html", "a.html", "b.html"][..],
    ] {
        assert_eq!(
            exit_code(args, "x\n"),
            2,
            "expected a usage exit for {args:?}"
        );
    }
}

/// An unreadable input is "could not run" too, so it is 2 as well.
///
/// `carve lint` already draws the line here - its help says "exit 1 on findings,
/// 2 if a file cannot be read" - and carve-js exits 2 for this path.
#[test]
fn an_unreadable_input_exits_two() {
    let missing = "does-not-exist-8f3a12c9.html";
    assert_eq!(exit_code(&["migrate", "--from", "html", missing], ""), 2);
}

/// AND EXIT 1 STILL MEANS LOSS, which is the half that must not move.
///
/// A test that only pinned 2 on the usage paths would pass just as well if the
/// loss signal had been renumbered along with them, and then the codes would
/// still be indistinguishable - just at a different number.
#[test]
fn check_loss_still_exits_one_for_loss_and_zero_without_it() {
    let lossy = "<p>a<marquee>x</marquee></p>\n";
    assert_eq!(
        exit_code(&["migrate", "--from", "html", "--check-loss"], lossy),
        1,
        "a lossy conversion reports loss with 1"
    );
    assert_eq!(
        exit_code(
            &["migrate", "--from", "html", "--check-loss"],
            "<p>plain</p>\n"
        ),
        0,
        "a lossless conversion is a clean exit"
    );
    // Without the flag the same lossy document is a normal success: the flag is
    // what turns the report into an exit status.
    assert_eq!(exit_code(&["migrate", "--from", "html"], lossy), 0);
}

/// The two codes must be DIFFERENT, stated as its own row.
///
/// This is the defect in one line, and it is the assertion that fails no matter
/// which side someone later moves.
#[test]
fn a_usage_error_is_distinguishable_from_reported_loss() {
    let bad_flag = exit_code(
        &["migrate", "--from", "html", "--check-loss", "--bogusflag"],
        "",
    );
    let loss = exit_code(
        &["migrate", "--from", "html", "--check-loss"],
        "<p>a<marquee>x</marquee></p>\n",
    );
    assert_ne!(
        bad_flag, loss,
        "a caller cannot tell a bad flag ({bad_flag}) from reported loss ({loss})"
    );
}
