//! CLI integration for `{{ path }}` includes and `--include-root` (spec §19 I10).
//!
//! The containment root defaults to the input FILE's directory, never the
//! process working directory, and stdin has no path context so directives stay
//! literal unless a root is named explicitly.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "carve-cli-includes-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&base).expect("temp dir");
        Self(base)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("parent dir");
        }
        fs::write(&full, contents).expect("write");
        full
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Output {
    stdout: String,
    stderr: String,
    success: bool,
}

fn run(args: &[&str], input: Option<&str>) -> Output {
    run_impl(args, input, None)
}

/// Same, but with the child process's working directory set, so a relative
/// input path can be exercised.
fn run_in(args: &[&str], input: Option<&str>, cwd: &Path) -> Output {
    run_impl(args, input, Some(cwd))
}

fn run_impl(args: &[&str], input: Option<&str>, cwd: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_carve"));
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command
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
        .write_all(input.unwrap_or("").as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait carve binary");
    Output {
        stdout: String::from_utf8(out.stdout).expect("utf8 stdout"),
        stderr: String::from_utf8(out.stderr).expect("utf8 stderr"),
        success: out.status.success(),
    }
}

#[test]
fn a_file_input_defaults_the_include_root_to_the_documents_directory() {
    let tmp = TempDir::new("default-root");
    let main = tmp.write("main.crv", "Before.\n\n{{ child.crv }}\n");
    tmp.write("child.crv", "Included body.\n");
    let out = run(&[main.to_str().unwrap()], None);
    assert!(out.success);
    assert!(
        out.stdout.contains("<p>Included body.</p>"),
        "stdout: {}",
        out.stdout
    );
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
}

#[test]
fn a_relative_input_path_with_a_directory_resolves_includes_correctly() {
    // Regression: the document path is absolutized before the root is derived.
    // Left relative, `book/main.crv` under root `book` made the resolver look
    // for `book/book/child.crv`.
    let tmp = TempDir::new("relative-input");
    tmp.write("book/main.crv", "{{ child.crv }}\n");
    tmp.write("book/child.crv", "Relative body.\n");
    let out = run_in(&["book/main.crv"], None, tmp.path());
    assert!(out.success);
    assert!(
        out.stdout.contains("<p>Relative body.</p>"),
        "stdout: {} stderr: {}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn stdin_has_no_path_context_so_directives_stay_literal() {
    let out = run(&[], Some("See {{ child.crv }} here.\n"));
    assert!(out.success);
    assert!(
        out.stdout.contains("{{ child.crv }}"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn include_root_enables_includes_on_stdin() {
    let tmp = TempDir::new("stdin-root");
    tmp.write("child.crv", "From stdin root.\n");
    let out = run(
        &["--include-root", tmp.path().to_str().unwrap()],
        Some("{{ child.crv }}\n"),
    );
    assert!(out.success);
    assert!(
        out.stdout.contains("<p>From stdin root.</p>"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn an_unresolvable_directive_warns_on_stderr_and_stays_literal() {
    let tmp = TempDir::new("warn");
    let main = tmp.write("main.crv", "{{ missing.crv }}\n");
    let out = run(&[main.to_str().unwrap()], None);
    assert!(out.success);
    assert!(
        out.stdout.contains("{{ missing.crv }}"),
        "stdout: {}",
        out.stdout
    );
    assert!(
        out.stderr.contains("include-unresolved"),
        "stderr: {}",
        out.stderr
    );
    // The warning names the file the directive lives in (I4 attribution).
    assert!(out.stderr.contains("main.crv"), "stderr: {}", out.stderr);
}

#[test]
fn an_escape_above_the_default_root_is_denied() {
    let tmp = TempDir::new("escape");
    tmp.write("secret.crv", "TOP SECRET\n");
    let main = tmp.write("book/main.crv", "{{ ../secret.crv }}\n");
    let out = run(&[main.to_str().unwrap()], None);
    assert!(out.success);
    assert!(!out.stdout.contains("TOP SECRET"), "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("include-unresolved"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn a_widened_include_root_admits_the_sibling_directory() {
    // Same layout as above; naming the parent as the root makes the reach legal
    // because containment is checked against THAT root.
    let tmp = TempDir::new("widened");
    tmp.write("secret.crv", "SHARED BODY\n");
    let main = tmp.write("book/main.crv", "{{ ../secret.crv }}\n");
    let out = run(
        &[
            "--include-root",
            tmp.path().to_str().unwrap(),
            main.to_str().unwrap(),
        ],
        None,
    );
    assert!(out.success);
    assert!(
        out.stdout.contains("<p>SHARED BODY</p>"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn a_nonexistent_explicit_include_root_is_a_fatal_error() {
    let tmp = TempDir::new("bad-root");
    let main = tmp.write("main.crv", "{{ child.crv }}\n");
    let out = run(
        &[
            "--include-root",
            tmp.path().join("no-such-dir").to_str().unwrap(),
            main.to_str().unwrap(),
        ],
        None,
    );
    assert!(!out.success);
    assert!(
        out.stderr.contains("cannot use include root"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn the_formatter_does_not_expand_includes() {
    // `carve fmt` round-trips SOURCE; inlining a file into it would rewrite the
    // author's document rather than format it.
    //
    // It must also PRESERVE the directive. This used to emit
    // `\{\{ child\.crv \}\}`: still literal text to the core, so the round-trip
    // invariant held, but the include was destroyed and nothing looked wrong
    // until a resolver ran and the chapter had silently vanished.
    let tmp = TempDir::new("fmt");
    let main = tmp.write("main.crv", "{{ child.crv }}\n");
    tmp.write("child.crv", "Included body.\n");
    let out = run(&["fmt", main.to_str().unwrap()], None);
    assert!(out.success);
    assert_eq!(out.stdout, "{{ child.crv }}\n");
    assert!(
        !out.stdout.contains("Included body"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn includes_expand_for_non_html_output_formats_too() {
    let tmp = TempDir::new("markdown");
    let main = tmp.write("main.crv", "{{ child.crv }}\n");
    tmp.write("child.crv", "# Included heading\n");
    let out = run(&["--markdown", main.to_str().unwrap()], None);
    assert!(out.success);
    assert!(
        out.stdout.contains("# Included heading"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn included_content_is_subject_to_the_same_sanitization_as_typed_content() {
    // Expansion sits BEFORE profile filtering, so an include cannot smuggle raw
    // HTML past `--no-raw-html` (spec §25, "no privilege escalation via
    // include").
    let tmp = TempDir::new("sanitize");
    let main = tmp.write("main.crv", "{{ child.crv }}\n");
    tmp.write("child.crv", "```=html\n<script>alert(1)</script>\n```\n");
    let out = run(&["--no-raw-html", main.to_str().unwrap()], None);
    assert!(out.success);
    assert!(
        !out.stdout.contains("<script>"),
        "raw HTML escaped through an include: {}",
        out.stdout
    );
}
