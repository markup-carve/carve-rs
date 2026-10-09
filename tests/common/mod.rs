//! Shared helpers for the integration tests.
//!
//! Each integration test is its own binary and compiles this module separately,
//! so a helper only one test uses is dead code in the others. CI builds with
//! `-D warnings`, hence the allow.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The number of corpus documents the pinned spec should produce.
///
/// DERIVED INDEPENDENTLY of the corpus directory, on purpose. A sweep that
/// counts its own inputs and then asserts a FLOOR on that count cannot notice a
/// truncated checkout: `> 400` accepts 401 of 892, less than half the corpus,
/// and every document it never read passes by not existing. Counting the
/// declared fence pairs in the spec's own examples gives a second, unrelated
/// route to the same number, so the two disagreeing is the signal.
///
/// carve-js and carve-php took the same route in markup-carve/carve-js#969 and
/// markup-carve/carve-php#1155; this is the third engine (carve#755).
///
/// The authored examples live under `resources/examples`. `docs/examples` is
/// generated output as of markup-carve/carve#1194 and is no longer committed,
/// so counting the generated copies would make this guard depend on whether a
/// docs build had run in the checkout.
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
    let examples = spec_root().join("resources/examples");
    let mut count = 0;
    for page in ["core.md", "extensions.md", "edge-cases.md"] {
        let path = examples.join(page);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        count += count_declared_pairs(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
    assert!(count > 0, "no comparison pairs found in spec examples");
    count
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
