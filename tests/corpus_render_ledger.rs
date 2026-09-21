//! The Markdown, plain-text, and ANSI output of every spec-corpus document.
//!
//! Reviewed sidecars state which output is correct. This ledger instead pins
//! the remaining corpus population, so a renderer change must name every
//! document whose output it moves. Regenerate it with:
//!
//! ```text
//! UPDATE_RENDER_LEDGER=1 cargo test --test corpus_render_ledger
//! ```

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const COVERAGE_FLOOR: f64 = 0.95;
const HEADER: &str = "# carve-rs non-HTML render ledger.\n# <slug> md:<digest> txt:<digest> ansi:<digest>, sha-256 truncated to 16 hex.\n# Regenerate with `UPDATE_RENDER_LEDGER=1 cargo test --test corpus_render_ledger`.\n";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus")
}

fn ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus-render-ledger.txt")
}

fn digest(value: String) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))[..16].to_string()
}

fn rendered() -> BTreeMap<String, [String; 3]> {
    let mut rows = BTreeMap::new();
    for entry in fs::read_dir(corpus_dir()).expect("read spec corpus") {
        let path = entry.expect("read corpus entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("crv") {
            continue;
        }
        let slug = path.file_stem().unwrap().to_string_lossy().into_owned();
        let source = fs::read_to_string(path).expect("read corpus source");
        rows.insert(
            slug,
            [
                digest(carve::to_markdown(&source)),
                digest(carve::to_plain_text(&source)),
                digest(carve::to_ansi(&source)),
            ],
        );
    }
    rows
}

fn serialize(rows: &BTreeMap<String, [String; 3]>) -> String {
    let mut text = HEADER.to_owned();
    for (slug, hashes) in rows {
        text.push_str(&format!(
            "{slug} md:{} txt:{} ansi:{}\n",
            hashes[0], hashes[1], hashes[2]
        ));
    }
    text
}

fn recorded() -> Result<BTreeMap<String, [String; 3]>, Vec<String>> {
    let mut rows = BTreeMap::new();
    let mut invalid = Vec::new();
    for line in fs::read_to_string(ledger_path())
        .expect("read render ledger")
        .lines()
    {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        let [slug, md, txt, ansi] = parts.as_slice() else {
            invalid.push(line.to_owned());
            continue;
        };
        let hashes = [md, txt, ansi].map(|part| part.split_once(':'));
        if !matches!(
            hashes,
            [Some(("md", _)), Some(("txt", _)), Some(("ansi", _))]
        ) {
            invalid.push(line.to_owned());
            continue;
        }
        let hashes = hashes.map(|part| part.unwrap().1);
        let [md, txt, ansi] = hashes;
        if ![md, txt, ansi]
            .iter()
            .all(|hash| hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()))
        {
            invalid.push(line.to_owned());
            continue;
        }
        rows.insert(
            (*slug).to_owned(),
            [md.to_owned(), txt.to_owned(), ansi.to_owned()],
        );
    }
    invalid.is_empty().then_some(rows).ok_or(invalid)
}

#[test]
fn every_non_html_corpus_render_is_pinned() {
    let actual = rendered();
    assert!(!actual.is_empty(), "spec corpus is empty or unavailable");

    if std::env::var("UPDATE_RENDER_LEDGER").as_deref() == Ok("1") {
        fs::write(ledger_path(), serialize(&actual)).expect("write render ledger");
    }

    let expected =
        recorded().unwrap_or_else(|lines| panic!("unparsable ledger line(s): {lines:?}"));
    let covered = actual
        .keys()
        .filter(|slug| expected.contains_key(*slug))
        .count();
    assert!(
        covered as f64 / actual.len() as f64 >= COVERAGE_FLOOR,
        "ledger covers {covered} of {} corpus documents; regenerate it",
        actual.len()
    );

    let stale: Vec<_> = expected
        .keys()
        .filter(|slug| !actual.contains_key(*slug))
        .collect();
    assert!(
        stale.is_empty(),
        "ledger names removed document(s): {stale:?}"
    );

    let moved: Vec<_> = actual
        .iter()
        .filter_map(|(slug, hashes)| {
            expected
                .get(slug)
                .filter(|old| *old != hashes)
                .map(|old| (slug, old, hashes))
        })
        .collect();
    assert!(
        moved.is_empty(),
        "{} recorded render(s) moved; regenerate and review the ledger",
        moved.len()
    );
}
