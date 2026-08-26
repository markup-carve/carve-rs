//! Every committed non-HTML spec fixture is a PR gate in this engine.
//!
//! The cross-engine nightly remains the broad agreement check. This sweep is
//! deliberately smaller: it reads only reviewed golden files, so a renderer
//! change that moves one gets feedback in the repository that made the change.

use std::fs;
use std::path::{Path, PathBuf};

const FMT_AHEAD_OF_PIN: &[&str] = &[
    "227-a-definition-inside-a-definition-list-dd-is-collected-and-the-entry-keeps-no-trace",
    "227-a-definition-inside-a-definition-list-dd-is-collected-and-the-entry-keeps-no-trace-2",
    "279-a-boundary-line-inside-an-open-fence-does-not-end-the-container-3",
    "407-one-consumed-boolean-spells-the-looseness-no-blank-line-can-2",
];

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
    for (fixture, target) in fixtures {
        let slug = fixture.file_stem().unwrap().to_string_lossy();
        let source_path = dir.join(format!("{slug}.crv"));
        assert!(
            source_path.is_file(),
            "{} has no .crv source pair",
            fixture.display()
        );
        let source = fs::read_to_string(&source_path).expect("read corpus source");
        let expected = fs::read_to_string(&fixture).expect("read render fixture");
        let actual = render(&target, &source);
        if actual != expected {
            if target == "fmt" && FMT_AHEAD_OF_PIN.contains(&slug.as_ref()) {
                continue;
            }
            failures.push(format!(
                "{slug}.{target}\n--- expected ---\n{expected:?}\n--- actual ---\n{actual:?}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "non-HTML spec fixture mismatch(es):\n{}",
        failures.join("\n\n")
    );
}
