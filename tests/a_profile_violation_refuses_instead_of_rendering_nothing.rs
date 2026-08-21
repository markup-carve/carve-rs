//! CLI integration: a profile violation on the parse-and-render path is a
//! REFUSAL - non-zero exit and a stderr line naming the limit - not an empty
//! stdout with exit 0 (carve-rs#1190).
//!
//! Asserting only on empty stdout would stay green with the bug present, since
//! the bug's symptom IS empty stdout. Every assertion below is on the exit
//! status or on stderr, and the near-miss case pins that the refusal is the cap
//! talking and not the renderer having stopped working.

use std::io::Write;
use std::process::{Command, Output, Stdio};

/// Every output flag whose render path runs the parse + profile pipeline.
/// `--carve` is absent on purpose: `to_carve` takes no options, so no profile
/// reaches that target to be violated.
const PROFILED_FORMATS: &[&str] = &["--html", "--markdown", "--plain", "--ansi", "--json"];

/// `Profile::minimal()`'s `max_length`, spelled out so the test states the
/// number it is about rather than asking the code under test for it.
const MINIMAL_MAX_LENGTH: usize = 10_000;

/// The infallible library wrappers. Convenient for a caller that asked not to
/// handle a violation; never right for the CLI.
const INFALLIBLE_WRAPPERS: &[&str] = &[
    "to_html_with_options",
    "to_markdown_with_options",
    "to_plain_text_with_options",
    "to_ansi_with_options",
    "to_json_with_options",
];

fn run(args: &[&str], input: &str) -> Output {
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
    child.wait_with_output().expect("wait carve binary")
}

#[test]
fn an_over_cap_document_refuses_on_every_render_target() {
    let source = "x".repeat(MINIMAL_MAX_LENGTH + 1);
    for flag in PROFILED_FORMATS {
        let out = run(&[flag, "--profile", "minimal"], &source);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            !out.status.success(),
            "{flag}: an over-cap document exited {:?} - a refusal that reports success \
             is indistinguishable from an empty render",
            out.status
        );
        assert!(
            stderr.contains("max_length_exceeded"),
            "{flag}: stderr does not say why it refused: {stderr:?}"
        );
        assert!(
            stderr.contains(&MINIMAL_MAX_LENGTH.to_string()),
            "{flag}: stderr does not name the limit it enforced: {stderr:?}"
        );
        assert!(
            out.stdout.is_empty(),
            "{flag}: a refused render still wrote {} bytes to stdout",
            out.stdout.len()
        );
    }
}

/// The near miss. A document of exactly `max_length` bytes is under the cap, so
/// it must still render and still exit 0 - otherwise the test above would pass
/// on a build that had simply stopped rendering.
#[test]
fn a_document_at_the_cap_still_renders_and_exits_zero() {
    let source = format!("{}\n", "x".repeat(MINIMAL_MAX_LENGTH - 1));
    assert_eq!(source.len(), MINIMAL_MAX_LENGTH);
    for flag in PROFILED_FORMATS {
        let out = run(&[flag, "--profile", "minimal"], &source);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            out.status.success(),
            "{flag}: a document AT the cap exited {:?}: {stderr}",
            out.status
        );
        assert!(
            !out.stdout.is_empty(),
            "{flag}: a document at the cap rendered nothing"
        );
    }
}

/// The cap is what decides, and one byte is the whole difference. Without this
/// the two tests above could both pass against a cap that had moved.
#[test]
fn one_byte_is_the_whole_difference() {
    let at_cap = format!("{}\n", "x".repeat(MINIMAL_MAX_LENGTH - 1));
    let over_cap = format!("{}\n", "x".repeat(MINIMAL_MAX_LENGTH));
    assert_eq!(at_cap.len(), MINIMAL_MAX_LENGTH);
    assert_eq!(over_cap.len(), MINIMAL_MAX_LENGTH + 1);
    assert!(run(&["--html", "--profile", "minimal"], &at_cap)
        .status
        .success());
    assert!(!run(&["--html", "--profile", "minimal"], &over_cap)
        .status
        .success());
}

/// A structural guard, so the next output format added to the CLI cannot
/// reintroduce the swallow by copying the line above it. The infallible
/// wrappers are library API and stay; `src/main.rs` is the one caller that must
/// never reach for them.
#[test]
fn the_cli_render_path_calls_no_infallible_wrapper() {
    let main_rs = include_str!("../src/main.rs");
    for (index, line) in main_rs.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for wrapper in INFALLIBLE_WRAPPERS {
            for (column, _) in line.match_indices(wrapper) {
                assert!(
                    line[..column].ends_with("try_"),
                    "src/main.rs:{}: calls the infallible `{wrapper}`, which turns a \
                     profile violation into an empty string. Use `try_{wrapper}` and map \
                     the error to a stderr line and a non-zero exit (carve-rs#1190).\n{line}",
                    index + 1
                );
            }
        }
    }
}
