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
    code: Option<i32>,
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
        code: out.status.code(),
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
fn a_relative_include_root_is_absolutized_by_the_flag_not_by_the_resolver() {
    // The resolver refuses a relative root (carve#2004). The convenience the
    // spec sanctions lives in the FRONT END, so `--include-root .` still works,
    // and it means the cwd it was typed in rather than whatever the resolver
    // would have canonicalized against.
    let tmp = TempDir::new("relative-root");
    tmp.write("secret.crv", "SHARED BODY\n");
    tmp.write("book/main.crv", "{{ ../secret.crv }}\n");
    let out = run_in(&["--include-root", ".", "book/main.crv"], None, tmp.path());
    assert!(out.success, "stderr: {}", out.stderr);
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

// ---------------------------------------------------------------------------
// `carve flatten` (the deliberate opposite of `carve fmt`)
// ---------------------------------------------------------------------------

#[test]
fn flatten_inlines_every_include_into_one_document() {
    let tmp = TempDir::new("flatten");
    let main = tmp.write("main.crv", "Intro.\n\n{{ chapters/one.crv }}\n");
    tmp.write("chapters/one.crv", "Chapter body.\n");

    let out = run(&["flatten", main.to_str().unwrap()], None);
    assert!(out.success, "{}", out.stderr);
    assert!(out.stdout.contains("Chapter body."), "{}", out.stdout);
    assert!(!out.stdout.contains("{{"), "{}", out.stdout);
}

#[test]
fn flatten_is_the_only_carve_output_that_expands() {
    // The pair that says why both commands exist: `fmt` round-trips the
    // author's document (I15), `flatten` asks for the other one.
    let tmp = TempDir::new("flatten-vs-fmt");
    let main = tmp.write("main.crv", "{{ child.crv }}\n");
    tmp.write("child.crv", "Child body.\n");

    let fmt = run(&["fmt", main.to_str().unwrap()], None);
    assert!(fmt.stdout.contains("{{ child.crv }}"), "{}", fmt.stdout);

    let flat = run(&["flatten", main.to_str().unwrap()], None);
    assert!(flat.stdout.contains("Child body."), "{}", flat.stdout);
}

#[test]
fn a_flattened_document_renders_exactly_like_the_expanded_original() {
    // THE INVARIANT THAT MAKES FLATTENING SAFE TO PASTE, and it is not free:
    // expansion RENAMES colliding heading ids and footnote labels (I5), so the
    // renames have to survive into the SOURCE or the flattened file resolves
    // its own references differently. Explicit ids and footnote labels are
    // spelled by the writer, which is what carries them.
    let tmp = TempDir::new("flatten-fidelity");
    let main = tmp.write(
        "main.crv",
        "Parent [^note].\n\n[^note]: Parent note.\n\n{{ ch/a.crv }}\n\n{{ ch/b.crv }}\n",
    );
    tmp.write(
        "ch/a.crv",
        "{#intro}\n# A\n\nChild a [^note].\n\n[^note]: A note.\n",
    );
    tmp.write(
        "ch/b.crv",
        "{#intro}\n# B\n\nChild b [^note].\n\n[^note]: B note.\n",
    );

    let expanded = run(&[main.to_str().unwrap()], None);
    assert!(expanded.success, "{}", expanded.stderr);

    let flat = run(&["flatten", main.to_str().unwrap()], None);
    assert!(flat.success, "{}", flat.stderr);
    let flattened = tmp.write("flat.crv", &flat.stdout);
    let rendered = run(&[flattened.to_str().unwrap()], None);

    assert_eq!(
        rendered.stdout, expanded.stdout,
        "the flattened document renders differently from the expanded original"
    );
    // And the renames are visible in the source rather than implied.
    assert!(flat.stdout.contains("[^note-2]"), "{}", flat.stdout);
    assert!(flat.stdout.contains("{#intro-2}"), "{}", flat.stdout);
}

#[test]
fn flatten_refuses_stdin_with_no_root_rather_than_writing_it_back_unchanged() {
    // "Flatten this" with nothing to resolve against cannot be honoured, and
    // echoing the document back would look like it simply had no includes.
    let out = run(&["flatten"], Some("{{ child.crv }}\n"));
    assert!(!out.success);
    assert!(out.stderr.contains("--include-root"), "{}", out.stderr);
}

#[test]
fn flatten_takes_an_explicit_root_for_stdin() {
    let tmp = TempDir::new("flatten-stdin");
    tmp.write("child.crv", "Child body.\n");
    let out = run(
        &["flatten", "--include-root", tmp.path().to_str().unwrap()],
        Some("{{ child.crv }}\n"),
    );
    assert!(out.success, "{}", out.stderr);
    assert!(out.stdout.contains("Child body."), "{}", out.stdout);
}

#[test]
fn flatten_reports_an_unresolved_target_and_keeps_the_directive() {
    let tmp = TempDir::new("flatten-missing");
    let main = tmp.write("main.crv", "x\n\n{{ nope.crv }}\n");
    let out = run(&["flatten", main.to_str().unwrap()], None);
    assert!(out.success, "{}", out.stderr);
    assert!(out.stdout.contains("{{ nope.crv }}"), "{}", out.stdout);
    assert!(out.stderr.contains("include-unresolved"), "{}", out.stderr);
}

/// `--report-includes` publishes the spec I11 dependency list (carve-rs#1676).
///
/// Refusals carry their class here, which the WARNING deliberately does not:
/// every refusal warns as `include-unresolved` so a rendered page cannot report
/// "outside the root" apart from "not there" (I7). This file is written only
/// where the operator asked for it, and it goes to the processor.
#[test]
fn the_dependency_list_names_resolved_and_refused_targets_with_their_class() {
    let tmp = TempDir::new("deps");
    tmp.write("secret.crv", "TOP SECRET\n");
    let main = tmp.write(
        "book/main.crv",
        "{{ child.crv }}\n\n{{ missing.crv }}\n\n{{ ../secret.crv }}\n",
    );
    tmp.write("book/child.crv", "Child body.\n");
    let report = tmp.path().join("deps.json");
    let out = run(
        &[
            "--report-includes",
            report.to_str().unwrap(),
            main.to_str().unwrap(),
        ],
        None,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    let json = fs::read_to_string(&report).expect("dependency report written");

    assert!(
        json.contains("\"id\":\"") && json.contains("child.crv\",\"resolved\":true"),
        "resolved child missing: {json}"
    );
    assert!(
        json.contains("{\"id\":\"missing.crv\",\"resolved\":false,\"denial\":\"not-found\"}"),
        "missing target missing: {json}"
    );
    assert!(
        json.contains("{\"id\":\"../secret.crv\",\"resolved\":false,\"denial\":\"outside-root\"}"),
        "denied target missing: {json}"
    );
    // The contrast: both refusals warn under one rule, and the report separates
    // them.
    assert_eq!(
        out.stderr.matches("include-unresolved").count(),
        2,
        "stderr: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("outside-root"),
        "the denial class must stay off the warning channel: {}",
        out.stderr
    );
}

/// The list is what a host needs past the warning cap.
///
/// Warnings stop at `DEFAULT_MAX_WARNINGS`, so reconstructing the refused half
/// from them cannot tell a complete list from a truncated one. That is the
/// workaround carve-go is stuck with (markup-carve/hugo-carve#27).
#[test]
fn the_dependency_list_survives_the_warning_cap() {
    let tmp = TempDir::new("deps-cap");
    let directives = (0..120)
        .map(|n| format!("{{{{ m{n:03}.crv }}}}\n"))
        .collect::<Vec<_>>()
        .join("\n");
    let main = tmp.write("main.crv", &directives);
    let report = tmp.path().join("deps.json");
    let out = run(
        &[
            "--report-includes",
            report.to_str().unwrap(),
            main.to_str().unwrap(),
        ],
        None,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("include warning(s) suppressed"),
        "the warning channel is expected to be capped here: {}",
        out.stderr
    );
    let json = fs::read_to_string(&report).expect("dependency report written");
    assert_eq!(
        json.matches("\"resolved\":false").count(),
        120,
        "the list is capped with the warnings: {json}"
    );
    // Both ends, so a truncation at either one is visible.
    assert!(json.contains("\"id\":\"m000.crv\""), "{json}");
    assert!(json.contains("\"id\":\"m119.crv\""), "{json}");
}

#[test]
fn a_document_with_no_directives_reports_an_empty_dependency_list() {
    // A host reads one file per render, so "no includes" has to be a list, not
    // a missing file.
    let tmp = TempDir::new("deps-empty");
    let main = tmp.write("main.crv", "Nothing to include.\n");
    let report = tmp.path().join("deps.json");
    let out = run(
        &[
            "--report-includes",
            report.to_str().unwrap(),
            main.to_str().unwrap(),
        ],
        None,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    let json = fs::read_to_string(&report).expect("dependency report written");
    assert_eq!(json.trim(), "{\"dependencies\":[]}");
}

#[test]
fn stdin_with_no_root_reports_an_empty_dependency_list() {
    // The directive stays literal here, so there is no dependency - and the
    // report still has to exist.
    let tmp = TempDir::new("deps-stdin");
    let report = tmp.path().join("deps.json");
    let out = run(
        &["--report-includes", report.to_str().unwrap()],
        Some("See {{ child.crv }} here.\n"),
    );
    assert!(out.success, "stderr: {}", out.stderr);
    let json = fs::read_to_string(&report).expect("dependency report written");
    assert_eq!(json.trim(), "{\"dependencies\":[]}");
}

#[test]
fn the_dependency_list_goes_to_stderr_for_a_dash() {
    let tmp = TempDir::new("deps-dash");
    let main = tmp.write("main.crv", "{{ child.crv }}\n");
    tmp.write("child.crv", "Child body.\n");
    let out = run(&["--report-includes", "-", main.to_str().unwrap()], None);
    assert!(out.success, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("{\"dependencies\":["), "{}", out.stderr);
    assert!(out.stdout.contains("<p>Child body.</p>"), "{}", out.stdout);
}

#[test]
fn report_includes_without_a_file_is_a_usage_error() {
    // Exit 2, like every other bad flag: 1 is reserved for "ran and found
    // something". No stdin: the usage error exits before reading it, so writing
    // any would race the exit and break the pipe.
    let out = run(&["--report-includes"], None);
    assert_eq!(out.code, Some(2), "stderr: {}", out.stderr);
    assert!(out.stderr.contains("--report-includes"), "{}", out.stderr);
}
