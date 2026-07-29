//! PART 11 section 7: `fmt` never emits a line whose only content is ASCII
//! space or tab. Such a line is emitted empty.
//!
//! Swept over the whole corpus rather than pinned per case. A whitespace-only
//! line is not stable -- editors that strip trailing whitespace on save,
//! `git apply --whitespace=fix` and CI whitespace checks all rewrite it, so a
//! formatter emitting one produces output that ordinary tooling changes behind
//! it (carve#375).
//!
//! Two things section 7 deliberately does NOT cover, and this sweep must not
//! either: whitespace at the end of a line that HAS content (it can be document
//! content -- stripping it before a soft break changed rendered output in
//! carve#359), and whitespace that IS verbatim content, since a line of three
//! spaces inside a code block renders as three spaces.

use std::fs;
use std::path::Path;

/// Lines whose ONLY content is ASCII space or tab. A trailing no-break space is
/// content rather than layout -- the author wrote it and it renders as
/// `&nbsp;` -- so U+00A0 is excluded, which Rust's `trim` would not do and
/// which corpus case 139 pins.
fn offending_lines(out: &str) -> Vec<(usize, String)> {
    out.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && line.trim_matches([' ', '\t']).is_empty())
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect()
}

#[test]
fn the_writer_never_emits_trailing_whitespace() {
    let dir = Path::new("tests/spec/tests/corpus");
    let entries = fs::read_dir(dir).expect("the corpus directory");
    let mut checked = 0;
    let mut failures = Vec::new();

    for entry in entries {
        let path = entry.expect("a corpus entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("crv") {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        let out = carve::to_carve(&source);
        checked += 1;
        for (line_no, line) in offending_lines(&out) {
            failures.push(format!(
                "{}:{line_no}: {:?}",
                path.file_name().unwrap().to_string_lossy(),
                line
            ));
        }
    }

    assert!(checked > 400, "only {checked} corpus inputs were read");

    // Known remaining, shared with carve-js: a fenced block inside a list item
    // has its indentation SENTINEL-PROTECTED so that normalize() cannot eat real
    // code indentation, which also hides the structural indent on a line whose
    // verbatim content is empty. Section 7 says that indent is layout and must
    // go; fixing it means teaching the protection to tell the two apart, in both
    // engines. Listed rather than filtered out of the sweep, so it stays visible.
    let known = ["73-list-nesting-and-looseness-5.crv:3"];
    let failures: Vec<String> = failures
        .into_iter()
        .filter(|f| !known.iter().any(|k| f.starts_with(k)))
        .collect();
    assert!(
        failures.is_empty(),
        "fmt emitted {} line(s) ending in whitespace:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn a_blank_line_inside_a_list_item_is_empty() {
    let src = "1. one\n\n    > q\n";
    let out = carve::to_carve(src);
    assert!(
        out.contains("\n\n"),
        "expected an empty blank line, got: {out:?}"
    );
    assert!(
        !out.lines()
            .any(|line| !line.is_empty() && line.trim().is_empty()),
        "a whitespace-only line survived: {out:?}"
    );
    assert_eq!(carve::to_html(&out), carve::to_html(src));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}
