use std::io::Write;
use std::process::{Command, Stdio};

fn aligned(source: &str) -> String {
    carve::try_to_carve_with_options(source, &carve::Options::default().with_align_tables(true))
        .unwrap()
}

fn cli(args: &[&str], input: &str) -> std::process::Output {
    // A file-path run never reads stdin, so it gets none: writing to it races the exit.
    let stdin = if input.is_empty() {
        Stdio::null()
    } else {
        Stdio::piped()
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(input.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn writer_options_and_separator() {
    let source = "| A | B |\n|---|---|\n| x | yy |\n";
    let expected = "| A | B  |\n|---|--- |\n| x | yy |\n";
    assert_eq!(aligned(source), expected);
    assert_eq!(carve::to_carve(expected), source);
    let doc = carve::parse(source);
    assert_eq!(
        carve::render_carve_with_options(
            &doc,
            &carve::CarveWriterOptions::default().with_align_tables(true)
        )
        .unwrap(),
        expected
    );
    assert_eq!(carve::render_carve(&doc).unwrap(), source);
}

#[test]
fn written_escapes_count_in_width() {
    let source = "| a\\ b | x |\n| ccc | `y|z` |\n";
    let written = aligned(source);
    assert_eq!(written, "| a\\ b | x     |\n| ccc  | `y|z` |\n");
    assert_eq!(aligned(&written), written);
    let even = "| a\\ \\ b |\n| cccccc |\n";
    assert_eq!(aligned(even), even);
    for source in [
        "| &amp; | x |\n| abc | yy |\n",
        "| a\\|b | x |\n| ccc | yy |\n",
    ] {
        let written = aligned(source);
        assert_eq!(aligned(&written), written);
        assert_eq!(carve::to_html(&written), carve::to_html(source));
    }
}

#[test]
fn cli_formats_and_checks_aligned_output() {
    let source = "| A | B |\n|---|---|\n| x | yy |\n";
    let expected = aligned(source);
    let printed = cli(&["fmt", "--align-tables"], source);
    assert!(printed.status.success());
    assert_eq!(String::from_utf8(printed.stdout).unwrap(), expected);
    let path = std::env::temp_dir().join(format!("carve-align-check-{}", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let arg = path.to_str().unwrap();
    assert_eq!(
        cli(&["fmt", "--align-tables", "--check", arg], "")
            .status
            .code(),
        Some(1)
    );
    assert!(cli(&["fmt", "--align-tables", "-w", arg], "")
        .status
        .success());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    assert_eq!(
        cli(&["fmt", "--align-tables", "--check", arg], "")
            .status
            .code(),
        Some(0)
    );
    std::fs::remove_file(path).unwrap();
    assert!(!cli(&["--align-tables"], "").status.success());
}

#[test]
fn long_regional_indicator_run() {
    let source = format!("| a | b |\n|---|---|\n| {} | x |\n", "🇦".repeat(8000));
    let started = std::time::Instant::now();
    let written = aligned(&source);
    assert!(started.elapsed().as_secs_f32() < 10.0);
    assert_eq!(aligned(&written), written);
}

#[test]
fn extra_separator_entry_and_nested_tables() {
    let extra = "| A | B |\n|:--|--:|----|\n| x | long |\n";
    let written = aligned(extra);
    assert!(written.contains("|----|"), "{written}");
    assert_eq!(aligned(&written), written);
    for source in [
        "> | a | longer |\n> | x | y |\n",
        "- | a | longer |\n  | x | y |\n",
    ] {
        let written = aligned(source);
        assert_eq!(aligned(&written), written, "{source}");
        assert_eq!(carve::to_html(&written), carve::to_html(source));
    }
}
