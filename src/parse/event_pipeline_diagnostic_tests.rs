#![cfg(feature = "experimental-event-parser-options")]

use super::*;
use crate::{BlockMatch, CarveExtension, MatcherContext};

#[test]
fn depth_exhaustion_declines_before_extension_callbacks() {
    use std::cell::Cell;

    struct Probe(Cell<usize>);
    impl CarveExtension for Probe {
        fn name(&self) -> &'static str {
            "depth-probe"
        }

        fn match_block(&self, _: &[&str], _: usize, _: &MatcherContext<'_>) -> Option<BlockMatch> {
            self.0.set(self.0.get() + 1);
            None
        }
    }

    let probe = Probe(Cell::new(0));
    let options = Options::default().with_extension(&probe);
    let _depth = DepthScope::set(MAX_NESTING_DEPTH);
    for source in [":: t\n: ```\ncode\n```\n", "[^n]: text\n\nsee[^n]\n"] {
        assert!(parse_document_with_options(source, &options).is_none());
    }
    assert_eq!(probe.0.get(), 0);
}

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
        // Rule A: the event route declines a document the ownership walk rewrites.
        if walk_moves_lines(&normalize_source(&source)) {
            continue;
        }
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
    let mut walked = 0;
    for first in pieces {
        for second in pieces {
            for third in pieces {
                let source = format!("{first}{second}{third}");
                // Rule A: the event route declines a document the walk rewrites.
                if walk_moves_lines(&source) {
                    walked += 1;
                    continue;
                }
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
    assert_eq!(admitted + walked, pieces.len().pow(3));
    // A piece with a lone `+` now always walks, so these no longer outnumber the walked.
    assert!(
        admitted > walked / 2,
        "{admitted} admitted, {walked} walked"
    );
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

#[test]
fn extension_blocks_preserve_parser_diagnostics() {
    struct Marker;
    impl CarveExtension for Marker {
        fn name(&self) -> &'static str {
            "diagnostic-marker"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            (lines.get(start) == Some(&"@@@")).then(|| BlockMatch {
                node: BlockNode::Paragraph(Paragraph::default()),
                lines_consumed: 1,
            })
        }
    }

    let marker = Marker;
    let options = Options::default()
        .with_positions(true)
        .with_extension(&marker);
    for source in ["{.x}\n@@@\n", "{.x}\n@@@\n\n{.y}\n", "@@@\n\n{.x}\n"] {
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

#[test]
fn eof_fences_preserve_parser_diagnostics() {
    let options = Options::default().with_positions(true);
    for source in [
        "```\nbody",
        "[^n]: note\n\n```\nbody",
        "> - outer\n>   - head\n>\n>     ```\n>     a\n>   ```\n>\n>   tail",
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
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    for entry in std::fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension() != Some(std::ffi::OsStr::new("crv")) {
            continue;
        }
        let authored = std::fs::read_to_string(&path).unwrap();
        let source = authored.strip_suffix('\n').unwrap_or(&authored);
        // Rule A: the event route declines a document the ownership walk rewrites.
        if walk_moves_lines(&normalize_source(source)) {
            continue;
        }
        let ((expected, expected_unattached), expected_events) = collecting_lint_events(|| {
            collecting_unattached_block_attrs(|| parse_with_options(source, &options))
        });
        let (((actual, admitted), actual_unattached), actual_events) =
            collecting_lint_events(|| {
                collecting_unattached_block_attrs(|| {
                    parse_document_with_options(source, &options).unwrap()
                })
            });
        assert!(admitted, "{}", path.display());
        assert_eq!(actual, expected, "{}", path.display());
        assert_eq!(actual_unattached, expected_unattached, "{}", path.display());
        assert_eq!(actual_events, expected_events, "{}", path.display());
    }
}
