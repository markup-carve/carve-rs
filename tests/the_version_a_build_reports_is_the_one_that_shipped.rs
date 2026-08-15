//! The version this build reports is the version that shipped.
//!
//! A library's version constant is read by people who cannot see the build:
//! `carve fmt --stamp` writes it into a document as `generated-by: carve-rs
//! <version>`, and an embedder prints it in a bug report. When it names a
//! release that is not the one running, every downstream conclusion drawn from
//! it is wrong, and the reader has no way to notice - they suspect their own
//! build first. carve-js shipped `LIB_VERSION = '0.1.0'` through three releases
//! for exactly this reason (`markup-carve/carve-js#1074`, reported by an
//! embedder); the comment guarding it said "keep in sync with package.json on
//! release", which is an instruction, not a check.
//!
//! This engine has no hand-written release constant - it reports
//! `CARGO_PKG_VERSION`, which cargo derives from the manifest - so the drift
//! carve-js had is structurally impossible here. What is NOT impossible is the
//! surrounding set going out of step: the manifest against the changelog
//! section a release cuts, and the hand-written `SPEC_VERSION` against the
//! grammar it claims to implement. Those are the sides checked below.
//!
//! `.github/workflows/release.yml` runs a tag-versus-manifest guard, but only
//! once the tag exists, i.e. after the decision it would correct. These run on
//! every push.
//!
//! Every assertion reads BOTH of its sides from a file at run time. None of
//! them compares anything to a version literal written in this file - a literal
//! would have to be edited on release too, which is the defect, not the fix.

use std::fs;
use std::path::PathBuf;

fn repo_file(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}. This gate compares two files against each \
             other; a missing side means the comparison did not happen.",
            path.display()
        )
    })
}

/// The newest CUT changelog section, i.e. the first `## [X.Y.Z]` heading,
/// skipping the open `## [Unreleased]` one. That heading is what the release
/// process writes when it cuts a release, so it is an independently maintained
/// record of the last version this repo shipped.
fn newest_released_changelog_version(changelog: &str) -> String {
    for line in changelog.lines() {
        let Some(rest) = line.strip_prefix("## ") else {
            continue;
        };
        let heading = rest.trim().trim_start_matches('[');
        let version = heading
            .split(']')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if version.starts_with(|c: char| c.is_ascii_digit()) {
            return version.to_string();
        }
    }
    panic!("CHANGELOG.md has no cut '## [X.Y.Z]' section");
}

/// The `Version:` field in the grammar header, which `docs/versioning.md` names
/// as the spec's version.
fn grammar_version(grammar: &str) -> String {
    for line in grammar.lines() {
        if let Some(rest) = line.trim().strip_prefix("Version:") {
            return rest.trim().to_string();
        }
    }
    panic!("resources/grammar.ebnf has no 'Version:' field");
}

#[test]
fn the_version_this_build_reports_is_the_newest_released_changelog_section() {
    // The left side is the manifest's `version`, which is what an embedder,
    // `--version` and the provenance stamp all end up reading. The right side is
    // the heading the release process cuts. They are maintained in different
    // files by different steps, so this fails from either direction.
    let reported = env!("CARGO_PKG_VERSION");
    let changelog = newest_released_changelog_version(&repo_file("CHANGELOG.md"));

    assert_eq!(
        reported, changelog,
        "this build reports version {reported}, but the newest cut CHANGELOG \
         section is {changelog}. Either the release bumped Cargo.toml without \
         cutting the changelog, or it cut the changelog without bumping \
         Cargo.toml; RELEASING.md does both in one step."
    );
}

#[test]
fn the_spec_version_constant_matches_the_vendored_grammar() {
    let grammar = grammar_version(&repo_file("tests/spec/resources/grammar.ebnf"));

    assert_eq!(
        carve::SPEC_VERSION,
        grammar,
        "SPEC_VERSION says this engine implements Carve {}, but the vendored \
         grammar is Carve {grammar}. SPEC_VERSION is what `carve fmt --stamp` \
         writes into a document and what `needs_review` compares an old stamp \
         against, so a stale value tells a reader their document is current \
         when it is not.",
        carve::SPEC_VERSION
    );
}
