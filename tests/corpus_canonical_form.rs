//! The writer matches the canonical form the spec pins (PART 11 §2).
//!
//! The corpus formatter sweep asserts the two PART 11 §1 properties -
//! `to_html(fmt(x)) == to_html(x)` and `fmt(fmt(x)) == fmt(x)` - and neither
//! can see WHICH of two valid canonical forms the writer picked. Both hold for
//! every writer divergence found so far: a comment renders nothing, so a body
//! written at the wrong column still preserves the HTML, and a writer is
//! happily idempotent about a spelling it chose itself.
//!
//! The bytes are what separate one canonical form from two, and §2 is normative
//! about which one it is. The spec ships `<slug>.fmt` fixtures for exactly that
//! and reads them against its pinned carve-js build only - its own test file
//! names the engine-side readers as the open half of markup-carve/carve#671.
//!
//! Measured before adding: this engine already matches every fixture at the
//! current pin, so this lands green and bites only on a regression.

use std::fs;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus")
}

/// Every corpus document whose canonical form is pinned, as `(slug, source, expected)`.
fn pinned() -> Vec<(String, String, String)> {
    let dir = corpus_dir();
    if !dir.exists() {
        panic!(
            "Spec corpus not found at {}.\n\
             Did you initialize the submodule?\n  git submodule update --init",
            dir.display()
        );
    }
    let mut out: Vec<(String, String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("fmt") {
                return None;
            }
            let slug = path.file_stem().and_then(|s| s.to_str())?.to_string();
            let source = fs::read_to_string(dir.join(format!("{slug}.crv"))).ok()?;
            let expected = fs::read_to_string(&path).ok()?;
            Some((slug, source, expected))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn a_pinned_fixture_is_read() {
    // Guards the sweep below against a glob that quietly matches nothing - the
    // state these fixtures were already in for five releases, where a checker
    // reported success having compared nothing.
    let found = pinned().len();
    assert!(found >= 5, "found {found} .fmt fixtures");
}

#[test]
fn fmt_matches_every_pinned_canonical_form() {
    let mut wrong = Vec::new();
    for (slug, source, expected) in pinned() {
        let actual = carve::to_carve(&source);
        if actual != expected {
            wrong.push(format!(
                "{slug}\n  ----- expected -----\n{expected}\n  ----- actual -------\n{actual}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the writer disagrees with its pinned canonical form:\n{}",
        wrong.join("\n")
    );
}
