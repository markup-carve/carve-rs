//! Shared helpers for the integration tests.
//!
//! Each integration test is its own binary and compiles this module separately,
//! so a helper only one test uses is dead code in the others. CI builds with
//! `-D warnings`, hence the allow.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The number of corpus documents the pinned spec should produce.
///
/// Derived from the generated spec test files, independently of the corpus
/// directory. The generator deduplicates declared examples before writing
/// both sets of files. Seven byte-transform cases are omitted from the spec
/// test files but remain in the corpus.
///
/// carve-js and carve-php took the same route in markup-carve/carve-js#969 and
/// markup-carve/carve-php#1155; this is the third engine (carve#755).
///
/// Count the authored comparison pairs before the generator deduplicates them.
pub fn count_declared_pairs(source: &str) -> Result<usize, &'static str> {
    let mut marker: Option<&str> = None;
    let mut fence: Option<&str> = None;
    let (mut carve, mut html, mut total) = (0, 0, 0);
    for line in source.lines() {
        if let Some(run) = fence {
            if line.starts_with(run) && line[run.len()..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        let ticks = line.bytes().take_while(|b| *b == b'`').count();
        if ticks >= 3 {
            fence = Some(&line[..ticks]);
            if marker.is_some() {
                match line[ticks..].trim() {
                    "carve" => carve += 1,
                    "html" => html += 1,
                    _ => {}
                }
            }
            continue;
        }
        let trimmed = line.trim();
        if let Some(run) = marker {
            if trimmed == run {
                if carve == 0 || carve != html {
                    return Err("unpaired or empty compare block");
                }
                total += carve;
                marker = None;
            }
            continue;
        }
        if is_compare_opener(trimmed) {
            let colons = trimmed.bytes().take_while(|b| *b == b':').count();
            marker = Some(&trimmed[..colons]);
            carve = 0;
            html = 0;
        }
    }
    if marker.is_some() || fence.is_some() {
        return Err("unclosed compare block or fence");
    }
    Ok(total)
}

pub fn expected_corpus_size() -> usize {
    let spec_tests = spec_root().join("tests/spec");
    let mut count = 0;
    for entry in std::fs::read_dir(&spec_tests)
        .unwrap_or_else(|error| panic!("read {}: {error}", spec_tests.display()))
    {
        let path = entry.expect("spec test entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("test") {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        count += source.lines().filter(|line| *line == ".").count();
    }
    assert!(count > 0, "no mirrored pairs found in spec test files");
    // bom (1), cr (1), crlf (1), nul (2), no-final-newline (2).
    count + 7
}

/// `:::` or longer, then `compare`, optionally followed by arguments.
fn is_compare_opener(line: &str) -> bool {
    let rest = line.trim_start_matches(':');
    if line.len() - rest.len() < 3 {
        return false;
    }
    let rest = rest.strip_prefix(char::is_whitespace).map(str::trim_start);
    match rest {
        Some(r) => {
            r == "compare"
                || r.strip_prefix("compare")
                    .is_some_and(|t| t.starts_with(char::is_whitespace))
        }
        None => false,
    }
}

pub fn corpus_dir() -> PathBuf {
    spec_root().join("tests/corpus")
}

fn spec_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec")
}

/// The name libtest's `--exact` filter matches for `test` in the module that
/// passes its own `module_path!()`. In the suite binary that is
/// `file_stem::test`, not the bare function name, so a re-exec spelled with the
/// bare name selects nothing.
pub fn exact_test_name(module_path: &str, test: &str) -> String {
    match module_path.split_once("::") {
        Some((_, module)) => format!("{module}::{test}"),
        None => test.to_string(),
    }
}

pub(crate) mod footnote_fanout;

/// Asserts a test's inputs against `fixtures/rule_a_ledger_pins.json`: rule A
/// (carve#3885) retired the reading the test was written for, so its inputs are
/// pinned to the spec model's HTML, and where it checked the writer, to output
/// that renders the same in the model.
pub fn assert_rule_a_pins(key: &str) {
    let pins: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/rule_a_ledger_pins.json")).unwrap();
    for pin in pins[key].as_array().expect("pinned inputs") {
        let source = pin["source"].as_str().unwrap();
        assert_eq!(
            carve::to_html(source).trim(),
            pin["html"].as_str().unwrap(),
            "{source:?}"
        );
        if let Some(carve) = pin["carve"].as_str() {
            assert_eq!(carve::to_carve(source).trim(), carve, "{source:?}");
        }
    }
}
