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

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Writer rulings implemented ahead of this repository's spec submodule pin, as
/// `(slug, reason, ahead)`. Remove each entry when the pin reaches
/// markup-carve/carve#1757.
///
/// THE THIRD COLUMN IS WHAT THIS WRITER EMITS TODAY, and it is the difference
/// between a declaration and a skip. A skip says only that the fixture and the
/// writer disagree, which leaves the writer's own answer unpinned - and these
/// four are the only `.fmt` fixtures holding a definition body, so that answer
/// was unpinned everywhere.
///
/// Measured, by reverting the writer to the two-space separator this repo
/// shipped before markup-carve/carve#1757: under the skip this test failed, but
/// it reported `an ahead-of-pin canonical-form declaration is stale` - the four
/// fixtures had started AGREEING again, so the sweep read a writer regression
/// as a bookkeeping error. The declaration reports the writer instead, and
/// prints the form it expected beside the form it got.
///
/// `tests/corpus.rs` has recorded the value for the render side since it was
/// written, and for the same reason. This is that shape, on the writer side.
const AHEAD_OF_PIN: &[(&str, &str, &str)] = &[
    (
        "227-a-definition-inside-a-definition-list-dd-is-collected-and-the-entry-keeps-no-trace",
        "one space is the canonical definition separator (markup-carve/carve#1757)",
        ":: term\n: [r]: /u\n\nsee [t][r]\n",
    ),
    (
        "227-a-definition-inside-a-definition-list-dd-is-collected-and-the-entry-keeps-no-trace-2",
        "one space is the canonical definition separator (markup-carve/carve#1757)",
        ":: term\n: [^f]: x\n\nsee[^f]\n",
    ),
    (
        "279-a-boundary-line-inside-an-open-fence-does-not-end-the-container-3",
        "narrowing the separator carries the body's fence down with it \
         (markup-carve/carve#1757)",
        ":: t\n: d\n\n  ```\n  a\n\n  b\n  ```\n",
    ),
    (
        "407-one-consumed-boolean-spells-the-looseness-no-blank-line-can-2",
        "one space is the canonical definition separator (markup-carve/carve#1757)",
        "{loose}\n:: Term\n: Definition.\n",
    ),
];

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
    let mut observed_ahead = BTreeSet::new();
    for (slug, source, expected) in pinned() {
        let actual = carve::to_carve(&source);
        if let Some((_, reason, ahead)) = AHEAD_OF_PIN.iter().find(|(name, _, _)| *name == slug) {
            observed_ahead.insert(slug.clone());
            if actual != *ahead {
                wrong.push(format!(
                    "{slug} ({reason})\n  ----- declared ahead -----\n{ahead}\n  \
                     ----- actual -------\n{actual}"
                ));
            } else if actual == expected {
                wrong.push(format!(
                    "{slug}: the pin has caught up; delete its AHEAD_OF_PIN entry"
                ));
            }
            continue;
        }
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
    assert_eq!(
        observed_ahead.len(),
        AHEAD_OF_PIN.len(),
        "an ahead-of-pin canonical-form declaration is stale"
    );
}
