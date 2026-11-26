//! Findings of an included file: its own duplicate definitions (carve-rs#4614)
//! and the CLI line and column counted in that file (C2-N-087).
use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use std::process::Command;
use std::sync::Arc;

const PART: &str = "a[^n]\n\n[^n]: x\n\n[^n]: y\n\n[l]: /a\n[l]: /b\n";

fn options() -> LintOptions {
    let resolver = Arc::new(|request: &Request<'_>| {
        if request.path == "b.crv" {
            Ok((PART.as_bytes().to_vec(), "b.crv".into()))
        } else {
            Err(Cause::NotFound)
        }
    });
    LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    }
}

#[test]
fn c2_n_060_duplicate_definitions_of_an_included_file_name_that_file() {
    let findings = lint("{{ b.crv }}\n", &options());
    let duplicates: Vec<_> = findings
        .iter()
        .filter(|d| d.code.starts_with("duplicate-"))
        .map(|d| {
            (
                d.code,
                d.file.as_deref(),
                d.pos.map(|p| (p.start_line, p.start_offset)),
            )
        })
        .collect();
    assert_eq!(
        duplicates,
        [
            (
                "duplicate-footnote-definition",
                Some("b.crv"),
                Some((5, 16))
            ),
            (
                "duplicate-link-reference-definition",
                Some("b.crv"),
                Some((8, 33))
            ),
        ]
    );
}

#[test]
fn c2_n_087_a_file_included_twice_reports_its_parse_findings_once() {
    let findings = lint("{{ b.crv }}\n\n{{ b.crv }}\n", &options());
    let duplicates = findings
        .iter()
        .filter(|d| d.code.starts_with("duplicate-"))
        .count();
    assert_eq!(duplicates, 2);
}

#[test]
fn c2_n_087_cli_counts_an_included_finding_in_its_own_file() {
    let root = std::env::temp_dir().join(format!(
        "carve-include-findings-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.crv"), "{{ b.crv }}\n").unwrap();
    std::fs::write(root.join("b.crv"), PART).unwrap();
    let binary = std::env::var("CARVE_CLI_TEST_BIN")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_carve").to_string());
    let output = Command::new(binary)
        .args(["lint", root.join("a.crv").to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("b.crv:5:1: warning duplicate-footnote-definition"),
        "{stdout}"
    );
    assert!(
        stdout.contains("b.crv:8:1: warning duplicate-link-reference-definition"),
        "{stdout}"
    );
    assert!(
        stdout.contains("b.crv:7:1: warning unused-link-reference-definition"),
        "{stdout}"
    );
}
