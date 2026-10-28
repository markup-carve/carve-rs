#![cfg(feature = "experimental-event-parser-options")]

use super::*;

#[test]
fn admitted_corpus_preserves_parser_diagnostics() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let options = Options::default().with_positions(true);
    for entry in std::fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension() != Some(std::ffi::OsStr::new("crv")) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let ((expected, expected_unattached), expected_events) = collecting_lint_events(|| {
            collecting_unattached_block_attrs(|| parse_with_options(&source, &options))
        });
        let (((actual, admitted), actual_unattached), actual_events) =
            collecting_lint_events(|| {
                collecting_unattached_block_attrs(|| {
                    parse_document_with_options(&source, &options).unwrap()
                })
            });
        assert!(admitted, "{}", path.display());
        assert_eq!(actual, expected, "{}", path.display());
        assert_eq!(actual_unattached, expected_unattached, "{}", path.display());
        assert_eq!(actual_events, expected_events, "{}", path.display());
    }
    let pieces = [
        "- a\n",
        "  - b\n",
        "```\nx\n```\n",
        "{.x}\n",
        "+\n",
        "> q\n",
        "::: note\nx\n:::\n",
        "\n",
    ];
    let mut admitted = 0;
    for first in pieces {
        for second in pieces {
            for third in pieces {
                let source = format!("{first}{second}{third}");
                let ((expected, expected_unattached), expected_events) =
                    collecting_lint_events(|| {
                        collecting_unattached_block_attrs(|| parse_with_options(&source, &options))
                    });
                let (candidate, actual_events) = collecting_lint_events(|| {
                    collecting_unattached_block_attrs(|| {
                        parse_document_with_options(&source, &options)
                    })
                });
                let (Some((actual, _)), actual_unattached) = candidate else {
                    continue;
                };
                admitted += 1;
                assert_eq!(actual, expected, "{source:?}");
                assert_eq!(actual_unattached, expected_unattached, "{source:?}");
                assert_eq!(actual_events, expected_events, "{source:?}");
            }
        }
    }
    assert_eq!(admitted, pieces.len().pow(3));
    for source in [
        "::: note\n{.inner}\n:::\n\n{.outer}\n",
        "::: note\n{.inner}\n:::\n- a\n\n  {.item}\n",
        "::: note\n```\nx\n:::\n\n- a\n\n  ```\n  y\n",
    ] {
        let ((expected, expected_unattached), expected_events) = collecting_lint_events(|| {
            collecting_unattached_block_attrs(|| parse_with_options(source, &options))
        });
        let (((actual, admitted), actual_unattached), actual_events) =
            collecting_lint_events(|| {
                collecting_unattached_block_attrs(|| {
                    parse_document_with_options(source, &options).unwrap()
                })
            });
        assert!(admitted, "{source:?}");
        assert_eq!(actual, expected, "{source:?}");
        assert_eq!(actual_unattached, expected_unattached, "{source:?}");
        assert_eq!(actual_events, expected_events, "{source:?}");
    }
}
