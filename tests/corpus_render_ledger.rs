//! The Markdown, plain-text and ANSI output of EVERY corpus document, pinned.
//!
//! `corpus_render_fixtures.rs` asserts the reviewed bytes the spec ships, and
//! that population is thin: 39 documents on markdown, 13 on plain and 13 on
//! ansi out of 1740, plus the 35 self-regression cases under
//! `tests/fixtures/golden`. Every other document reaches these three renderers
//! only through `compare:impls` in the spec repository, which runs on a nightly
//! schedule and reports engine-to-engine DISAGREEMENT, so a regression this
//! engine makes on its own surfaces the next morning, elsewhere, beside
//! whatever else landed.
//!
//! A digest line is not a correctness claim; the reviewed fixtures are. It says
//! the output has not moved since it was recorded, and a change that moves it
//! has to rewrite the line, so the diff names every document affected.
//!
//! Regenerate with `UPDATE_RENDER_LEDGER=1 cargo test --test
//! corpus_render_ledger`. A moved document is a replaced line and a new one is
//! an added line.
//!
//! A document the ledger has never seen is tolerated while coverage holds. The
//! spec corpus grew by about 13 documents a day over the month to 2026-09-21
//! and the pin moves with it, so failing on an unrecorded document would put
//! most pin bumps red for bookkeeping. A regression moves output on documents
//! already recorded, so the signal survives a few unrecorded ones; a ledger
//! that has stopped describing the corpus does not, which is what
//! `COVERAGE_FLOOR` bounds.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Roughly a week of corpus growth. Below it the ledger needs regenerating.
const COVERAGE_FLOOR: f64 = 0.95;

const HEADER: &str = concat!(
    "# carve-rs non-HTML render ledger.\n",
    "# <slug> md:<digest> txt:<digest> ansi:<digest>, FNV-1a 64 as 16 hex.\n",
    "# Regenerate with `UPDATE_RENDER_LEDGER=1 cargo test --test corpus_render_ledger`.\n",
);

/// FNV-1a rather than a truncated sha-256, which is what carve-js and carve-php
/// record. Both are 64 bits wide against a change nobody is hiding, and this
/// crate has no dev-dependencies at all: a digest crate would be the first, for
/// a fixture file.
fn digest(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

struct Row {
    md: String,
    txt: String,
    ansi: String,
}

impl Row {
    fn get(&self, target: &str) -> &str {
        match target {
            "md" => &self.md,
            "txt" => &self.txt,
            "ansi" => &self.ansi,
            _ => unreachable!("the target list is a constant"),
        }
    }
}

const TARGETS: [&str; 3] = ["md", "txt", "ansi"];

fn ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus-render-ledger.txt")
}

fn slugs() -> Vec<String> {
    let dir = common::corpus_dir();
    let mut slugs: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("spec corpus unreadable at {}: {e}", dir.display()))
        .map(|entry| entry.expect("read corpus entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "crv"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    slugs.sort();
    slugs
}

fn rendered() -> BTreeMap<String, Row> {
    let dir = common::corpus_dir();
    slugs()
        .into_iter()
        .map(|slug| {
            let source = fs::read_to_string(dir.join(format!("{slug}.crv")))
                .unwrap_or_else(|e| panic!("corpus document {slug} unreadable: {e}"));
            let row = Row {
                md: digest(&carve::to_markdown(&source)),
                txt: digest(&carve::to_plain_text(&source)),
                ansi: digest(&carve::to_ansi(&source)),
            };
            (slug, row)
        })
        .collect()
}

fn serialize(rows: &BTreeMap<String, Row>) -> String {
    let mut out = String::from(HEADER);
    for (slug, row) in rows {
        out.push_str(&format!(
            "{slug} md:{} txt:{} ansi:{}\n",
            row.md, row.txt, row.ansi
        ));
    }
    out
}

fn parse_line(line: &str) -> Option<(String, Row)> {
    let mut parts = line.split(' ');
    let slug = parts.next()?;
    if slug.is_empty() {
        return None;
    }
    let mut digests = Vec::new();
    for target in TARGETS {
        let field = parts.next()?.strip_prefix(target)?.strip_prefix(':')?;
        if field.len() != 16
            || !field
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        {
            return None;
        }
        digests.push(field.to_owned());
    }
    if parts.next().is_some() {
        return None;
    }
    Some((
        slug.to_owned(),
        Row {
            md: digests[0].clone(),
            txt: digests[1].clone(),
            ansi: digests[2].clone(),
        },
    ))
}

/// The recorded rows, plus every line neither blank nor a comment nor a row.
fn read_ledger() -> (BTreeMap<String, Row>, Vec<String>) {
    let path = ledger_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no render ledger at {}: {e}", path.display()));
    let mut rows = BTreeMap::new();
    let mut unparsable = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(line) {
            Some((slug, row)) => {
                rows.insert(slug, row);
            }
            None => unparsable.push(line.to_owned()),
        }
    }
    (rows, unparsable)
}

#[test]
fn the_render_ledger_pins_every_corpus_document() {
    let rendered = rendered();

    // An update run rewrites the file before reading it back, so it is green by
    // construction. That is the point of the variable, and CI never sets it.
    if std::env::var("UPDATE_RENDER_LEDGER").as_deref() == Ok("1") {
        fs::write(ledger_path(), serialize(&rendered)).expect("write render ledger");
    }

    let (recorded, unparsable) = read_ledger();

    // The floor. Every sweep below asserts that a list came out empty, and an
    // unbuilt or empty submodule produces exactly that.
    assert!(
        unparsable.is_empty(),
        "unparsable ledger line(s): {unparsable:?}"
    );
    assert_eq!(
        rendered.len(),
        common::expected_corpus_size(),
        "the corpus is not the one tests/spec pins: run `git submodule update --init`"
    );

    let covered = rendered
        .keys()
        .filter(|slug| recorded.contains_key(*slug))
        .count();
    let coverage = covered as f64 / rendered.len() as f64;
    assert!(
        coverage >= COVERAGE_FLOOR,
        "the ledger covers {covered} of {} corpus documents: run \
         `UPDATE_RENDER_LEDGER=1 cargo test --test corpus_render_ledger`",
        rendered.len()
    );

    let gone: Vec<&String> = recorded
        .keys()
        .filter(|slug| !rendered.contains_key(*slug))
        .collect();
    assert!(
        gone.is_empty(),
        "ledger line(s) whose document is gone: {gone:?}"
    );

    let mut moved = Vec::new();
    for (slug, row) in &rendered {
        let Some(was) = recorded.get(slug) else {
            continue;
        };
        for target in TARGETS {
            if was.get(target) != row.get(target) {
                moved.push(format!(
                    "{slug} {target}: {} -> {}",
                    was.get(target),
                    row.get(target)
                ));
            }
        }
    }
    assert!(
        moved.is_empty(),
        "{} recorded output(s) moved. If the change is intended, run \
         `UPDATE_RENDER_LEDGER=1 cargo test --test corpus_render_ledger` and review \
         the rewritten lines:\n{}",
        moved.len(),
        moved.join("\n")
    );
}
