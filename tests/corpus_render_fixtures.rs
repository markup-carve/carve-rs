//! Every committed non-HTML spec fixture is a PR gate in this engine.
//!
//! The cross-engine nightly remains the broad agreement check. This sweep is
//! deliberately smaller: it reads only reviewed golden files, so a renderer
//! change that moves one gets feedback in the repository that made the change.

use std::fs;
use std::path::{Path, PathBuf};

/// Fixtures this writer is AHEAD of, as `(slug, target, reason, ahead)`.
///
/// The list was `fmt`-only, and the target is a column now because the same
/// situation reached `md`: a corpus sidecar is hand-written and checked in the
/// spec repository against the pinned carve-js build, so a ruling this engine
/// implements first leaves the sidecar describing the other engine until that
/// one catches up and the pin moves.
///
/// TWO COLUMNS THAT WERE NOT HERE, and each closes a way the list could not do
/// its job. `ahead` is what this writer emits TODAY, so the document is still
/// pinned while it is declared - as a bare skip it was pinned by nothing, and
/// these four are the only `.fmt` fixtures holding a definition body.
///
/// Measured, by reverting the writer to the two-space separator this repo
/// shipped before markup-carve/carve#1757: this sweep stayed GREEN. The four
/// fixtures matched their sidecars again, so the skip was never even reached
/// and the regression passed through a sweep whose whole job is to notice it.
///
/// And the entry now RETIRES. The skip lived inside the `actual != expected`
/// branch, so a slug whose fixture had caught up was never reached and its line
/// survived forever - which is the same green above, read the other way round.
/// The check below is made outside that branch for exactly that reason.
const AHEAD_OF_PIN: &[(&str, &str, &str, &str)] = &[];

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus")
}

fn render(target: &str, source: &str) -> String {
    match target {
        "md" => carve::to_markdown(source),
        "txt" => carve::to_plain_text(source),
        "ansi" => carve::to_ansi(source),
        "fmt" => carve::to_carve(source),
        _ => unreachable!("the discovery list controls the target set"),
    }
}

#[test]
fn every_non_html_spec_fixture_matches() {
    let dir = corpus_dir();
    assert!(dir.is_dir(), "spec corpus not found at {}", dir.display());

    let mut fixtures = Vec::new();
    for entry in fs::read_dir(&dir).expect("read spec corpus") {
        let path = entry.expect("read corpus entry").path();
        let Some(ext) = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if !matches!(ext.as_str(), "md" | "txt" | "ansi" | "fmt") {
            continue;
        }
        let is_numbered = path
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|stem| {
                stem.split_once('-')
                    .is_some_and(|(number, _)| number.chars().all(|c| c.is_ascii_digit()))
            });
        if is_numbered {
            fixtures.push((path, ext));
        }
    }
    fixtures.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        !fixtures.is_empty(),
        "no non-HTML render fixtures discovered"
    );

    let mut failures = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for (fixture, target) in fixtures {
        let slug = fixture.file_stem().unwrap().to_string_lossy();
        let source_path = dir.join(format!("{slug}.crv"));
        assert!(
            source_path.is_file(),
            "{} has no .crv source pair",
            fixture.display()
        );
        seen.push((slug.to_string(), target.clone()));
        let source = fs::read_to_string(&source_path).expect("read corpus source");
        let expected = fs::read_to_string(&fixture).expect("read render fixture");
        let actual = render(&target, &source);
        let declared = AHEAD_OF_PIN
            .iter()
            .find(|(name, kind, _, _)| *name == slug.as_ref() && *kind == target);
        if let Some((_, _, reason, ahead)) = declared {
            // OUTSIDE THE MISMATCH BRANCH. Both halves are asked on every run:
            // the writer still emits what the declaration says, AND the fixture
            // still disagrees. The second is what retires the entry when the pin
            // moves past it.
            if actual != *ahead {
                failures.push(format!(
                    "{slug}.{target} ({reason})\n--- declared ahead ---\n{ahead:?}\n\
                     --- actual ---\n{actual:?}"
                ));
            } else if actual == expected {
                failures.push(format!(
                    "{slug}.{target}: the pin has caught up; delete its AHEAD_OF_PIN entry"
                ));
            }
            continue;
        }
        if actual != expected {
            failures.push(format!(
                "{slug}.{target}\n--- expected ---\n{expected:?}\n--- actual ---\n{actual:?}"
            ));
        }
    }

    let missing: Vec<_> = AHEAD_OF_PIN
        .iter()
        .filter(|(slug, target, _, _)| {
            !seen.contains(&((*slug).to_string(), (*target).to_string()))
        })
        .map(|(slug, target, _, _)| format!("{slug}.{target}"))
        .collect();
    assert!(
        missing.is_empty(),
        "AHEAD_OF_PIN names fixture(s) the corpus does not have: {missing:?}",
    );
    assert!(
        failures.is_empty(),
        "non-HTML spec fixture mismatch(es):\n{}",
        failures.join("\n\n")
    );
}
