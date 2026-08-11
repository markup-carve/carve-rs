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
/// `::: compare` blocks in the spec's own examples gives a second, unrelated
/// route to the same number, so the two disagreeing is the signal.
///
/// carve-js and carve-php took the same route in markup-carve/carve-js#969 and
/// markup-carve/carve-php#1155; this is the third engine (carve#755).
pub fn expected_corpus_size() -> usize {
    let examples = spec_root().join("docs/examples");
    let mut count = 0usize;
    let mut files = 0usize;
    for entry in std::fs::read_dir(&examples)
        .unwrap_or_else(|e| panic!("spec examples unreadable at {}: {e}", examples.display()))
    {
        let path = entry.expect("read spec example entry").path();
        if !path.extension().is_some_and(|e| e == "md") {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).expect("spec example readable");
        for line in text.lines() {
            if is_compare_opener(line.trim()) {
                count += 1;
            }
        }
    }
    assert!(
        files > 0,
        "no spec examples found at {}",
        examples.display()
    );
    assert!(
        count > 0,
        "no `::: compare` blocks found in {}",
        examples.display()
    );
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
