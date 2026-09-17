use std::io::Write;
use std::process::{Command, Output, Stdio};

fn render(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("carve starts");
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn html(args: &[&str], source: &str) -> String {
    let output = render(args, source);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn registry_extensions_are_repeatable() {
    let output = html(
        &["--extension", "autolink", "--extension", "semantic-span"],
        "Visit https://example.com. [x]{samp}",
    );
    assert!(output.contains("<a href=\"https://example.com\">"));
    assert!(output.contains("<samp>x</samp>"));
}

#[test]
fn every_registry_key_is_selectable() {
    for key in carve::extensions::registry::keys() {
        let output = render(&["--extension", key], "");
        assert!(
            output.status.success(),
            "{key}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn unknown_and_missing_extension_keys_fail() {
    for args in [&["--extension"][..], &["--extension", "not-registered"][..]] {
        let output = render(args, "x");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("extension"));
    }
    let unknown = render(&["--extension", "not-registered"], "");
    let stderr = String::from_utf8_lossy(&unknown.stderr);
    assert!(stderr.contains("autolink"));
    assert!(stderr.contains("tabs"));
}

#[test]
fn extension_modes_require_their_extension() {
    for args in [
        &["--tabs-mode", "aria"][..],
        &["--citation-mode", "author-date"][..],
    ] {
        let output = render(args, "x");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("requires --extension"));
    }
}

#[test]
fn selective_render_options_are_refused_by_other_commands() {
    for args in [
        &["fmt", "--extension", "tabs"][..],
        &["flatten", "--no-sections"][..],
    ] {
        let output = render(args, "x");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("only to rendering"));
    }
}

#[test]
fn tabs_mode_configures_the_named_extension() {
    let source = ":::: tabs\n::: tab [First]\nOne.\n:::\n::::";
    let output = html(&["--extension", "tabs", "--tabs-mode", "aria"], source);
    assert!(output.contains("role=\"tablist\""));
    assert!(output.contains("<button type=\"button\" role=\"tab\""));
    assert!(output.contains("role=\"tabpanel\""));
}

#[test]
fn citation_mode_configures_the_named_extension() {
    let source = "See [@smith].\n\n[@smith]: {author=Smith year=2020} Entry.";
    let output = html(
        &["--extension", "citations", "--citation-mode", "author-date"],
        source,
    );
    assert!(output.contains(">Smith 2020</a>"));
}

#[test]
fn section_and_source_line_options_reach_the_renderer() {
    let output = html(&["--no-sections", "--source-lines"], "# Heading");
    assert!(!output.contains("<section"));
    assert!(output.contains("<h1 id=\"Heading\" data-source-line=\"1\">"));
}

#[test]
fn help_lists_the_selective_extension_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for flag in [
        "--extension KEY",
        "--tabs-mode MODE",
        "--citation-mode MODE",
        "--no-sections",
        "--source-lines",
    ] {
        assert!(stdout.contains(flag), "help omits {flag}");
    }
}
