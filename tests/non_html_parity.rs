//! Parity oracle for the non-HTML renderers. There is no cross-impl corpus for
//! non-HTML output, so carve-php's Markdown / PlainText / ANSI output (captured
//! per case in tests/fixtures/golden/<name>.{md,plain,ansi}) is the reference
//! these must reproduce byte-for-byte. carve-js's merged renderers match the
//! same golden, so all three impls agree.

use std::fs;
use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden")
}

fn case_names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(golden_dir())
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            if p.extension().and_then(|s| s.to_str()) == Some("crv") {
                p.file_stem().map(|s| s.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

fn read(name: &str, ext: &str) -> String {
    fs::read_to_string(golden_dir().join(format!("{name}.{ext}"))).unwrap()
}

fn check(ext: &str, render: impl Fn(&str) -> String) {
    let mut failures = Vec::new();
    for name in case_names() {
        let input = read(&name, "crv");
        let expected = read(&name, ext);
        let actual = render(&input);
        if actual != expected {
            failures.push(format!(
                "case `{name}` ({ext}):\n--- expected ---\n{expected:?}\n--- actual ---\n{actual:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn markdown_parity() {
    check("md", carve::to_markdown);
}

#[test]
fn plain_text_parity() {
    check("plain", carve::to_plain_text);
}

#[test]
fn ansi_parity() {
    check("ansi", carve::to_ansi);
}
