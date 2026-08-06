//! CLI integration: `--smart-typography glyph|source`.
//!
//! The switch existed in `Options` and reached both HTML and Markdown, but
//! nothing on the command line could set it - so a host piping the binary into
//! something else, which is the case source mode exists for, could not ask for
//! it. The spec's optional corpus case `29-smart-typography-off` is driven
//! through each engine's CLI, so without this flag that fixture could not be
//! measured against this engine at all.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    // A rejected flag makes the binary exit before it reads stdin, so the write
    // fails with EPIPE. That is the behavior under test, not an error.
    let _ = child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes());
    let out = child.wait_with_output().expect("wait carve binary");
    (
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
        out.status.success(),
    )
}

const INPUT: &str = "He said \"hi\" -- really... 1 -> 2\n";

#[test]
fn html_source_mode_emits_what_the_author_typed() {
    let (out, _, ok) = run(&["--html", "--smart-typography", "source"], INPUT);
    assert!(ok);
    assert_eq!(
        out.trim(),
        "<p>He said \"hi\" -- really... 1 -&gt; 2</p>",
        "{out}"
    );
}

#[test]
fn markdown_source_mode_emits_what_the_author_typed() {
    let (out, _, ok) = run(&["--markdown", "--smart-typography", "source"], INPUT);
    assert!(ok);
    assert_eq!(out.trim(), "He said \"hi\" -- really... 1 -> 2", "{out}");
}

// The next two assert on the OUTPUT, never on the exit status. The flag was
// accepted on both of these targets before carve#560 and did nothing, so an
// exit-status assertion here is a check that cannot fail.
#[test]
fn plain_source_mode_emits_what_the_author_typed() {
    let (out, _, ok) = run(&["--plain", "--smart-typography", "source"], INPUT);
    assert!(ok);
    assert_eq!(out.trim(), "He said \"hi\" -- really... 1 -> 2", "{out}");

    let (glyph, _, ok) = run(&["--plain"], INPUT);
    assert!(ok);
    assert_ne!(glyph, out);
}

#[test]
fn ansi_source_mode_emits_what_the_author_typed() {
    let (out, _, ok) = run(&["--ansi", "--smart-typography", "source"], INPUT);
    assert!(ok);
    assert_eq!(out.trim(), "He said \"hi\" -- really... 1 -> 2", "{out}");

    let (glyph, _, ok) = run(&["--ansi"], INPUT);
    assert!(ok);
    assert_ne!(glyph, out);
}

#[test]
fn glyph_is_the_default_and_can_be_asked_for() {
    let (implicit, _, ok) = run(&["--html"], INPUT);
    assert!(ok);
    let (explicit, _, ok) = run(&["--html", "--smart-typography", "glyph"], INPUT);
    assert!(ok);
    assert_eq!(implicit, explicit);
    assert!(implicit.contains('\u{201c}'), "{implicit}");
}

#[test]
fn an_unknown_mode_is_rejected_rather_than_ignored() {
    // Silently falling back to the default is the failure this switch keeps
    // hitting: output that looks configured and is not.
    let (_, err, ok) = run(&["--html", "--smart-typography", "bogus"], INPUT);
    assert!(!ok);
    assert!(err.contains("expected glyph|source"), "{err}");
}

#[test]
fn a_missing_value_is_rejected() {
    let (_, err, ok) = run(&["--html", "--smart-typography"], INPUT);
    assert!(!ok);
    assert!(err.contains("requires a mode"), "{err}");
}
