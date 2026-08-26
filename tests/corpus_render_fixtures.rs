//! Every committed non-HTML spec fixture is a PR gate in this engine.
//!
//! The cross-engine nightly remains the broad agreement check. This sweep is
//! deliberately smaller: it reads only reviewed golden files, so a renderer
//! change that moves one gets feedback in the repository that made the change.

use std::fs;
use std::path::{Path, PathBuf};

/// `fmt` fixtures this writer is AHEAD of, as `(slug, reason, ahead)`.
///
/// TWO COLUMNS THAT WERE NOT HERE, and each closes a way the list could not do
/// its job. `ahead` is what this writer emits TODAY, so the document is still
/// pinned while it is declared - as a bare skip it was pinned by nothing, and
/// these four are the only `.fmt` fixtures holding a definition body, so with
/// all four skipped the canonical form of a definition body had no corpus gate
/// at all: a writer emitting `:GARBAGE` with a six-column body left this sweep
/// and `corpus_canonical_form` both green (measured).
///
/// And the entry now RETIRES. The skip lived inside the `actual != expected`
/// branch, so once the pin caught up and the two agreed the branch was never
/// reached and the line survived forever - a slug whose fixture already matched
/// sat in this list with the suite green (measured). The check below is made
/// outside that branch for exactly that reason.
const FMT_AHEAD_OF_PIN: &[(&str, &str, &str)] = &[
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
    let mut seen_fmt: Vec<String> = Vec::new();
    for (fixture, target) in fixtures {
        let slug = fixture.file_stem().unwrap().to_string_lossy();
        let source_path = dir.join(format!("{slug}.crv"));
        assert!(
            source_path.is_file(),
            "{} has no .crv source pair",
            fixture.display()
        );
        if target == "fmt" {
            seen_fmt.push(slug.to_string());
        }
        let source = fs::read_to_string(&source_path).expect("read corpus source");
        let expected = fs::read_to_string(&fixture).expect("read render fixture");
        let actual = render(&target, &source);
        let declared = (target == "fmt")
            .then(|| {
                FMT_AHEAD_OF_PIN
                    .iter()
                    .find(|(name, _, _)| *name == slug.as_ref())
            })
            .flatten();
        if let Some((_, reason, ahead)) = declared {
            // OUTSIDE THE MISMATCH BRANCH. Both halves are asked on every run:
            // the writer still emits what the declaration says, AND the fixture
            // still disagrees. The second is what retires the entry when the pin
            // moves past it.
            if actual != *ahead {
                failures.push(format!(
                    "{slug}.fmt ({reason})\n--- declared ahead ---\n{ahead:?}\n\
                     --- actual ---\n{actual:?}"
                ));
            } else if actual == expected {
                failures.push(format!(
                    "{slug}.fmt: the pin has caught up; delete its FMT_AHEAD_OF_PIN entry"
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

    let missing: Vec<_> = FMT_AHEAD_OF_PIN
        .iter()
        .map(|(slug, _, _)| *slug)
        .filter(|slug| !seen_fmt.contains(&slug.to_string()))
        .collect();
    assert!(
        missing.is_empty(),
        "FMT_AHEAD_OF_PIN names fixture(s) the corpus does not have: {missing:?}",
    );
    assert!(
        failures.is_empty(),
        "non-HTML spec fixture mismatch(es):\n{}",
        failures.join("\n\n")
    );
}
