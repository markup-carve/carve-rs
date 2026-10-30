use super::*;

const MIXED_QUOTE_DEPTH_LIMIT: usize = 32;
// The direct quote helpers recurse through `parse_mixed_blocks` per level.
pub(super) const DIRECT_QUOTE_DEPTH_LIMIT: usize = 8;
// Event frames that may sit on the native stack at once. The mapped event
// frame is larger than the standard block frame; logical depth is capped
// separately by `MAX_NESTING_DEPTH`, exactly as on the standard reader.
#[cfg(feature = "experimental-event-parser-options")]
pub(super) const EVENT_FRAME_LIMIT: usize = 8;
// Lines that may keep one quote level eager; see `deep_quote_chain_owned`.
// The count bounds the eager quotes on any one chain, and each holds one event
// frame, so a deep chain peaks at budget + 1 frames. Exhausting them would hand
// the remaining ~190 levels to the recursive standard reader on top. Those
// frames can sit under `DIRECT_QUOTE_DEPTH_LIMIT` direct helper levels, which
// the count does not see; the release-stack tests run that stacking.
#[cfg(feature = "experimental-event-parser-options")]
pub(super) const DEEP_QUOTE_EAGER_BUDGET: usize = 3;
#[cfg(feature = "experimental-event-parser-options")]
const _: () = assert!(2 * (DEEP_QUOTE_EAGER_BUDGET + 1) <= EVENT_FRAME_LIMIT);

// Event frames on the native stack. A worklist body restores the logical depth
// it was collected at, so that depth no longer measures the stack under it.
#[cfg(feature = "experimental-event-parser-options")]
thread_local! {
    static EVENT_FRAMES: Cell<usize> = const { Cell::new(0) };
}

#[cfg(all(feature = "experimental-event-parser-options", test))]
thread_local! {
    pub(super) static PEAK_EVENT_FRAMES: Cell<usize> = const { Cell::new(0) };
    // The deepest logical level an event frame was entered at.
    pub(super) static PEAK_EVENT_DEPTH: Cell<usize> = const { Cell::new(0) };
    // The highest `stacked_depth` an unquoted body was admitted at.
    pub(super) static PEAK_UNQUOTED_STACKED_DEPTH: Cell<usize> = const { Cell::new(0) };
    pub(super) static EVENT_FRAME_REFUSALS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(feature = "experimental-event-parser-options")]
pub(super) struct EventFrame;

#[cfg(feature = "experimental-event-parser-options")]
impl EventFrame {
    fn enter() -> Self {
        EVENT_FRAMES.with(|frames| frames.set(frames.get() + 1));
        #[cfg(test)]
        {
            PEAK_EVENT_FRAMES.with(|peak| peak.set(peak.get().max(EVENT_FRAMES.with(Cell::get))));
            let depth = NESTING_DEPTH.with(Cell::get);
            PEAK_EVENT_DEPTH.with(|peak| peak.set(peak.get().max(depth)));
        }
        EventFrame
    }
}

/// Occupy `count` event frames, as an eager chain above the caller would.
#[cfg(all(feature = "experimental-event-parser-options", test))]
pub(super) fn hold_event_frames(count: usize) -> Vec<EventFrame> {
    (0..count).map(|_| EventFrame::enter()).collect()
}

#[cfg(feature = "experimental-event-parser-options")]
impl Drop for EventFrame {
    fn drop(&mut self) {
        EVENT_FRAMES.with(|frames| frames.set(frames.get().saturating_sub(1)));
    }
}

/// May a nested body take an event frame here? Asked before the cursor reads
/// a line, so a refusal cannot repeat an extension callback.
///
/// `EVENT_FRAME_LIMIT` bounds the event frames on the native stack. An
/// unquoted body is also held to `EVENT_FRAME_LIMIT` levels above
/// `WORKLIST_BASE`: at the root that is the old logical gate, so an eager
/// chain (a definition body, a `::: quote` block, a ladder tail, a drain)
/// takes event frames only near the top and never on top of a deep standard
/// recursion. A body popped from the worklist restores its depth over a base
/// that discounts the levels with no frame under them, so it reads on the
/// event cursor however deep it sits. A quoted body keeps the frame count
/// alone, as before; its deep chains are bounded by `DEEP_QUOTE_EAGER_BUDGET`.
/// The direct quote helpers stay gated by logical depth.
#[cfg(feature = "experimental-event-parser-options")]
fn event_frame_admitted(quoted: bool) -> bool {
    let admitted = EVENT_FRAMES.with(Cell::get) < EVENT_FRAME_LIMIT
        && NESTING_DEPTH.with(Cell::get) < MAX_NESTING_DEPTH
        && (quoted || stacked_depth() < EVENT_FRAME_LIMIT);
    #[cfg(test)]
    if admitted && !quoted {
        PEAK_UNQUOTED_STACKED_DEPTH.with(|peak| peak.set(peak.get().max(stacked_depth())));
    }
    #[cfg(test)]
    if !admitted {
        EVENT_FRAME_REFUSALS.with(|count| count.set(count.get() + 1));
    }
    admitted
}

#[cfg(test)]
thread_local! {
    pub(super) static NESTED_EXTENSION_EVENT_PARSES: Cell<usize> = const { Cell::new(0) };
    pub(super) static LADDER_TAIL_EVENT_PARSES: Cell<usize> = const { Cell::new(0) };
    pub(super) static COLON_BODY_EVENT_PARSES: Cell<usize> = const { Cell::new(0) };
    pub(super) static DIRECT_QUOTE_PARSES: Cell<usize> = const { Cell::new(0) };
    pub(super) static OBSERVED_DEFINITION_DEFERRALS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MARKER_HOSTED_DEFINITION_LISTS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MARKER_HOSTED_QUOTE_DEFERRALS: Cell<usize> = const { Cell::new(0) };
    pub(super) static COLON_QUOTE_DEFERRALS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
#[path = "event_pipeline_diagnostic_tests.rs"]
mod event_diagnostic_parity_tests;

#[cfg(test)]
mod standard_reader_probe_tests {
    use super::*;
    use crate::{BlockMatch, CarveExtension, MatcherContext};

    struct Observer(RefCell<Vec<(usize, String)>>);

    impl CarveExtension for Observer {
        fn name(&self) -> &'static str {
            "single-paragraph-observer"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            self.0.borrow_mut().push((start, lines[start].to_owned()));
            None
        }
    }

    #[test]
    fn reference_captions_in_colon_bodies_match_standard_reader() {
        for body in [
            "![a][r]\n^ *cap*\n",
            "![a][r]\n^ *cap*\n\n",
            "![a][r]\n^ *cap*\n\nnext\n",
            "![a][r]\n^ *cap*\nnext\n",
            "![a][r]\n\n^ *cap*\n",
        ] {
            for resolved in [false, true] {
                let source = format!(
                    "::: box\n{body}:::\n{}",
                    if resolved { "\n[r]: /u\n" } else { "" }
                );
                for (positions, source_lines) in
                    [(false, false), (true, false), (false, true), (true, true)]
                {
                    let observer = Observer(RefCell::new(Vec::new()));
                    let options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&observer);
                    STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                    let ((actual, admitted), diagnostics) = collecting_lint_events(|| {
                        parse_document_with_options(&source, &options).unwrap()
                    });
                    assert!(admitted, "{source:?}");
                    let entries = STANDARD_PARAGRAPH_ENTRIES.with(Cell::get);
                    let callbacks = observer.0.into_inner();
                    let standard_observer = Observer(RefCell::new(Vec::new()));
                    let standard_options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&standard_observer);
                    let (expected, standard_diagnostics) = {
                        let _standard = StandardEventRootGuard::install();
                        collecting_lint_events(|| parse_with_options(&source, &standard_options))
                    };
                    assert_eq!(
                        (actual, diagnostics),
                        (expected, standard_diagnostics),
                        "{source:?}"
                    );
                    assert_eq!(callbacks, standard_observer.0.into_inner(), "{source:?}");
                    if body == "![a][r]\n^ *cap*\n\n" {
                        assert_eq!(entries, 0, "{source:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn reference_caption_in_deep_colon_body_uses_event_reader() {
        let mut source = "::: box\n".repeat(150);
        source.push_str("![a][r]\n^ *cap*\n\n");
        source.push_str(&":::\n".repeat(150));
        source.push_str("\n[r]: /u\n");
        for positions in [false, true] {
            let options = Options::default()
                .with_positions(positions)
                .with_source_lines(true);
            STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
            let (actual, admitted) = parse_document_with_options(&source, &options).unwrap();
            assert!(admitted);
            assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0);
            let expected = {
                let _standard = StandardEventRootGuard::install();
                parse_with_options(&source, &options)
            };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn blank_image_captions_match_standard_reader_without_paragraph_entry() {
        for image in ["![a](/u)", "![a][r]"] {
            for resolved in [false, true] {
                for tail in ["", "\n", "\nnext\n", "\n\nnext\n"] {
                    let source = format!(
                        "::: box\n{image}\n\n^ *cap*\n{tail}:::\n{}",
                        if resolved { "\n[r]: /u\n" } else { "" }
                    );
                    for (positions, source_lines) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        let observer = Observer(RefCell::new(Vec::new()));
                        let options = Options::default()
                            .with_positions(positions)
                            .with_source_lines(source_lines)
                            .with_extension(&observer);
                        STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                        let ((actual, admitted), diagnostics) = collecting_lint_events(|| {
                            parse_document_with_options(&source, &options).unwrap()
                        });
                        assert!(admitted, "{source:?}");
                        let entries = STANDARD_PARAGRAPH_ENTRIES.with(Cell::get);
                        let callbacks = observer.0.into_inner();
                        let standard_observer = Observer(RefCell::new(Vec::new()));
                        let standard_options = Options::default()
                            .with_positions(positions)
                            .with_source_lines(source_lines)
                            .with_extension(&standard_observer);
                        let (expected, standard_diagnostics) = {
                            let _standard = StandardEventRootGuard::install();
                            collecting_lint_events(|| {
                                parse_with_options(&source, &standard_options)
                            })
                        };
                        assert_eq!(
                            (actual, diagnostics),
                            (expected, standard_diagnostics),
                            "{source:?}"
                        );
                        assert_eq!(callbacks, standard_observer.0.into_inner(), "{source:?}");
                        if tail.starts_with('\n') {
                            assert_eq!(entries, 0, "{source:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn blank_image_caption_in_deep_colon_body_uses_default_stack() {
        let mut source = "::: box\n".repeat(150);
        source.push_str("![a](/u)\n\n^ cap\n\n");
        source.push_str(&":::\n".repeat(150));
        STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
        let (actual, admitted) = parse_document_with_options(&source, &Options::default()).unwrap();
        assert!(admitted);
        assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0);
        let expected = {
            let _standard = StandardEventRootGuard::install();
            parse_with_options(&source, &Options::default())
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn nested_mapped_plain_paragraphs_match_standard_reader() {
        for wrapper in [
            "::: note\n{body}\n:::\n",
            "> {body}\n",
            "- {body}\n",
            "term\n: {body}\n",
        ] {
            for body in [
                "alpha",
                "alpha beta",
                "alpha!",
                "alpha, beta.\ngamma?",
                "alpha\n\nbeta",
                "alpha\n:: definition",
                "alpha\n::: container",
                "alpha\n> quote",
                "alpha\n- item",
                "alpha\n1. item",
                "alpha\n``` fence",
                "alpha\n# heading",
                "alpha\n!include file",
                "alpha\n+",
                "alpha\n: separator",
                "alpha\n---",
                "alpha\n alpha",
                "alpha\n\talpha",
                "alpha\n!!!",
            ] {
                let source = wrapper.replace("{body}", body);
                let options = Options::default();
                STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                let ((actual, admitted), diagnostics) = collecting_lint_events(|| {
                    parse_document_with_options(&source, &options).unwrap()
                });
                assert!(admitted, "{source:?}");
                let expected_entries = STANDARD_PARAGRAPH_ENTRIES.with(Cell::get);
                let (expected, standard_diagnostics) = {
                    let _standard = StandardEventRootGuard::install();
                    collecting_lint_events(|| parse_with_options(&source, &options))
                };
                assert_eq!(
                    (actual, diagnostics),
                    (expected, standard_diagnostics),
                    "{source:?}"
                );
                if body == "alpha" && wrapper.starts_with(":::") {
                    assert_eq!(expected_entries, 0, "{source:?}");
                }

                let observer = Observer(RefCell::new(Vec::new()));
                let event_options = Options::default().with_extension(&observer);
                let (event_doc, _) = parse_document_with_options(&source, &event_options).unwrap();
                let event_callbacks = observer.0.into_inner();
                let observer = Observer(RefCell::new(Vec::new()));
                let standard_options = Options::default().with_extension(&observer);
                let standard_doc = {
                    let _standard = StandardEventRootGuard::install();
                    parse_with_options(&source, &standard_options)
                };
                assert_eq!(event_doc, standard_doc, "{source:?}");
                assert_eq!(event_callbacks, observer.0.into_inner(), "{source:?}");
            }
        }
    }

    #[test]
    fn plain_colon_body_owns_paragraph_with_strict_parity() {
        for source in [
            "::: note\nalpha\n:::\n",
            "::: note\nalpha beta\n:::\n",
            "::: note\nalpha!\n:::\n",
            "::: note\nalpha, beta.\ngamma?\n:::\n",
            "::: note\nalpha\n\nbeta\n:::\n",
            "::: note\nalpha *bold* and _italic_\n:::\n",
            "::: note\ncafé κόσμος\n:::\n",
            "::: note\nalpha [link](https://example.org/a)\n:::\n",
            "::: note\nalpha ![alt](image.png)\n:::\n",
            "::: note\nalpha \\*literal\\*\n:::\n",
            "::: note\nalpha *bold*\nbeta [link](https://example.org)\n:::\n",
        ] {
            for (positions, source_lines) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let options = Options {
                    positions,
                    source_lines,
                    ..Options::default()
                };
                STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                let ((actual, admitted), event_diagnostics) = collecting_lint_events(|| {
                    parse_document_with_options(source, &options).unwrap()
                });
                assert!(admitted, "{source:?}");
                assert_eq!(
                    STANDARD_PARAGRAPH_ENTRIES.with(Cell::get),
                    0,
                    "{source:?}, positions={positions}, source_lines={source_lines}"
                );
                let (expected, standard_diagnostics) = {
                    let _standard = StandardEventRootGuard::install();
                    collecting_lint_events(|| parse_with_options(source, &options))
                };
                assert_eq!(event_diagnostics, standard_diagnostics, "{source:?}");
                assert_eq!(
                    actual, expected,
                    "{source:?}, positions={positions}, source_lines={source_lines}"
                );

                let event_observer = Observer(RefCell::new(Vec::new()));
                let event_options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&event_observer);
                let (event_doc, admitted) =
                    parse_document_with_options(source, &event_options).unwrap();
                assert!(admitted, "{source:?}");
                let standard_observer = Observer(RefCell::new(Vec::new()));
                let standard_options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&standard_observer);
                let standard_doc = {
                    let _standard = StandardEventRootGuard::install();
                    parse_with_options(source, &standard_options)
                };
                assert_eq!(event_doc, standard_doc, "{source:?}");
                let event_callbacks = event_observer.0.into_inner();
                let expected_calls = if source.contains("\n\nbeta") { 2 } else { 1 };
                assert_eq!(
                    event_callbacks.len(),
                    expected_calls,
                    "{source:?}: {event_callbacks:?}"
                );
                assert_eq!(
                    event_callbacks,
                    standard_observer.0.into_inner(),
                    "{source:?}"
                );
            }
        }
    }

    #[test]
    fn mapped_colon_paragraph_keeps_authored_columns() {
        let source = MappedSource::new_line_at(
            "::: note\nalpha beta\ngamma?\n:::\n".to_owned(),
            Some(17),
            Some(5),
        );
        let options = Options::default()
            .with_positions(true)
            .with_source_lines(true);
        STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
        let (actual, diagnostics) =
            collecting_lint_events(|| parse_mapped_source(&source, &options));
        assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0);
        let (expected, standard_diagnostics) = {
            let _standard = StandardEventRootGuard::install();
            collecting_lint_events(|| parse_mapped_source(&source, &options))
        };
        assert_eq!((actual, diagnostics), (expected, standard_diagnostics));
    }

    #[test]
    fn long_plain_colon_paragraph_stays_event_owned() {
        let source = format!("::: note\n{}\n:::\n", "alpha beta.\n".repeat(40));
        for (positions, source_lines) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let event_observer = Observer(RefCell::new(Vec::new()));
            let options = Options::default()
                .with_positions(positions)
                .with_source_lines(source_lines)
                .with_extension(&event_observer);
            STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
            let ((actual, admitted), diagnostics) =
                collecting_lint_events(|| parse_document_with_options(&source, &options).unwrap());
            assert!(admitted);
            assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0);
            let callbacks = event_observer.0.into_inner();
            assert_eq!(callbacks.len(), 1);

            let standard_observer = Observer(RefCell::new(Vec::new()));
            let options = Options::default()
                .with_positions(positions)
                .with_source_lines(source_lines)
                .with_extension(&standard_observer);
            let (expected, standard_diagnostics) = {
                let _standard = StandardEventRootGuard::install();
                collecting_lint_events(|| parse_with_options(&source, &options))
            };
            assert_eq!((actual, diagnostics), (expected, standard_diagnostics));
            assert_eq!(callbacks, standard_observer.0.into_inner());
        }
    }

    #[test]
    fn long_complex_colon_paragraphs_have_event_ownership() {
        for body in [
            "alpha *bold*\nbeta [link](/u)\ngamma `code`\ndelta _em_\nepsilon \\*literal\\*",
            "alpha *bold*\nbeta [link](/u)\ngamma `code`\ndelta _em_\nepsilon /path",
        ] {
            for positions in [false, true] {
                for source_lines in [false, true] {
                    let source = format!("::: note\n{body}\n:::\n");
                    let options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines);
                    STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                    let ((actual, admitted), diagnostics) = collecting_lint_events(|| {
                        parse_document_with_options(&source, &options).unwrap()
                    });
                    assert!(admitted);
                    assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0, "{source:?}");
                    let (expected, expected_diagnostics) = {
                        let _standard = StandardEventRootGuard::install();
                        collecting_lint_events(|| parse_with_options(&source, &options))
                    };
                    assert_eq!(
                        (actual, diagnostics),
                        (expected, expected_diagnostics),
                        "{source:?}"
                    );
                    let event_observer = Observer(RefCell::new(Vec::new()));
                    let event_options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&event_observer);
                    let (actual, admitted) =
                        parse_document_with_options(&source, &event_options).unwrap();
                    assert!(admitted);
                    let standard_observer = Observer(RefCell::new(Vec::new()));
                    let standard_options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&standard_observer);
                    let expected = {
                        let _standard = StandardEventRootGuard::install();
                        parse_with_options(&source, &standard_options)
                    };
                    assert_eq!(actual, expected, "{source:?}");
                    assert_eq!(
                        event_observer.0.into_inner(),
                        standard_observer.0.into_inner(),
                        "{source:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn deep_long_complex_colon_paragraph_uses_default_stack() {
        let mut source = "::: note\n".repeat(150);
        source.push_str("alpha *bold*\nbeta [link](/u)\ngamma `code`\ndelta _em_\nepsilon /path\n");
        source.push_str(&":::\n".repeat(150));
        let options = Options::default()
            .with_positions(true)
            .with_source_lines(true);
        STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
        let (actual, admitted) = parse_document_with_options(&source, &options).unwrap();
        assert!(admitted);
        assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0);
        let expected = {
            let _standard = StandardEventRootGuard::install();
            parse_with_options(&source, &options)
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn colon_paragraph_preflight_refuses_attributes_and_images() {
        for source in [
            "::: note\nalpha{#id}\nbeta\n:::\n",
            "::: note\nalpha ![alt](image.png){#id}\n:::\n",
        ] {
            for (positions, source_lines) in [(false, false), (true, true)] {
                let observer = Observer(RefCell::new(Vec::new()));
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&observer);
                STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                let ((actual, admitted), diagnostics) = collecting_lint_events(|| {
                    parse_document_with_options(source, &options).unwrap()
                });
                assert!(admitted, "{source:?}");
                assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 1, "{source:?}");
                let callbacks = observer.0.into_inner();
                assert_eq!(callbacks.len(), 1, "{source:?}: {callbacks:?}");
                let observer = Observer(RefCell::new(Vec::new()));
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&observer);
                let (expected, standard_diagnostics) = {
                    let _standard = StandardEventRootGuard::install();
                    collecting_lint_events(|| parse_with_options(source, &options))
                };
                assert_eq!(
                    (actual, diagnostics),
                    (expected, standard_diagnostics),
                    "{source:?}"
                );
                assert_eq!(callbacks, observer.0.into_inner(), "{source:?}");
            }
        }
    }

    #[test]
    fn standalone_attrs_after_colon_paragraph_match_standard_reader() {
        for source in [
            "::: outer\n::: inner\nalpha\n{#id}\n# sibling\n:::\n:::\n",
            "::: outer\n::: inner\nalpha\n{.class}\n# sibling\n:::\n:::\n",
            "::: outer\n::: inner\nalpha\n{#id}{.class}\n# sibling\n:::\n:::\n",
            "::: outer\n::: inner\nalpha\n{#id}\n{.class}\n# sibling\n:::\n:::\n",
            "::: outer\n::: inner\nalpha\n\n{.class}\n# sibling\n:::\n:::\n",
        ] {
            for (positions, source_lines) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let observer = Observer(RefCell::new(Vec::new()));
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&observer);
                STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                let ((actual, admitted), diagnostics) = collecting_lint_events(|| {
                    parse_document_with_options(source, &options).unwrap()
                });
                assert!(admitted, "{source:?}");
                assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0, "{source:?}");
                let callbacks = observer.0.into_inner();
                let observer = Observer(RefCell::new(Vec::new()));
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&observer);
                let (expected, standard_diagnostics) = {
                    let _standard = StandardEventRootGuard::install();
                    collecting_lint_events(|| parse_with_options(source, &options))
                };
                assert_eq!(
                    (actual, diagnostics),
                    (expected, standard_diagnostics),
                    "{source:?}"
                );
                assert_eq!(callbacks, observer.0.into_inner(), "{source:?}");
            }
        }
    }

    #[test]
    fn attached_id_colon_paragraph_matches_standard_reader() {
        for source in [
            "::: note\nalpha{#id}\n:::\n",
            "::: note\nalpha beta{#two-2}\n:::\n",
            "::: note\nalpha{#id}\n\n:::\n",
        ] {
            for (positions, source_lines) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let observer = Observer(RefCell::new(Vec::new()));
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&observer);
                STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                let ((actual, admitted), diagnostics) = collecting_lint_events(|| {
                    parse_document_with_options(source, &options).unwrap()
                });
                assert!(admitted, "{source:?}");
                assert_eq!(STANDARD_PARAGRAPH_ENTRIES.with(Cell::get), 0, "{source:?}");
                let callbacks = observer.0.into_inner();
                let standard_observer = Observer(RefCell::new(Vec::new()));
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines)
                    .with_extension(&standard_observer);
                let (expected, standard_diagnostics) = {
                    let _standard = StandardEventRootGuard::install();
                    collecting_lint_events(|| parse_with_options(source, &options))
                };
                assert_eq!((actual, diagnostics), (expected, standard_diagnostics));
                assert_eq!(callbacks, standard_observer.0.into_inner());
            }
        }
    }
}

#[cfg(test)]
mod mixed_definition_owner_tests {
    use super::*;

    #[test]
    fn keeps_following_root_blocks_and_declines_continuations() {
        for (source, start) in [
            ("# heading\n\n:: term\n:  definition\n\n# tail\n", 2),
            (":: a\n:  alpha\n:: b\n:  beta\n\nroot\n", 0),
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_simple_definition_list_prefix(&lines, start, &Options::default(), false)
                    .is_some(),
                "{source:?}"
            );
            let (candidate, used) = parse_document(source).expect("event document");
            assert!(used, "{source:?}");
            assert_eq!(candidate, parse_with_options(source, &Options::default()));
        }
        for source in [
            ":: term\n:  definition\n a\n",
            ":: t\n:  head\n\n     %%%\n     a\n",
            ":: term\n:  definition\nroot\n",
            ":: a\n:  x\n\n:: b\n:  y\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_simple_definition_list_prefix(&lines, 0, &Options::default(), false)
                    .is_none(),
                "{source:?}"
            );
        }
    }

    #[test]
    fn repeated_mixed_definitions_match_full_document() {
        let source = (0..300)
            .map(|index| format!(":: term{index}\n:  definition{index}\n\nroot{index}\n\n"))
            .collect::<String>();
        let lines: Vec<&str> = source.lines().collect();
        let direct = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with(":: "))
            .filter(|(at, _)| {
                parse_simple_definition_list_prefix(&lines, *at, &Options::default(), false)
                    .is_some()
            })
            .count();
        assert_eq!(direct, 300);
        let (candidate, used) = parse_document(&source).expect("event document");
        assert!(used);
        assert_eq!(candidate, parse_with_options(&source, &Options::default()));
    }
}

#[cfg(test)]
mod root_fence_owner_tests {
    use super::*;

    #[test]
    fn closed_root_code_fences_match_full_document() {
        for source in [
            "```\ncode\n```\n",
            "```rust\ncode\n```\n",
            "~~~~\n\n~~~~\n",
            "```\n```\n",
            "```\ncode\n```\n\n# next\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let open = detect_fence_open(lines[0]).unwrap();
            let (block, end) = parse_direct_root_code_fence(&lines, 0, open, &Options::default())
                .unwrap_or_else(|| panic!("{source:?}"));
            assert!(matches!(block, BlockNode::CodeBlock(_)));
            assert!(end >= 2);
            let (candidate, used) = parse_document(source).unwrap();
            assert!(used);
            assert_eq!(candidate, parse_with_options(source, &Options::default()));
        }
    }

    #[test]
    fn raw_caption_and_unclosed_fences_decline_direct_owner() {
        for source in [
            "```=html\n<b>x</b>\n```\n",
            "```\nx\n```\n^ caption\n",
            "```\nx\n```\n\n^ caption\n",
            "```\nx\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let open = detect_fence_open(lines[0]).unwrap();
            assert!(
                parse_direct_root_code_fence(&lines, 0, open, &Options::default()).is_none(),
                "{source:?}"
            );
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(candidate, parse_with_options(source, &Options::default()));
            }
        }
    }

    #[test]
    fn root_fence_matrix_matches_full_document() {
        let mut direct = 0;
        for marker in ['`', '~'] {
            for count in [3, 4] {
                let fence = marker.to_string().repeat(count);
                for info in ["", " rust", " \"title\""] {
                    for payload in ["", "x\n", "x\n\n", "```\ninner\n"] {
                        for tail in ["", "\n", "# after\n", "^ caption\n"] {
                            let source = format!("{fence}{info}\n{payload}{fence}\n{tail}");
                            let lines: Vec<&str> = source.lines().collect();
                            let Some(open) = detect_fence_open(lines[0]) else {
                                continue;
                            };
                            direct += usize::from(
                                parse_direct_root_code_fence(&lines, 0, open, &Options::default())
                                    .is_some(),
                            );
                            if let Some((candidate, _)) = parse_document(&source) {
                                assert_eq!(
                                    candidate,
                                    parse_with_options(&source, &Options::default()),
                                    "{source:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(direct > 0);
    }

    #[test]
    fn repeated_root_code_fences_match_full_document() {
        let source = (0..300)
            .map(|index| format!("```rust\nlet item_{index} = {index};\n```\n\n"))
            .collect::<String>();
        let (candidate, used) = parse_document(&source).unwrap();
        assert!(used);
        assert_eq!(candidate, parse_with_options(&source, &Options::default()));
    }

    #[test]
    fn fenced_code_inside_quotes_and_containers_matches_full_document() {
        for source in [
            "> ```rust\n> code\n> ```\n",
            "::: note\n```rust\ncode\n```\n:::\n",
            "- ```rust\n  code\n  ```\n",
        ] {
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(candidate, parse_with_options(source, &Options::default()));
            }
        }
    }

    #[test]
    fn direct_fence_waits_for_position_and_source_line_support() {
        let lines = ["```", "code", "```"];
        let open = detect_fence_open(lines[0]).unwrap();
        let positioned = Options {
            positions: true,
            ..Options::default()
        };
        assert!(parse_direct_root_code_fence(&lines, 0, open, &positioned).is_none());
        let source_lines = Options {
            source_lines: true,
            ..Options::default()
        };
        assert!(parse_direct_root_code_fence(&lines, 0, open, &source_lines).is_none());
    }
}

#[cfg(test)]
mod continuation_owner_tests {
    use super::*;

    #[test]
    fn non_marker_plus_keeps_direct_document_parity() {
        for source in [
            "plain a+b\n",
            "plain a + b\n",
            "code `+` inline\n",
            "- item + detail\n",
        ] {
            let _guard = continuation_marker::Guard::install(source);
            assert!(!continuation_marker::active(), "{source:?}");
            let (candidate, used) = parse_document(source).unwrap();
            assert!(used, "{source:?}");
            assert_eq!(candidate, parse_with_options(source, &Options::default()));
        }
    }

    #[test]
    fn owned_list_continuations_match_full_ast() {
        for source in ["- +\nx\n", "- a\n+\nb\n", "1. - +\npara\n"] {
            let _guard = continuation_marker::Guard::install(source);
            let lines: Vec<&str> = source.lines().collect();
            let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
            let (_, consumed) =
                parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map))
                    .unwrap_or_else(|| panic!("{source:?}"));
            assert_eq!(consumed, lines.len(), "{source:?}");
            let (candidate, used) = parse_document(source).unwrap();
            assert!(used, "{source:?}");
            assert_eq!(
                candidate,
                parse_with_options(source, &Options::default()),
                "{source:?}"
            );
        }
    }

    #[test]
    fn refused_list_continuations_keep_the_standard_ast() {
        for source in [
            "- +\n  x\n",
            "- a\n  +\n  b\n",
            "- a\n  +\nb\n",
            "- a\n  > q\n  +\n  para\n",
            "- +\n\n",
        ] {
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(
                    candidate,
                    parse_with_options(source, &Options::default()),
                    "{source:?}"
                );
            }
        }
    }

    #[test]
    fn list_continuation_owner_matrix_matches_full_document() {
        let mut direct = 0;
        let mut checked = 0;
        for marker in ["-", "1."] {
            for nested in ["", "- ", "1. "] {
                for suffix in ["", " ", "\t"] {
                    for follower in ["x", "# h", "> q", "- x", "```\ncode\n```", "***", "___"] {
                        for indent in 0..=4 {
                            let prefix = " ".repeat(indent);
                            let follower = follower
                                .lines()
                                .map(|line| format!("{prefix}{line}\n"))
                                .collect::<String>();
                            let source = format!("{marker} {nested}+{suffix}\n{follower}");
                            let _guard = continuation_marker::Guard::install(&source);
                            let lines: Vec<&str> = source.lines().collect();
                            let line_map: Vec<Option<usize>> =
                                (1..=lines.len()).map(Some).collect();
                            direct += usize::from(
                                parse_unified_list_mapped(
                                    &lines,
                                    0,
                                    &Options::default(),
                                    Some(&line_map),
                                )
                                .is_some(),
                            );
                            if let Some((candidate, _)) = parse_document(&source) {
                                assert_eq!(
                                    candidate,
                                    parse_with_options(&source, &Options::default()),
                                    "{source:?}"
                                );
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 630);
        assert!(direct > 0);
    }

    #[test]
    fn standalone_list_continuation_matrix_matches_full_document() {
        for marker in ["-", "1."] {
            for plus_indent in 0..=4 {
                for follower_indent in 0..=4 {
                    for follower in ["x", "# h", "> q", "- x", "```\ncode\n```", "***", "___"] {
                        let prefix = " ".repeat(follower_indent);
                        let follower = follower
                            .lines()
                            .map(|line| format!("{prefix}{line}\n"))
                            .collect::<String>();
                        let source =
                            format!("{marker} a\n{}+\n{follower}", " ".repeat(plus_indent));
                        if let Some((candidate, _)) = parse_document(&source) {
                            assert_eq!(
                                candidate,
                                parse_with_options(&source, &Options::default()),
                                "{source:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn repeated_owned_list_continuations_match_full_document() {
        let source = (0..300)
            .map(|index| format!("- item {index}\n+\ncontinuation {index}\n\n"))
            .collect::<String>();
        let _guard = continuation_marker::Guard::install(&source);
        let lines: Vec<&str> = source.lines().collect();
        let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
        let (_, consumed) =
            parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map))
                .expect("shared owner accepts the repeated list");
        assert!(lines[consumed..].iter().all(|line| is_blank_line(line)));
        let (candidate, used) = parse_document(&source).unwrap();
        assert!(used);
        assert_eq!(candidate, parse_with_options(&source, &Options::default()));
    }

    #[test]
    fn owned_list_continuation_marker_events_match_standard() {
        for source in ["- +\nx\n", "- a\n+\nb\n", "1. - +\npara\n"] {
            let (_, standard_events) =
                collecting_lint_events(|| parse_with_options(source, &Options::default()));
            let (_, event_events) = collecting_lint_events(|| parse_document(source));
            let markers = |events: Vec<LintEvent>| {
                events
                    .into_iter()
                    .filter_map(|event| match event {
                        LintEvent::Marker {
                            line,
                            column,
                            host,
                            reason,
                            first_block,
                        } => Some((line, column, host, reason, first_block)),
                        LintEvent::Fence { .. } => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                markers(event_events),
                markers(standard_events),
                "{source:?}"
            );
        }
    }

    #[test]
    fn continuation_followed_by_indented_content_declines_direct_owner() {
        for source in [
            "- +\nx\n  - c\n",
            "- +\nx\n  > q\n",
            "- +\nx\n  # h\n",
            "- +\nx\n  ```\n  ```\n",
            "- - +\nx\n\n    x\n",
            "- a\n+\nb\n  - c\n",
        ] {
            let _guard = continuation_marker::Guard::install(source);
            let lines: Vec<&str> = source.lines().collect();
            let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
            assert!(
                parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map))
                    .is_none(),
                "{source:?}"
            );
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(candidate, parse_with_options(source, &Options::default()));
            }
        }
    }
}

pub(super) enum Event<'a> {
    Heading(usize, u8, &'a str),
    Paragraph(usize, &'a [&'a str]),
}

pub(super) struct Events<'a> {
    pub(super) lines: &'a [&'a str],
    pub(super) next: usize,
}

impl<'a> Iterator for Events<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.lines.len() && is_blank_line(self.lines[self.next]) {
            self.next += 1;
        }
        let first = *self.lines.get(self.next)?;
        if let Some((level, text)) = detect_heading(first) {
            let start = self.next;
            self.next += 1;
            return Some(Event::Heading(start, level, text));
        }
        let start = self.next;
        self.next += 1;
        while self.next < self.lines.len() {
            let line = self.lines[self.next];
            if is_blank_line(line) || detect_heading(line).is_some() {
                break;
            }
            self.next += 1;
        }
        Some(Event::Paragraph(start, &self.lines[start..self.next]))
    }
}

fn supported_line(line: &str) -> bool {
    if is_blank_line(line) {
        return true;
    }
    if line.starts_with('#') {
        return detect_heading(line).is_some();
    }
    line.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && !is_list_marker(line)
}

/// Materialize the public nodes from borrowed line events. A complete lexical
/// gate runs first, so an unsupported line never leaves a partly built tree.
pub(super) fn parse_blocks(lines: &[&str], options: &Options<'_>) -> Option<Vec<BlockNode>> {
    parse_blocks_mapped(lines, options, None)
}

fn parse_blocks_mapped(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
) -> Option<Vec<BlockNode>> {
    if options.positions
        || (options.source_lines && line_map.is_none())
        || !options.extensions.is_empty()
    {
        return None;
    }
    if !lines.iter().copied().all(supported_line) {
        return None;
    }
    let mut out = Vec::new();
    for event in (Events { lines, next: 0 }) {
        match event {
            Event::Heading(start, level, text) => {
                let mut node = BlockNode::Heading(Heading {
                    attrs: None,
                    level,
                    children: parse_event_inlines(trim_ascii_end(text), options),
                    pos: None,
                });
                if options.source_lines {
                    stamp_source_line(&mut node, line_map?.get(start).copied().flatten()?);
                }
                out.push(node);
            }
            Event::Paragraph(start, lines) => {
                let joined = lines
                    .iter()
                    .map(|line| trim_ascii_end(trim_ascii_start(line)))
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut node = BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: parse_event_inlines(&joined, options),
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                });
                if options.source_lines {
                    stamp_source_line(&mut node, line_map?.get(start).copied().flatten()?);
                }
                out.push(node);
            }
        }
    }
    Some(out)
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_positioned_event_paragraph(
    cur: &mut LineCursor<'_>,
    lines: &[&str],
    options: &Options<'_>,
) -> BlockNode {
    let start = cur.pos;
    let trimmed: Vec<&str> = lines
        .iter()
        .map(|line| trim_ascii_end(trim_ascii_start(line)))
        .collect();
    let joined = trimmed.join("\n");
    let anchors = trimmed
        .iter()
        .enumerate()
        .map(|(offset, line)| inline_anchor_for_line(cur, start + offset, line))
        .collect();
    cur.pos += lines.len();
    BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: parse_inline_lines_with_anchor(&joined, options, anchors),
        at_content_column: true,
        block_image: false,
        pos: span_of(cur, start, cur.pos, options),
    })
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_positioned_simple_blocks(
    lines: &[&str],
    options: &Options<'_>,
    line_map: &[Option<usize>],
) -> Option<Vec<BlockNode>> {
    if !options.positions
        || !options.extensions.is_empty()
        || !lines.iter().copied().all(supported_line)
    {
        return None;
    }
    let col_map = vec![Some(0); lines.len()];
    let mut cur = LineCursor::new_with_cols(lines, Some(line_map), Some(&col_map));
    cur.at_document_level = true;
    let mut out = Vec::new();
    for event in (Events { lines, next: 0 }) {
        let (start, end, mut node) = match event {
            Event::Heading(start, level, text) => {
                cur.pos = start;
                let node = *parse_heading_block(&mut cur, level, text, options);
                (start, start + 1, node)
            }
            Event::Paragraph(start, paragraph_lines) => {
                cur.pos = start;
                let node = parse_positioned_event_paragraph(&mut cur, paragraph_lines, options);
                (start, start + paragraph_lines.len(), node)
            }
        };
        if cur.pos != end {
            return None;
        }
        if options.source_lines {
            stamp_source_line(&mut node, line_map.get(start).copied().flatten()?);
        }
        out.push(node);
    }
    Some(out)
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
mod positioned_simple_paragraph_tests {
    use super::*;

    #[test]
    fn event_paragraph_matches_standard_blocks_and_diagnostics() {
        for source in [
            "alpha",
            "alpha\n",
            "alpha\nbeta",
            "alpha\nbeta\n",
            "alpha\n\nbeta\n",
            "# head\nalpha\nbeta\n## next\nend",
            "alpha *emphasis* and `code`\nbeta [link](url)\n",
            "alpha [broken](\nbeta\n",
            "alpha\n# heading\nlast\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
            for source_lines in [false, true] {
                let options = Options::default()
                    .with_positions(true)
                    .with_source_lines(source_lines);
                let actual = collecting_lint_events(|| {
                    parse_positioned_simple_blocks(&lines, &options, &line_map)
                        .expect("simple source admitted")
                });
                let expected = collecting_lint_events(|| {
                    let cols = vec![Some(0); lines.len()];
                    let mut cur = LineCursor::new_with_cols(&lines, Some(&line_map), Some(&cols));
                    cur.at_document_level = true;
                    let mut blocks = Vec::new();
                    for event in (Events {
                        lines: &lines,
                        next: 0,
                    }) {
                        let (start, mut node) = match event {
                            Event::Heading(start, level, text) => {
                                cur.pos = start;
                                (start, *parse_heading_block(&mut cur, level, text, &options))
                            }
                            Event::Paragraph(start, _) => {
                                cur.pos = start;
                                (start, parse_paragraph(&mut cur, &options))
                            }
                        };
                        if source_lines {
                            stamp_source_line(&mut node, line_map[start].unwrap());
                        }
                        blocks.push(node);
                    }
                    blocks
                });
                assert_eq!(
                    actual, expected,
                    "source={source:?} source_lines={source_lines}"
                );
            }
        }
    }
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
mod positioned_mixed_paragraph_tests {
    use super::*;
    use crate::{BlockMatch, CarveExtension, MatcherContext};

    struct Observer(RefCell<Vec<usize>>);

    impl CarveExtension for Observer {
        fn name(&self) -> &'static str {
            "positioned-paragraph-observer"
        }

        fn match_block(
            &self,
            _: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            self.0.borrow_mut().push(start);
            None
        }
    }

    #[test]
    fn mixed_root_paragraphs_keep_positions_and_source_lines() {
        for (name, source) in [
            (
                "mixed",
                "{.lead}\nAlpha *emphasis*\nBeta [link](url)\n\n# Tail\n",
            ),
            (
                "colon",
                "::: note\nAlpha *emphasis*\nBeta [link](url)\n:::\n",
            ),
            (
                "definition",
                ":: term\n:  Alpha *emphasis*\n   Beta [link](url)\n",
            ),
        ] {
            for positions in [false, true] {
                for source_lines in [false, true] {
                    let event_observer = Observer(RefCell::new(Vec::new()));
                    let standard_observer = Observer(RefCell::new(Vec::new()));
                    let options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&event_observer);
                    STANDARD_PARAGRAPH_ENTRIES.with(|count| count.set(0));
                    let (actual, diagnostics) = collecting_lint_events(|| {
                        parse_document_with_options(source, &options).unwrap().0
                    });
                    let entries = STANDARD_PARAGRAPH_ENTRIES.with(Cell::get);
                    let (expected, expected_diagnostics) = {
                        let _standard = StandardEventRootGuard::install();
                        let standard_options = Options::default()
                            .with_positions(positions)
                            .with_source_lines(source_lines)
                            .with_extension(&standard_observer);
                        collecting_lint_events(|| parse_with_options(source, &standard_options))
                    };
                    assert_eq!(
                        actual, expected,
                        "{source:?}, positions={positions}, source_lines={source_lines}"
                    );
                    assert_eq!(diagnostics, expected_diagnostics);
                    assert_eq!(
                        event_observer.0.into_inner(),
                        standard_observer.0.into_inner()
                    );
                    if name != "definition" {
                        assert_eq!(
                            entries, 0,
                            "{source:?}, positions={positions}, source_lines={source_lines}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
mod event_level_resume_tests {
    use super::*;
    use crate::{BlockMatch, CarveExtension, MatcherContext};

    struct Observer(RefCell<Vec<(usize, String)>>);

    impl CarveExtension for Observer {
        fn name(&self) -> &'static str {
            "resume-observer"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            self.0.borrow_mut().push((start, lines[start].to_owned()));
            None
        }
    }

    #[test]
    fn resumed_level_matches_uninterrupted_blocks_and_callbacks() {
        let source = "{#first}\n# Heading\n\n::: note\nbody\n:::\n\n{.tail}\n## Tail\nparagraph\n";
        let lines: Vec<&str> = source.lines().collect();
        let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
        let cols = vec![Some(0); lines.len()];
        let boundary = 7;

        for positions in [false, true] {
            for source_lines in [false, true] {
                let run = |split| {
                    let observer = Observer(RefCell::new(Vec::new()));
                    let options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&observer);
                    let mut cur = LineCursor::new_with_cols(&lines, Some(&line_map), Some(&cols));
                    cur.at_document_level = true;
                    let mut state = EventLevelState::new(&cur, false, true);
                    let (blocks, events) = collecting_lint_events(|| {
                        if split {
                            assert!(matches!(
                                run_event_level::<true, false>(
                                    &lines,
                                    &options,
                                    Some(&line_map),
                                    &mut cur,
                                    &mut state,
                                    None,
                                    None,
                                    true,
                                    Some(boundary),
                                ),
                                Some(EventLevelRun::Yield)
                            ));
                            assert_eq!(cur.pos, boundary);
                        }
                        match run_event_level::<true, false>(
                            &lines,
                            &options,
                            Some(&line_map),
                            &mut cur,
                            &mut state,
                            None,
                            None,
                            true,
                            None,
                        ) {
                            Some(EventLevelRun::Done(blocks)) => blocks,
                            _ => panic!("event level did not finish"),
                        }
                    });
                    (blocks, events, observer.0.into_inner())
                };
                assert_eq!(
                    run(true),
                    run(false),
                    "positions={positions} source_lines={source_lines}"
                );
            }
        }
    }

    #[test]
    fn observed_definition_yields_before_later_description() {
        let source = ":: term\n:  first\n:  second\n\n# Later\n";
        let lines: Vec<&str> = source.lines().collect();
        let run = |manual: bool| {
            let observer = Observer(RefCell::new(Vec::new()));
            let options = Options::default().with_extension(&observer);
            let mut cur = LineCursor::new_with_cols(&lines, None, None);
            cur.at_document_level = true;
            let mut pending = Vec::new();
            let mut blocks = if manual {
                let mut state = EventLevelState::new(&cur, true, false);
                let Some(EventLevelRun::NeedDefinitionBody(mut continuation)) =
                    run_event_level::<true, false>(
                        &lines,
                        &options,
                        None,
                        &mut cur,
                        &mut state,
                        None,
                        Some(&mut pending),
                        true,
                        None,
                    )
                else {
                    panic!("expected definition body yield");
                };
                cur.restore_state(continuation.cursor);
                assert_eq!(cur.pos, 2);
                assert!(observer
                    .0
                    .borrow()
                    .iter()
                    .all(|(_, line)| !line.contains("second") && !line.contains("Later")));
                state = continuation.state;
                let children = parse_definition_description(
                    continuation.request,
                    &cur,
                    &options,
                    &mut None,
                    false,
                );
                continuation.scan.fill_body(children, &cur, &options);
                loop {
                    match continuation.scan.step(&mut cur, &options) {
                        StepResult::Continue => {}
                        StepResult::NeedBody(request) => {
                            let children = parse_definition_description(
                                request, &cur, &options, &mut None, false,
                            );
                            continuation.scan.fill_body(children, &cur, &options);
                        }
                        StepResult::Done => break,
                    }
                }
                state.out.push(continuation.scan.finish(&cur, &options));
                match run_event_level::<true, false>(
                    &lines,
                    &options,
                    None,
                    &mut cur,
                    &mut state,
                    None,
                    Some(&mut pending),
                    true,
                    None,
                ) {
                    Some(EventLevelRun::Done(blocks)) => blocks,
                    _ => panic!("resumed level did not finish"),
                }
            } else {
                crate::parse::block::parse_blocks(&mut cur, &options, &mut pending)
            };
            resolve_pending_bodies(&mut blocks, pending, &options);
            (blocks, observer.0.into_inner())
        };
        assert_eq!(run(true), run(false));
    }

    #[test]
    fn mapped_task_requests_body_before_next_description_callback() {
        let source = ":: term\n:  first\n:  second\n\n# Later\n";
        let run = |task: bool| {
            let observer = Observer(RefCell::new(Vec::new()));
            let options = Options::default().with_extension(&observer);
            let blocks = if task {
                let mut task = MappedEventTask::new(
                    MappedSource::new_line_at(source.to_owned(), None, None),
                    &options,
                )
                .expect("task depth");
                let first = task.poll(&options);
                assert!(observer
                    .0
                    .borrow()
                    .iter()
                    .all(|(_, line)| { !line.contains("second") && !line.contains("Later") }));
                let mut result = first;
                loop {
                    match result {
                        TaskPoll::NeedBody(body) => {
                            let children = parse_mapped_source(&body, &options);
                            result = task.resume_with_children(children, &options);
                        }
                        TaskPoll::Done(blocks) => break blocks,
                    }
                }
            } else {
                let lines: Vec<_> = source.lines().collect();
                let mut cursor = LineCursor::new_with_cols(&lines, None, None);
                let mut pending = Vec::new();
                let mut blocks =
                    crate::parse::block::parse_blocks(&mut cursor, &options, &mut pending);
                resolve_pending_bodies(&mut blocks, pending, &options);
                blocks
            };
            (format!("{blocks:?}"), observer.0.into_inner())
        };
        assert_eq!(run(true), run(false));
    }

    #[test]
    fn mapped_task_plain_body_finishes_without_request() {
        let source = "Plain paragraph\n\n# Heading\n";
        let options = Options::default();
        let mut task = MappedEventTask::new(
            MappedSource::new_line_at(source.to_owned(), None, None),
            &options,
        )
        .expect("task depth");
        let TaskPoll::Done(blocks) = task.poll(&options) else {
            panic!("plain body requested children");
        };
        assert_eq!(
            format!("{blocks:?}"),
            format!(
                "{:?}",
                parse_mapped_source(
                    &MappedSource::new_line_at(source.to_owned(), None, None),
                    &options,
                )
            )
        );
    }

    #[test]
    fn mapped_task_matches_blank_separated_nested_definition_body() {
        fn parse_task(source: MappedSource, options: &Options<'_>) -> Vec<BlockNode> {
            let mut task = MappedEventTask::new(source, options).expect("task depth");
            let mut result = task.poll(options);
            loop {
                result = match result {
                    TaskPoll::NeedBody(body) => {
                        let children = {
                            let _depth = DepthScope::set(task.depth);
                            let _group = FigureGroupScope::set(task.in_group);
                            parse_task(body, options)
                        };
                        task.resume_with_children(children, options)
                    }
                    TaskPoll::Done(blocks) => return blocks,
                };
            }
        }
        let source = ":: t1\n: d1\n\n  :: t2\n  : d2\n\n    :: t3\n    : d3\n";
        let observer = Observer(RefCell::new(Vec::new()));
        let options = Options::default()
            .with_positions(true)
            .with_source_lines(true)
            .with_extension(&observer);
        let parent = MappedSource::new_line_at(source.to_owned(), None, None);
        let lines: Vec<_> = parent.source.lines().collect();
        let mut cursor =
            LineCursor::new_with_cols(&lines, Some(&parent.line_map), Some(&parent.col_map));
        cursor.reached = Some(&parent.reached);
        let mut scan = DefinitionListScan::new(&cursor);
        let mapped = loop {
            match scan.step(&mut cursor, &options) {
                StepResult::NeedBody(request) => break request.body,
                StepResult::Continue => {}
                StepResult::Done => panic!("missing body"),
            }
        };
        let expected = {
            let _standard = crate::parse::StandardNestedReaderGuard::install();
            parse_mapped_source(&mapped, &options)
        };
        let expected_callbacks = observer.0.replace(Vec::new());
        let actual = parse_task(mapped, &options);
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        assert_eq!(
            observer.0.borrow().as_slice(),
            expected_callbacks.as_slice()
        );
    }

    #[test]
    fn mapped_task_matches_tailed_definition_chain() {
        fn parse_task(source: MappedSource, options: &Options<'_>) -> Vec<BlockNode> {
            let Some(mut task) = MappedEventTask::new(source.clone(), options) else {
                return parse_mapped_source(&source, options);
            };
            let mut result = task.poll(options);
            loop {
                result = match result {
                    TaskPoll::NeedBody(body) => {
                        let _depth = DepthScope::set(task.depth);
                        let _group = FigureGroupScope::set(task.in_group);
                        let children = parse_task(body, options);
                        task.resume_with_children(children, options)
                    }
                    TaskPoll::Done(blocks) => return blocks,
                };
            }
        }
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for depth in [2, 4, 8, 12, 64, 120, 199, 200, 205, 260] {
                    let mut source = String::new();
                    for level in 0..depth {
                        let lead = "  ".repeat(level);
                        source.push_str(&format!("{lead}:: t{level}\n{lead}: d{level}\n\n"));
                    }
                    for level in (0..depth).rev() {
                        source.push_str(&format!("{}after{level}\n\n", "  ".repeat(level)));
                    }
                    let root = MappedSource::new_line_at(source, None, None);
                    let observer = Observer(RefCell::new(Vec::new()));
                    let options = Options::default()
                        .with_positions(true)
                        .with_source_lines(true)
                        .with_extension(&observer);
                    let lines: Vec<_> = root.source.lines().collect();
                    let mut cursor = LineCursor::new_with_cols(
                        &lines,
                        Some(&root.line_map),
                        Some(&root.col_map),
                    );
                    cursor.reached = Some(&root.reached);
                    let mut scan = DefinitionListScan::new(&cursor);
                    let mapped = loop {
                        match scan.step(&mut cursor, &options) {
                            StepResult::NeedBody(request) => break request.body,
                            StepResult::Continue => {}
                            StepResult::Done => panic!("missing body"),
                        }
                    };
                    let expected = {
                        let _standard = crate::parse::StandardNestedReaderGuard::install();
                        parse_mapped_source(&mapped, &options)
                    };
                    let expected_callbacks = observer.0.replace(Vec::new());
                    let actual = parse_task(mapped.clone(), &options);
                    assert_eq!(
                        observer.0.borrow().as_slice(),
                        expected_callbacks.as_slice(),
                        "depth={depth}: callbacks"
                    );
                    assert_eq!(
                        format!("{actual:?}"),
                        format!("{expected:?}"),
                        "depth={depth}"
                    );
                    observer.0.borrow_mut().clear();
                    let public = parse_mapped_source(&mapped, &options);
                    assert_eq!(
                        format!("{public:?}"),
                        format!("{expected:?}"),
                        "depth={depth}: public AST"
                    );
                    assert_eq!(
                        observer.0.borrow().as_slice(),
                        expected_callbacks.as_slice(),
                        "depth={depth}: public callbacks"
                    );
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn mapped_task_restores_nested_context_and_positioned_state() {
        let source = ":: term\n:  first\n:  second\n\n# Later\n";
        let options = Options::default()
            .with_positions(true)
            .with_source_lines(true);
        let outer_depth = NESTING_DEPTH.with(Cell::get);
        let _depth = DepthScope::set(7);
        let _group = FigureGroupScope::set(true);
        let mut task = MappedEventTask::new(
            MappedSource::new_line_at(source.to_owned(), Some(12), Some(3)),
            &options,
        )
        .expect("task depth");
        assert_eq!(task.depth, 8);
        assert!(task.in_group);
        let expected = parse_mapped_source(
            &MappedSource::new_line_at(source.to_owned(), Some(12), Some(3)),
            &options,
        );
        drop(_group);
        drop(_depth);
        assert_eq!(NESTING_DEPTH.with(Cell::get), outer_depth);
        let mut result = task.poll(&options);
        loop {
            result = match result {
                TaskPoll::NeedBody(body) => {
                    let _depth = DepthScope::set(task.depth);
                    let _group = FigureGroupScope::set(task.in_group);
                    let children = parse_mapped_source(&body, &options);
                    task.resume_with_children(children, &options)
                }
                TaskPoll::Done(blocks) => {
                    assert_eq!(format!("{blocks:?}"), format!("{expected:?}"));
                    break;
                }
            };
            assert_eq!(NESTING_DEPTH.with(Cell::get), outer_depth);
            assert!(!IN_FIGURE_GROUP.with(Cell::get));
        }
    }
}

/// Stream simple top-level blocks as events while handing list and definition
/// bodies to the existing readers. A construct outside this subset declines the
/// whole route before it can publish a partial AST.
fn parse_mixed_blocks<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    at_document_level: bool,
) -> Option<Vec<BlockNode>> {
    parse_mixed_blocks_mapped::<DEFINITIONS, ABBREVIATIONS>(lines, options, None, at_document_level)
}

#[cfg(feature = "experimental-event-parser-options")]
#[inline(never)]
pub(super) fn parse_nested_mapped_blocks(
    source: &MappedSource,
    options: &Options<'_>,
    scope: ParseScope,
    extents: Option<&mut Vec<(usize, usize)>>,
    pending: &mut Vec<PendingBody>,
    positioned: bool,
    drain_follows: bool,
) -> Option<Vec<BlockNode>> {
    if !event_frame_admitted(scope.in_quoted_body) {
        return None;
    }
    let _frame = EventFrame::enter();
    let _depth = DepthGuard::enter()?;
    let lines: Vec<&str> = source.source.lines().collect();
    let mut cur = LineCursor::new_with_cols(
        &lines,
        Some(&source.line_map),
        source
            .col_map
            .iter()
            .any(Option::is_some)
            .then_some(source.col_map.as_slice()),
    );
    cur.at_document_level = scope.at_document_level;
    cur.in_item_body = scope.in_item_body;
    cur.in_quoted_body = scope.in_quoted_body || source.in_quoted_body;
    cur.reached = Some(source.reached.as_slice());
    cur.sublists_carry_authored_base = positioned && source.sublists_carry_authored_base;
    cur.marker_tail = source.marker_tail;
    cur.drain_follows = drain_follows;
    let mut observed = Vec::new();
    let record_extents = extents.is_some();
    let mut nested_pending = Vec::new();
    let blocks = parse_mixed_blocks_cursor::<true, false>(
        &lines,
        options,
        Some(&source.line_map),
        cur,
        record_extents.then_some(&mut observed),
        Some(&mut nested_pending),
        scope.in_quoted_body,
    )?;
    #[cfg(test)]
    if !options.extensions.is_empty() {
        NESTED_EXTENSION_EVENT_PARSES.with(|count| count.set(count.get() + 1));
    }
    if let Some(extents) = extents {
        extents.extend(observed);
    }
    pending.extend(nested_pending);
    Some(blocks)
}

/// The event counterpart of `parse_blocks_with_options` for a nested fragment
/// whose bodies must resolve before the caller resumes. Declines only before
/// the cursor reads a line, so no extension callback can run twice.
#[cfg(feature = "experimental-event-parser-options")]
#[inline(never)]
pub(super) fn parse_drained_fragment(
    source: &str,
    options: &Options<'_>,
) -> Option<Vec<BlockNode>> {
    if !event_frame_admitted(false) {
        return None;
    }
    let lines: Vec<&str> = source.lines().collect();
    if continuation_marker::active()
        && lines.iter().copied().any(|line| {
            let line = trim_ascii(line);
            line == "+" || line.ends_with(" +")
        })
    {
        return None;
    }
    let want_lines = options.source_lines
        || options.positions
        || needs_source_ending_map()
        || needs_quote_marker_line_map(source);
    let line_map: Vec<Option<usize>> = if want_lines {
        (1..=lines.len()).map(Some).collect()
    } else {
        Vec::new()
    };
    let line_map = want_lines.then_some(line_map.as_slice());
    let col_map = options.positions.then(|| vec![Some(0); lines.len()]);
    let mut pending = Vec::new();
    // The drain below runs on top of this frame, so it stays counted; each
    // ladder level under it is one more eager fragment.
    let _frame = EventFrame::enter();
    // The standard reader drains after its depth guard drops; each body
    // restores the depth it recorded when it was collected.
    let mut blocks = {
        let _depth = DepthGuard::enter()?;
        let mut cur = LineCursor::new_with_cols(&lines, line_map, col_map.as_deref());
        cur.reached = Some(&[]);
        parse_mixed_blocks_cursor::<true, false>(
            &lines,
            options,
            line_map,
            cur,
            None,
            Some(&mut pending),
            false,
        )?
    };
    resolve_pending_bodies(&mut blocks, pending, options);
    Some(blocks)
}

/// The event counterpart of `parse_blocks` for a colon container body. Reads
/// the shared view in place, with the cursor `parse_colon_view` would build,
/// and leaves nested bodies on the caller's worklist.
#[cfg(feature = "experimental-event-parser-options")]
#[inline(never)]
pub(super) fn parse_colon_body<'a>(
    view: &'a ColonView,
    lines: CursorLines<'a>,
    source: &'a [SourceLine],
    options: &Options<'_>,
    pending: &mut Vec<PendingBody>,
) -> Option<Vec<BlockNode>> {
    if !event_frame_admitted(false) {
        return None;
    }
    let _frame = EventFrame::enter();
    let _depth = DepthGuard::enter()?;
    let mut cur = if options.source_lines
        || options.positions
        || needs_source_ending_map()
        || continuation_marker::active()
    {
        let mut cur =
            LineCursor::from_lines(lines, Some(&[]), options.positions.then_some([].as_slice()));
        cur.colon_geometry = Some(source);
        cur
    } else {
        LineCursor::from_lines(lines, None, None)
    };
    cur.colon_view = Some(view);
    let blocks = parse_mixed_blocks_cursor::<true, false>(
        &[],
        options,
        None,
        cur,
        None,
        Some(pending),
        false,
    )
    .expect("a deferring event cursor never declines");
    #[cfg(test)]
    COLON_BODY_EVENT_PARSES.with(|count| count.set(count.get() + 1));
    Some(blocks)
}

fn event_paragraph_needs_reader(
    next: &str,
    has_lines: bool,
    invalid_brace_line: bool,
    root_indented_text: bool,
) -> bool {
    (has_lines
        && (is_definition_list_start(next)
            || detect_fence_open(next).is_some()
            || thematic_break_marker(next).is_some()))
        || (!may_continue_plain_paragraph(next)
            && !is_list_marker(next)
            && !(!has_lines && invalid_brace_line)
            && !(!has_lines && root_indented_text))
}

fn direct_definition_allowed(direct_owner: bool, options: &Options<'_>) -> bool {
    direct_owner && !options.source_lines && !options.positions && options.extensions.is_empty()
}

/// A marker-hosted colon body opens a quote or a definition entry below its
/// own column too, ahead of every arm that would claim the line.
#[inline(always)]
fn parse_marker_hosted_opener(
    cur: &mut LineCursor<'_>,
    options: &Options<'_>,
    line: &str,
    index: usize,
    pending: &mut Vec<PendingBody>,
    drains_here: bool,
) -> Option<BlockNode> {
    if !cur.colon_view.is_some_and(|view| view.marker_hosted) {
        return None;
    }
    let line = trim_ascii_start(line);
    // A colon cursor never drains on the spot, so observed bodies stay eager.
    if is_definition_list_start(line) {
        #[cfg(test)]
        MARKER_HOSTED_DEFINITION_LISTS.with(|count| count.set(count.get() + 1));
        return Some(parse_definition_list_deferring(
            cur,
            options,
            index,
            pending,
            drains_here,
        ));
    }
    if strip_blockquote_prefix(line).is_some() {
        return Some(parse_marker_hosted_quote(cur, options, index, pending));
    }
    None
}

/// The quote at `index` in a marker-hosted colon body. Unobserved, its body
/// waits on the worklist; observed, it parses here.
#[inline(never)]
fn parse_marker_hosted_quote(
    cur: &mut LineCursor<'_>,
    options: &Options<'_>,
    index: usize,
    pending: &mut Vec<PendingBody>,
) -> BlockNode {
    let (span_start, parts, _) = collect_blockquote_body(cur, options);
    if parse_order_observed(options) || !quote_body_deferrable(cur, options, &parts) {
        return *parse_collected_blockquote(cur, options, span_start, parts);
    }
    #[cfg(test)]
    MARKER_HOSTED_QUOTE_DEFERRALS.with(|count| count.set(count.get() + 1));
    let body = QuoteBodyPending {
        plus_anchor: nested_plus_quote_anchor(cur, options, span_start),
        parts,
        depth: NESTING_DEPTH.with(Cell::get),
        in_group: IN_FIGURE_GROUP.with(Cell::get),
    };
    let node = finish_blockquote_shell(cur, options, span_start, Vec::new(), true);
    pending.push(PendingBody {
        index,
        body: Body::Quote(body),
    });
    *node
}

#[inline]
fn quote_deferred_in_level(
    cur: &LineCursor<'_>,
    options: &Options<'_>,
    parts: &[LineBuffer],
    defer_quotes: bool,
) -> bool {
    (defer_quotes || (cur.colon_view.is_some() && !parse_order_observed(options)))
        && quote_body_deferrable(cur, options, parts)
}

#[cfg(test)]
fn count_colon_quote_deferral(cur: &LineCursor<'_>) {
    if cur.colon_view.is_some() {
        COLON_QUOTE_DEFERRALS.with(|count| count.set(count.get() + 1));
    }
}

/// The definition list at `index` in the level's output. Unobserved, its
/// description bodies wait on the worklist; observed, callbacks must see each
/// body before what follows it, so they parse here. The one exception is a
/// body with only blank lines after it in a level whose pending bodies are
/// resolved right after it (`drains_here`: a locally draining level or a
/// worklist description body, see `LineCursor::drain_follows`). Nothing observable
/// runs before that drain, and the body is the last entry, so it pops first,
/// where it would have run.
#[inline(never)]
fn parse_definition_list_deferring(
    cur: &mut LineCursor<'_>,
    options: &Options<'_>,
    index: usize,
    pending: &mut Vec<PendingBody>,
    drains_here: bool,
) -> BlockNode {
    let observed = parse_order_observed(options);
    if observed && !drains_here {
        return parse_definition_list(cur, options);
    }
    let depth = NESTING_DEPTH.with(Cell::get);
    let in_group = IN_FIGURE_GROUP.with(Cell::get);
    let mut defer = |item: usize, description: usize, source: MappedSource| {
        #[cfg(test)]
        if observed {
            OBSERVED_DEFINITION_DEFERRALS.with(|count| count.set(count.get() + 1));
        }
        pending.push(PendingBody {
            index,
            body: Body::Definition(DefinitionBodyPending {
                item,
                description,
                source,
                depth,
                in_group,
            }),
        });
    };
    parse_definition_list_with(cur, options, Some(&mut defer), observed)
}

fn direct_quote_allowed(direct_owner: bool, options: &Options<'_>, line: &str) -> bool {
    direct_owner
        && NESTING_DEPTH.with(Cell::get) < DIRECT_QUOTE_DEPTH_LIMIT
        && !options.source_lines
        && !options.positions
        && options.extensions.is_empty()
        && !needs_source_ending_map()
        && !line.contains("![")
}

/// May the worklist parse this collected quote body after the rest of the
/// level? Only when the hollow node answers every question asked before the
/// body is filled, and nothing observable moves across the deferral.
fn quote_body_deferrable(
    cur: &LineCursor<'_>,
    options: &Options<'_>,
    parts: &[LineBuffer],
) -> bool {
    // `finish_blockquote` asks whether the body is empty only to take a caption.
    let after_blank_reaches =
        cur.peek().is_some_and(is_blank_line) && cur.carried_reach(cur.pos + 1) == Some(true);
    let caption_blocked = cur.carried_reach(cur.pos) == Some(false) && !after_blank_reaches;
    let caption_follows = !caption_blocked && caption_slot_follows(cur);
    if parse_order_observed(options) {
        // The eager body would run before anything after the quote, so the
        // level must end in silence: no caption, one part, blank lines only.
        return !caption_follows
            && parts.len() == 1
            && (cur.pos..cur.lines.len()).all(|at| is_blank_line(cur.lines.at(at)));
    }
    !caption_follows || parts.iter().any(part_opens_with_block)
}

/// The lookahead of `consume_caption`, without consuming.
fn caption_slot_follows(cur: &LineCursor<'_>) -> bool {
    let mut at = cur.pos;
    if cur.lines.line(at).is_some_and(is_blank_line) {
        at += 1;
    }
    cur.lines
        .line(at)
        .is_some_and(|line| caption_content(line).is_some())
}

/// A part whose first line opens a quote, a paragraph or a list yields a block.
fn part_opens_with_block(part: &LineBuffer) -> bool {
    part.lines
        .iter()
        .map(|line| line.text.as_str())
        .find(|line| !is_blank_line(line))
        .and_then(|line| line.as_bytes().first())
        .is_some_and(|byte| *byte == b'>' || byte.is_ascii_alphanumeric())
}

fn use_standard_event_paragraph(recording_extents: bool, defer_pending: bool) -> bool {
    recording_extents || defer_pending
}

fn event_owned_plain_paragraph(cur: &LineCursor<'_>, options: &Options<'_>) -> bool {
    if (options.positions || options.source_lines) && cur.colon_view.is_none() {
        return false;
    }
    let mut count = 0;
    let mut bytes = 0;
    let mut complex = false;
    const COMPLEX_LINE_LIMIT: usize = 32;
    const COMPLEX_BYTE_LIMIT: usize = 4096;
    for line in cur.lines.range(cur.pos..cur.lines.len()) {
        if is_blank_line(line) {
            break;
        }
        if count != 0
            && line.len() <= 256
            && !line.starts_with([' ', '\t'])
            && parse_standalone_attrs(line).is_some()
        {
            break;
        }
        bytes += line.len();
        // An attached ID on a single, otherwise plain line is handled by the
        // inline parser. Keep the next line out of this fast path so a block
        // attribute or caption still gets the standard paragraph boundary.
        if count == 0
            && attached_plain_id(line)
            && cur.lines.line(cur.pos + 1).map_or(true, is_blank_line)
        {
            return true;
        }
        let plain = line.as_bytes()[0].is_ascii_alphabetic()
            && line.bytes().all(|byte| {
                byte.is_ascii_alphabetic()
                    || matches!(byte, b' ' | b'.' | b',' | b'!' | b'?' | b';' | b':')
            });
        if !plain {
            complex = true;
            if count >= COMPLEX_LINE_LIMIT
                || bytes > COMPLEX_BYTE_LIMIT
                || !line.chars().next().is_some_and(char::is_alphabetic)
                || !line.chars().all(|ch| {
                    ch.is_alphabetic()
                        || matches!(
                            ch,
                            ' ' | '.'
                                | ','
                                | '!'
                                | '?'
                                | ';'
                                | ':'
                                | '*'
                                | '_'
                                | '`'
                                | '['
                                | ']'
                                | '('
                                | ')'
                                | '/'
                                | '-'
                                | '\\'
                        )
                })
            {
                return false;
            }
        }
        if complex && (count >= COMPLEX_LINE_LIMIT || bytes > COMPLEX_BYTE_LIMIT) {
            return false;
        }
        count += 1;
    }
    count != 0
}

fn attached_plain_id(line: &str) -> bool {
    if line.len() > 256 {
        return false;
    }
    let Some((host, id)) = line
        .strip_suffix('}')
        .and_then(|line| line.split_once("{#"))
    else {
        return false;
    };
    !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b' ')
        && !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn refused_root_plus_at(cur: &LineCursor<'_>, line: &str) -> bool {
    trim_ascii(line) == "+"
        && cur
            .source_line(cur.pos)
            .and_then(continuation_marker::decision)
            .is_some_and(|decision| decision.refusal.is_some())
}

fn refused_marker_fence_list_plus(decision: continuation_marker::Decision) -> bool {
    // The marker belongs outside the list after this fence, even if its next line is blank.
    decision.owner == Some(continuation_marker::Kind::List)
        && matches!(
            decision.refusal,
            Some(continuation_marker::Refusal::Right | continuation_marker::Refusal::Blank)
        )
        && decision.sibling_interrupt
}

fn direct_root_list_read(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
    indented: bool,
) -> Option<UnifiedListRead> {
    if options.source_lines
        || options.positions
        || !options.extensions.is_empty()
        || needs_source_ending_map()
    {
        return None;
    }
    let blank_then_indented = lines.get(start + 1).is_some_and(|line| is_blank_line(line))
        && lines[start + 2..]
            .iter()
            .find(|line| !is_blank_line(line))
            .is_some_and(|line| line.starts_with([' ', '\t']));
    let read = if indented || blank_then_indented {
        parse_unified_list_mapped_with_exit(lines, start, options, line_map)
    } else {
        parse_simple_list(lines, start, options)
            .map(|(list, end)| UnifiedListRead {
                list,
                end,
                marker_fence_exit: false,
            })
            .or_else(|| parse_unified_list_mapped_with_exit(lines, start, options, line_map))
    };
    read.filter(|read| {
        read.marker_fence_exit
            || !lines[read.end..]
                .iter()
                .find(|line| !is_blank_line(line))
                .is_some_and(|line| trim_ascii(line) == "+")
    })
}

#[cfg(test)]
mod direct_list_boundary_tests {
    use super::*;

    #[test]
    fn later_fenced_plus_does_not_disable_a_direct_list() {
        let unrelated = ["- a", "", "```diff", "+", "```"];
        assert!(direct_root_list_read(&unrelated, 0, &Options::default(), None, false).is_some());

        let continuation = ["- a", "", "+", "p"];
        assert!(
            direct_root_list_read(&continuation, 0, &Options::default(), None, false).is_none()
        );

        let marker_fence = "- ```\n+\n  x\n";
        let _markers = continuation_marker::Guard::install(marker_fence);
        let lines: Vec<&str> = marker_fence.lines().collect();
        let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
        assert!(
            direct_root_list_read(&lines, 0, &Options::default(), Some(&line_map), false)
                .is_some_and(|read| read.marker_fence_exit && read.end == 1)
        );
    }
}

fn read_local_list_like_line(
    cur: &mut LineCursor<'_>,
    options: &Options<'_>,
) -> (BlockNode, Vec<ItemBody>) {
    let mut item_bodies = Vec::new();
    let node = *parse_block(cur, options, &mut item_bodies)
        .expect("a list-like line is not a consumed comment");
    (node, item_bodies)
}

fn has_pending_leading_attrs(
    leading_attrs: &[(usize, Attrs, Option<Pos>)],
    out: &[BlockNode],
) -> bool {
    leading_attrs.last().is_some_and(|(start, _, _)| {
        out[*start..]
            .iter()
            .all(|node| matches!(node, BlockNode::AbbreviationDef(_) | BlockNode::Comment(_)))
    })
}

fn finish_unattributed_block(
    block: &mut BlockNode,
    cur: &LineCursor<'_>,
    start_line: usize,
    options: &Options<'_>,
    has_leading_attrs: bool,
) {
    if !has_leading_attrs {
        promote_delimited_code(block, cur, start_line, options);
        resolve_code_title(block);
    }
}

fn is_late_list_like_line(
    cur: &LineCursor<'_>,
    line: &str,
    invalid_brace_line: bool,
    root_indented_text: bool,
) -> bool {
    !may_start_plain_paragraph(line)
        && !invalid_brace_line
        && !root_indented_text
        && (is_list_marker(trim_ascii_start(line))
            || trim_ascii(line) == "+" && !refused_root_plus_at(cur, line)
            || line.ends_with(" +"))
}

fn parse_mixed_blocks_mapped<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
    at_document_level: bool,
) -> Option<Vec<BlockNode>> {
    if line_map.is_none()
        && (!at_document_level || continuation_marker::active())
        && lines.iter().copied().any(|line| {
            let line = trim_ascii(line);
            line == "+" || line.ends_with(" +")
        })
    {
        return None;
    }
    let col_map = options.positions.then(|| vec![Some(0); lines.len()]);
    let mut cur = LineCursor::new_with_cols(lines, line_map, col_map.as_deref());
    cur.at_document_level = at_document_level;
    parse_mixed_blocks_cursor::<DEFINITIONS, ABBREVIATIONS>(
        lines, options, line_map, cur, None, None, true,
    )
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_mixed_blocks_with_root_columns<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    line_map: &[Option<usize>],
) -> Option<Vec<BlockNode>> {
    let col_map = vec![Some(0); lines.len()];
    let mut cur = LineCursor::new_with_cols(lines, Some(line_map), Some(&col_map));
    cur.at_document_level = true;
    parse_mixed_blocks_cursor::<DEFINITIONS, ABBREVIATIONS>(
        lines,
        options,
        Some(line_map),
        cur,
        None,
        None,
        true,
    )
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_custom_mixed_blocks<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
    root_columns: bool,
) -> Option<Vec<BlockNode>> {
    if root_columns {
        parse_mixed_blocks_with_root_columns::<DEFINITIONS, ABBREVIATIONS>(
            lines, options, line_map?,
        )
    } else {
        parse_mixed_blocks_mapped::<DEFINITIONS, ABBREVIATIONS>(lines, options, line_map, true)
    }
}

struct EventLevelState {
    out: Vec<BlockNode>,
    block_start_indices: Vec<usize>,
    pending_bodies: Vec<PendingBody>,
    #[cfg(feature = "experimental-event-parser-options")]
    source_lines: Vec<Option<usize>>,
    leading_attrs: Vec<(usize, Attrs, Option<Pos>)>,
    current_block_start: Cell<usize>,
    at_document_level: bool,
    defer_pending: bool,
    drain_follows: bool,
    direct_owner: bool,
}

impl EventLevelState {
    fn new(cur: &LineCursor<'_>, defer_pending: bool, direct_owner: bool) -> Self {
        Self {
            out: Vec::new(),
            block_start_indices: Vec::new(),
            pending_bodies: Vec::new(),
            #[cfg(feature = "experimental-event-parser-options")]
            source_lines: Vec::new(),
            leading_attrs: Vec::new(),
            current_block_start: Cell::new(0),
            at_document_level: cur.at_document_level,
            defer_pending,
            drain_follows: !defer_pending || cur.drain_follows,
            direct_owner,
        }
    }
}

fn parse_mixed_blocks_cursor<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
    mut cur: LineCursor<'_>,
    mut extents: Option<&mut Vec<(usize, usize)>>,
    mut pending_out: Option<&mut Vec<PendingBody>>,
    // The caller fills hollow quotes before reading their children. An
    // unobserved colon body may defer quotes even when this argument is false.
    defer_quotes: bool,
) -> Option<Vec<BlockNode>> {
    let defer_pending = pending_out.is_some();
    let direct_owner = !defer_pending && extents.is_none();
    // Only the direct owner reads `lines`; everything else reads the cursor,
    // so a deferring caller may pass an empty slice instead of materializing.
    debug_assert!(!direct_owner || lines.len() == cur.lines.len());
    // Snapshot the cursor flag: nested paragraph probes may change it.
    let mut state = EventLevelState::new(&cur, defer_pending, direct_owner);
    let mut result = run_event_level::<DEFINITIONS, ABBREVIATIONS>(
        lines,
        options,
        line_map,
        &mut cur,
        &mut state,
        extents.as_deref_mut(),
        pending_out.as_deref_mut(),
        defer_quotes,
        None,
    )?;
    loop {
        match result {
            EventLevelRun::Done(out) => return Some(out),
            EventLevelRun::Yield => unreachable!("the wrapper does not request a boundary"),
            EventLevelRun::NeedDefinitionBody(mut continuation) => {
                cur.restore_state(continuation.cursor);
                state = continuation.state;
                let children = parse_definition_description(
                    continuation.request,
                    &cur,
                    options,
                    &mut None,
                    false,
                );
                continuation.scan.fill_body(children, &cur, options);
                loop {
                    match continuation.scan.step(&mut cur, options) {
                        StepResult::Continue => {}
                        StepResult::NeedBody(request) => {
                            continuation.request = request;
                            continuation.cursor = cur.take_state();
                            continuation.state = std::mem::replace(
                                &mut state,
                                EventLevelState::new(&cur, false, false),
                            );
                            result = EventLevelRun::NeedDefinitionBody(continuation);
                            break;
                        }
                        StepResult::Done => {
                            let node = continuation.scan.finish(&cur, options);
                            if let Some(extents) = extents.as_deref_mut() {
                                extents.push((continuation.start, cur.pos));
                            }
                            if options.positions {
                                state.block_start_indices.push(continuation.start);
                            }
                            #[cfg(feature = "experimental-event-parser-options")]
                            if options.source_lines {
                                state.source_lines.push(continuation.source_line);
                            }
                            debug_assert_eq!(state.out.len(), continuation.index);
                            state.out.push(node);
                            result = run_event_level::<DEFINITIONS, ABBREVIATIONS>(
                                lines,
                                options,
                                line_map,
                                &mut cur,
                                &mut state,
                                extents.as_deref_mut(),
                                pending_out.as_deref_mut(),
                                defer_quotes,
                                None,
                            )?;
                            break;
                        }
                    }
                }
            }
        }
    }
}

enum EventLevelRun {
    Done(Vec<BlockNode>),
    Yield,
    NeedDefinitionBody(Box<DefinitionContinuation>),
}

struct DefinitionContinuation {
    scan: DefinitionListScan,
    request: BodyRequest,
    start: usize,
    source_line: Option<usize>,
    index: usize,
    cursor: LineCursorOwnedState,
    state: EventLevelState,
}

#[allow(dead_code)]
enum TaskPoll {
    NeedBody(MappedSource),
    Done(Vec<BlockNode>),
}

#[allow(dead_code)]
struct MappedEventTask {
    frame: OwnedMappedCursorFrame,
    depth: usize,
    in_group: bool,
    state: Option<EventLevelState>,
    continuation: Option<Box<DefinitionContinuation>>,
    pending: Vec<PendingBody>,
}

#[allow(dead_code)]
impl MappedEventTask {
    fn new(source: MappedSource, options: &Options<'_>) -> Option<Self> {
        let depth = NESTING_DEPTH.with(Cell::get).checked_add(1)?;
        if depth > MAX_NESTING_DEPTH {
            return None;
        }
        let lines: Vec<_> = source.source.lines().collect();
        let mut cursor = LineCursor::new_with_cols(
            &lines,
            Some(&source.line_map),
            source
                .col_map
                .iter()
                .any(Option::is_some)
                .then_some(source.col_map.as_slice()),
        );
        cursor.reached = Some(&source.reached);
        cursor.marker_tail = source.marker_tail;
        cursor.in_quoted_body = source.in_quoted_body;
        cursor.sublists_carry_authored_base =
            options.positions && source.sublists_carry_authored_base;
        cursor.drain_follows = false;
        let state = EventLevelState::new(&cursor, true, false);
        let cursor_state = cursor.take_state();
        drop(cursor);
        drop(lines);
        let frame = OwnedMappedCursorFrame::from_source(source, cursor_state);
        Some(Self {
            frame,
            depth,
            in_group: IN_FIGURE_GROUP.with(Cell::get),
            state: Some(state),
            continuation: None,
            pending: Vec::new(),
        })
    }

    fn poll(&mut self, options: &Options<'_>) -> TaskPoll {
        let _depth = DepthScope::set(self.depth);
        let _group = FigureGroupScope::set(self.in_group);
        assert!(
            self.continuation.is_none(),
            "resume the requested body before polling"
        );
        let state = self.state.as_mut().expect("completed mapped event task");
        let result = self
            .frame
            .with_cursor(|cursor, source| {
                run_event_level::<true, false>(
                    &[],
                    options,
                    Some(&source.line_map),
                    cursor,
                    state,
                    None,
                    Some(&mut self.pending),
                    true,
                    None,
                )
            })
            .expect("mapped event level declined");
        self.handle_result(result, options)
    }

    fn resume_with_children(
        &mut self,
        children: Vec<BlockNode>,
        options: &Options<'_>,
    ) -> TaskPoll {
        let _depth = DepthScope::set(self.depth);
        let _group = FigureGroupScope::set(self.in_group);
        let mut continuation = self.continuation.take().expect("no requested body");
        let result = self.frame.with_cursor(|cursor, source| {
            cursor.restore_state(continuation.cursor);
            let state = self.state.as_mut().expect("completed mapped event task");
            *state = continuation.state;
            continuation.scan.fill_body(children, cursor, options);
            loop {
                match continuation.scan.step(cursor, options) {
                    StepResult::Continue => {}
                    StepResult::NeedBody(request) => {
                        continuation.request = request;
                        continuation.cursor = cursor.take_state();
                        continuation.state =
                            std::mem::replace(state, EventLevelState::new(cursor, true, false));
                        return EventLevelRun::NeedDefinitionBody(continuation);
                    }
                    StepResult::Done => break,
                }
            }
            let node = continuation.scan.finish(cursor, options);
            if options.positions {
                state.block_start_indices.push(continuation.start);
            }
            #[cfg(feature = "experimental-event-parser-options")]
            if options.source_lines {
                state.source_lines.push(continuation.source_line);
            }
            debug_assert_eq!(state.out.len(), continuation.index);
            state.out.push(node);
            run_event_level::<true, false>(
                &[],
                options,
                Some(&source.line_map),
                cursor,
                state,
                None,
                Some(&mut self.pending),
                true,
                None,
            )
            .expect("mapped event level declined")
        });
        self.handle_result(result, options)
    }

    fn handle_result(&mut self, result: EventLevelRun, options: &Options<'_>) -> TaskPoll {
        match result {
            EventLevelRun::NeedDefinitionBody(mut continuation) => {
                let body = std::mem::replace(
                    &mut continuation.request.body,
                    MappedSource::new_line_at(String::new(), None, None),
                );
                self.continuation = Some(continuation);
                TaskPoll::NeedBody(body)
            }
            EventLevelRun::Done(mut blocks) => {
                resolve_pending_bodies(&mut blocks, std::mem::take(&mut self.pending), options);
                self.state = None;
                TaskPoll::Done(blocks)
            }
            EventLevelRun::Yield => unreachable!("mapped task has no stop boundary"),
        }
    }
}

fn collect_event_paragraph<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    cur: &mut LineCursor<'_>,
    invalid_brace_line: bool,
    root_indented_text: bool,
) -> Option<String> {
    let start = cur.pos;
    let mut lines = Vec::new();
    while let Some(next) = cur.peek() {
        if DEFINITIONS && is_definition_placeholder(next) {
            break;
        }
        if ABBREVIATIONS && !lines.is_empty() && detect_abbreviation_def(next).is_some() {
            break;
        }
        if is_blank_line(next) || detect_heading(next).is_some() {
            break;
        }
        if !lines.is_empty()
            && next.len() <= 256
            && !next.starts_with([' ', '\t'])
            && parse_standalone_attrs(next).is_some()
        {
            break;
        }
        if !lines.is_empty() {
            if let Some(open) = detect_fence_open(next) {
                if cur
                    .lines
                    .range(cur.pos + 1..cur.lines.len())
                    .any(|line| is_fence_close(line, open))
                {
                    break;
                }
            }
        }
        if event_paragraph_needs_reader(
            next,
            !lines.is_empty(),
            invalid_brace_line,
            root_indented_text,
        ) {
            cur.pos = start;
            return None;
        }
        lines.push(trim_ascii_end(trim_ascii_start(next)));
        cur.consume();
    }
    Some(lines.join("\n"))
}

// Keep inline parsing out of the recursive event frame's native stack.
#[inline(never)]
fn collect_reference_caption(cur: &mut LineCursor<'_>, options: &Options<'_>) -> Option<BlockNode> {
    let line = cur.peek()?;
    if !reference_caption_shape(cur, line) {
        return None;
    }
    let start = cur.pos;
    let first = trim_ascii_end(trim_ascii_start(line));
    let second = trim_ascii_end(cur.lines.line(start + 1).unwrap());
    let joined = format!("{first}\n{second}");
    cur.consume();
    cur.consume();
    let children = if options.positions {
        parse_inline_lines_with_anchor(
            &joined,
            options,
            vec![
                inline_anchor_for_line(cur, start, first),
                inline_anchor_for_line(cur, start + 1, second),
            ],
        )
    } else {
        parse_inline_with_options(&joined, options)
    };
    Some(BlockNode::Paragraph(Paragraph {
        attrs: None,
        children,
        at_content_column: true,
        block_image: false,
        pos: span_of(cur, start, cur.pos, options),
    }))
}

#[inline(never)]
fn reference_caption_shape(cur: &LineCursor<'_>, line: &str) -> bool {
    !(line.starts_with([' ', '\t'])
        || !reference_image_line(line)
        || !cur
            .lines
            .line(cur.pos + 1)
            .is_some_and(|next| !next.starts_with([' ', '\t']) && caption_content(next).is_some())
        || !cur.lines.line(cur.pos + 2).map_or(true, is_blank_line))
}

#[inline(never)]
fn parse_event_or_standard_paragraph(cur: &mut LineCursor<'_>, options: &Options<'_>) -> BlockNode {
    collect_reference_caption(cur, options)
        .or_else(|| collect_blank_image_caption(cur, options))
        .unwrap_or_else(|| parse_paragraph(cur, options))
}

#[inline(never)]
fn collect_blank_image_caption(
    cur: &mut LineCursor<'_>,
    options: &Options<'_>,
) -> Option<BlockNode> {
    let start = cur.pos;
    let image = cur.peek()?;
    if !image.starts_with("![")
        || (detect_block_image(image).is_none() && !reference_image_line(image))
        || !cur.lines.line(start + 1).is_some_and(is_blank_line)
    {
        return None;
    }
    let caption = cur.lines.line(start + 2)?;
    if caption_content(caption).is_none() || !cur.lines.line(start + 3).map_or(true, is_blank_line)
    {
        return None;
    }
    let image = trim_ascii_end(image);
    let caption = trim_ascii_end(trim_ascii_start(caption));
    let joined = format!("{image}\n{caption}");
    cur.consume();
    cur.consume();
    cur.consume();
    let children = if options.positions {
        parse_inline_lines_with_anchor(
            &joined,
            options,
            vec![
                inline_anchor_for_line(cur, start, image),
                inline_anchor_for_line(cur, start + 2, caption),
            ],
        )
    } else {
        parse_inline_with_options(&joined, options)
    };
    Some(BlockNode::Paragraph(Paragraph {
        attrs: None,
        children,
        at_content_column: true,
        block_image: false,
        pos: span_of(cur, start, cur.pos, options),
    }))
}

#[allow(clippy::too_many_arguments)]
fn run_event_level<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
    cur: &mut LineCursor<'_>,
    state: &mut EventLevelState,
    mut extents: Option<&mut Vec<(usize, usize)>>,
    pending_out: Option<&mut Vec<PendingBody>>,
    defer_quotes: bool,
    stop_at: Option<usize>,
) -> Option<EventLevelRun> {
    macro_rules! emit {
        ($line:expr, $node:expr) => {{
            let node = $node;
            if let Some(extents) = extents.as_deref_mut() {
                extents.push((state.current_block_start.get(), cur.pos));
            }
            if options.positions {
                state
                    .block_start_indices
                    .push(state.current_block_start.get());
            }
            #[cfg(feature = "experimental-event-parser-options")]
            if options.source_lines {
                state.source_lines.push($line);
            }
            state.out.push(node);
        }};
    }
    'blocks: while cur.peek().is_some() {
        if stop_at.is_some_and(|at| cur.pos >= at) {
            return Some(EventLevelRun::Yield);
        }
        if options.positions || extents.is_some() {
            state.current_block_start.set(cur.pos);
        }
        let _block_source_line = cur.source_line(cur.pos);
        if DEFINITIONS && cur.peek().is_some_and(is_definition_placeholder) {
            cur.consume();
            continue;
        }
        let invalid_brace_line = if cur.peek().is_some_and(|line| line.starts_with('{')) {
            let at = cur.pos;
            if let Some(attrs) = parse_standalone_attrs_block(cur) {
                let pos = span_of(cur, at, cur.pos, options);
                state.leading_attrs.push((state.out.len(), attrs, pos));
                continue;
            }
            true
        } else {
            false
        };
        let line = cur.peek().unwrap();
        if is_blank_line(line) {
            cur.consume();
            continue;
        }
        match take_comment_block(cur, options) {
            CommentBlock::NotAComment => {}
            CommentBlock::Consumed(None) => continue,
            CommentBlock::Consumed(Some(node)) => {
                emit!(_block_source_line, *node);
                continue;
            }
        }
        let line = cur.peek().unwrap();
        if ABBREVIATIONS {
            if let Some(mut definition) = detect_abbreviation_def(line) {
                let at = cur.pos;
                cur.consume();
                definition.pos = span_of(cur, at, cur.pos, options);
                emit!(_block_source_line, BlockNode::AbbreviationDef(definition));
                continue;
            }
        }
        if let Some((level, text)) = detect_heading(line) {
            emit!(
                _block_source_line,
                *parse_heading_block(cur, level, text, options)
            );
            continue;
        }
        if line.starts_with(' ') && is_list_marker(trim_ascii_start(line)) {
            if let Some(read) = state
                .direct_owner
                .then(|| direct_root_list_read(lines, cur.pos, options, line_map, true))
                .flatten()
            {
                cur.pos = read.end;
                emit!(_block_source_line, BlockNode::List(read.list));
            } else {
                let mut item_bodies = Vec::new();
                let list = parse_list(cur, options, &mut item_bodies);
                let index = state.out.len();
                emit!(_block_source_line, *list);
                state
                    .pending_bodies
                    .extend(item_bodies.into_iter().map(|body| PendingBody {
                        index,
                        body: Body::Item(body),
                    }));
            }
            continue;
        }
        if is_list_marker(line) && !line.starts_with([' ', '\t']) {
            let direct = state
                .direct_owner
                .then(|| direct_root_list_read(lines, cur.pos, options, line_map, false))
                .flatten();
            if let Some(read) = direct {
                cur.pos = read.end;
                emit!(_block_source_line, BlockNode::List(read.list));
            } else {
                let mut item_bodies = Vec::new();
                let list = parse_list(cur, options, &mut item_bodies);
                let index = state.out.len();
                emit!(_block_source_line, *list);
                state
                    .pending_bodies
                    .extend(item_bodies.into_iter().map(|body| PendingBody {
                        index,
                        body: Body::Item(body),
                    }));
            }
            continue;
        }
        if let Some(node) = parse_marker_hosted_opener(
            cur,
            options,
            line,
            state.out.len(),
            &mut state.pending_bodies,
            state.drain_follows,
        ) {
            emit!(_block_source_line, node);
            continue;
        }
        if is_definition_list_start(line) {
            let direct = if direct_definition_allowed(state.direct_owner, options) {
                parse_simple_definition_list_prefix(lines, cur.pos, options, false)
            } else {
                None
            };
            if let Some((list, end)) = direct {
                cur.pos = end;
                emit!(_block_source_line, BlockNode::DefinitionList(list));
            } else if parse_order_observed(options) && !state.drain_follows {
                let mut scan = DefinitionListScan::new(cur);
                loop {
                    match scan.step(cur, options) {
                        StepResult::Continue => {}
                        StepResult::NeedBody(request) => {
                            return Some(EventLevelRun::NeedDefinitionBody(Box::new(
                                DefinitionContinuation {
                                    scan,
                                    request,
                                    start: state.current_block_start.get(),
                                    source_line: _block_source_line,
                                    index: state.out.len(),
                                    cursor: cur.take_state(),
                                    state: std::mem::replace(
                                        state,
                                        EventLevelState::new(cur, false, false),
                                    ),
                                },
                            )));
                        }
                        StepResult::Done => break,
                    }
                }
                emit!(_block_source_line, scan.finish(cur, options));
            } else {
                emit!(
                    _block_source_line,
                    parse_definition_list_deferring(
                        cur,
                        options,
                        state.out.len(),
                        &mut state.pending_bodies,
                        state.drain_follows,
                    )
                );
            }
            continue;
        }
        if strip_blockquote_prefix(line).is_some() {
            let direct = if direct_quote_allowed(state.direct_owner, options, line) {
                parse_simple_quote(lines, cur.pos, options)
                    .or_else(|| parse_marked_quote(lines, cur.pos, options))
                    .or_else(|| parse_simple_lazy_quote(lines, cur.pos, options))
            } else {
                None
            };
            if let Some((quote, end)) = direct {
                #[cfg(test)]
                DIRECT_QUOTE_PARSES.with(|count| count.set(count.get() + 1));
                cur.pos = end;
                emit!(_block_source_line, BlockNode::BlockQuote(quote));
                continue;
            }
            let (span_start, parts, _) = collect_blockquote_body(cur, options);
            if quote_deferred_in_level(cur, options, &parts, defer_quotes) {
                #[cfg(test)]
                count_colon_quote_deferral(cur);
                let body = QuoteBodyPending {
                    plus_anchor: nested_plus_quote_anchor(cur, options, span_start),
                    parts,
                    depth: NESTING_DEPTH.with(Cell::get),
                    in_group: IN_FIGURE_GROUP.with(Cell::get),
                };
                let node = finish_blockquote_shell(cur, options, span_start, Vec::new(), true);
                let index = state.out.len();
                emit!(_block_source_line, *node);
                state.pending_bodies.push(PendingBody {
                    index,
                    body: Body::Quote(body),
                });
            } else {
                emit!(
                    _block_source_line,
                    *parse_collected_blockquote(cur, options, span_start, parts)
                );
            }
            continue;
        }
        if let Some(fence) = detect_fence_open(line) {
            let mut block = if let Some((block, end)) = state
                .direct_owner
                .then(|| parse_direct_root_code_fence(lines, cur.pos, fence, options))
                .flatten()
            {
                cur.pos = end;
                block
            } else {
                *parse_fenced_block(cur, fence, options)
            };
            let has_leading_attrs = has_pending_leading_attrs(&state.leading_attrs, &state.out);
            if !has_leading_attrs {
                resolve_code_title(&mut block);
            }
            emit!(_block_source_line, block);
            continue;
        }
        if let Some(marker) = thematic_break_marker(line) {
            let at = cur.pos;
            cur.consume();
            emit!(
                _block_source_line,
                BlockNode::ThematicBreak(ThematicBreak {
                    marker: (marker != '-').then_some(marker),
                    pos: span_of(cur, at, cur.pos, options),
                    ..Default::default()
                })
            );
            continue;
        }
        if !line.starts_with([' ', '\t']) && is_table_start(line) {
            emit!(_block_source_line, parse_table(cur, options));
            continue;
        }
        if let Some(mut image) = detect_block_image(line) {
            if image_is_block(cur) {
                let image_at = cur.pos;
                cur.consume();
                image.pos = span_of(cur, image_at, image_at + 1, options);
                if let Some(caption) = consume_caption(cur, options) {
                    emit!(
                        _block_source_line,
                        BlockNode::Figure(Figure {
                            attrs: None,
                            target: Box::new(FigureTarget::Image(image)),
                            rendered_target: None,
                            caption,
                            short_caption: None,
                            pos: span_of(cur, image_at, cur.pos, options),
                        })
                    );
                } else {
                    emit!(
                        _block_source_line,
                        BlockNode::Paragraph(Paragraph {
                            attrs: None,
                            children: vec![InlineNode::Image(image)],
                            at_content_column: true,
                            block_image: true,
                            pos: span_of(cur, image_at, image_at + 1, options),
                        })
                    );
                }
                continue;
            }
        }
        if line.starts_with(":::") {
            if detect_line_block_open(line).is_some() {
                emit!(_block_source_line, parse_line_block(cur, options));
                continue;
            }
            if detect_hardbreaks_block_open(line).is_some() {
                emit!(_block_source_line, parse_hardbreaks_block(cur, options));
                continue;
            }
            if detect_quote_block_open(line).is_some() {
                emit!(_block_source_line, parse_quote_block(cur, options));
                continue;
            }
            if detect_container_open(line).is_some() {
                let (node, body) = open_container(cur, options);
                let index = state.out.len();
                emit!(_block_source_line, *node);
                state.pending_bodies.push(PendingBody {
                    index,
                    body: Body::Colon(body),
                });
                continue;
            }
        }
        let root_indented_text = if line.starts_with(' ') {
            let content = trim_ascii_start(line);
            content
                .as_bytes()
                .first()
                .is_some_and(|byte| *byte == b'>' || byte.is_ascii_alphabetic())
                && !is_list_marker(content)
        } else {
            false
        };
        let late_list_like =
            is_late_list_like_line(cur, line, invalid_brace_line, root_indented_text);
        let late_image_like =
            trim_ascii_start(line).starts_with("![") && detect_block_image(line).is_some();
        let has_leading_attrs = has_pending_leading_attrs(&state.leading_attrs, &state.out);
        if !late_list_like && !late_image_like {
            if let Some(mut block) = try_extension_block(cur, options) {
                finish_unattributed_block(
                    &mut block,
                    cur,
                    state.current_block_start.get(),
                    options,
                    has_leading_attrs,
                );
                emit!(_block_source_line, block);
                continue;
            }
        }
        if late_image_like {
            let mut item_bodies = Vec::new();
            let mut block = parse_block(cur, options, &mut item_bodies)
                .expect("image-like line is not a consumed comment");
            finish_unattributed_block(
                &mut block,
                cur,
                state.current_block_start.get(),
                options,
                has_leading_attrs,
            );
            let index = state.out.len();
            emit!(_block_source_line, *block);
            state
                .pending_bodies
                .extend(item_bodies.into_iter().map(|body| PendingBody {
                    index,
                    body: Body::Item(body),
                }));
            continue;
        }
        if line.starts_with("![")
            && cur.lines.line(cur.pos + 1).is_some_and(is_blank_line)
            && cur
                .lines
                .line(cur.pos + 2)
                .is_some_and(|next| caption_content(next).is_some())
        {
            emit!(
                _block_source_line,
                parse_event_or_standard_paragraph(cur, options)
            );
            continue;
        }
        if !may_start_plain_paragraph(line) && !invalid_brace_line && !root_indented_text {
            if !late_list_like && trim_ascii_start(line).starts_with("$$`") {
                if let Some(equation) = parse_equation_block(cur, options) {
                    emit!(_block_source_line, equation);
                    continue;
                }
            }
            if late_list_like {
                let (mut node, item_bodies) = read_local_list_like_line(cur, options);
                finish_unattributed_block(
                    &mut node,
                    cur,
                    state.current_block_start.get(),
                    options,
                    has_leading_attrs,
                );
                let index = state.out.len();
                emit!(_block_source_line, node);
                state
                    .pending_bodies
                    .extend(item_bodies.into_iter().map(|body| PendingBody {
                        index,
                        body: Body::Item(body),
                    }));
                continue;
            }
            emit!(_block_source_line, parse_paragraph(cur, options));
            continue;
        }
        let standard_paragraph =
            use_standard_event_paragraph(extents.is_some(), state.defer_pending);
        let simple_colon_paragraph = standard_paragraph
            && state.defer_pending
            && extents.is_none()
            && event_owned_plain_paragraph(cur, options);
        if reference_caption_shape(cur, line)
            || (standard_paragraph && !simple_colon_paragraph)
            || crate::includes::code_include_line(line).is_some()
        {
            emit!(
                _block_source_line,
                parse_event_or_standard_paragraph(cur, options)
            );
            continue;
        }
        let paragraph_start = cur.pos;
        let joined = match collect_event_paragraph::<DEFINITIONS, ABBREVIATIONS>(
            cur,
            invalid_brace_line,
            root_indented_text,
        ) {
            Some(joined) => joined,
            None if !state.at_document_level && !state.defer_pending => return None,
            None => {
                // A deferred fragment may already have called extensions.
                emit!(_block_source_line, parse_paragraph(cur, options));
                continue 'blocks;
            }
        };
        let children = if options.positions {
            let anchors = joined
                .split('\n')
                .enumerate()
                .map(|(offset, line)| inline_anchor_for_line(cur, paragraph_start + offset, line))
                .collect();
            parse_inline_lines_with_anchor(&joined, options, anchors)
        } else {
            parse_event_inlines(&joined, options)
        };
        emit!(
            _block_source_line,
            BlockNode::Paragraph(Paragraph {
                attrs: None,
                children,
                at_content_column: !line.starts_with([' ', '\t']),
                block_image: false,
                pos: span_of(cur, paragraph_start, cur.pos, options),
            })
        );
    }
    let mut out = std::mem::take(&mut state.out);
    let block_start_indices = std::mem::take(&mut state.block_start_indices);
    let pending_bodies = std::mem::take(&mut state.pending_bodies);
    #[cfg(feature = "experimental-event-parser-options")]
    let source_lines = std::mem::take(&mut state.source_lines);
    let leading_attrs = std::mem::take(&mut state.leading_attrs);
    if !leading_attrs.is_empty() {
        let mut next_visible = vec![None; out.len() + 1];
        for index in (0..out.len()).rev() {
            next_visible[index] = if matches!(
                out[index],
                BlockNode::AbbreviationDef(_) | BlockNode::Comment(_)
            ) {
                next_visible[index + 1]
            } else {
                Some(index)
            };
        }
        let mut target = None;
        let mut pending = None;
        let mut slots = None;
        let mut dangling_pos = None;
        for (start, attrs, pos) in leading_attrs {
            let Some(next_target) = next_visible[start] else {
                dangling_pos = dangling_pos.or(pos);
                continue;
            };
            if target != Some(next_target) {
                if let (Some(index), Some(attrs)) = (target, pending.take()) {
                    apply_event_leading_attrs(
                        &mut out[index],
                        attrs,
                        cur,
                        block_start_indices.get(index).copied().unwrap_or(0),
                        options,
                    );
                }
                target = Some(next_target);
                slots = None;
            }
            merge_attrs_indexed(&mut pending, attrs, &mut slots);
        }
        if let (Some(index), Some(attrs)) = (target, pending) {
            apply_event_leading_attrs(
                &mut out[index],
                attrs,
                cur,
                block_start_indices.get(index).copied().unwrap_or(0),
                options,
            );
        }
        note_unattached_block_attrs(dangling_pos);
    }
    if let Some(pending_out) = pending_out {
        pending_out.extend(pending_bodies);
    } else {
        resolve_pending_bodies(&mut out, pending_bodies, options);
    }
    #[cfg(feature = "experimental-event-parser-options")]
    if options.source_lines {
        for (node, line) in out.iter_mut().zip(source_lines) {
            if let Some(line) = line {
                stamp_source_line(node, line);
            }
        }
    }
    Some(EventLevelRun::Done(out))
}

#[cold]
#[inline(never)]
fn parse_direct_root_code_fence(
    lines: &[&str],
    start: usize,
    open: FenceOpen,
    options: &Options<'_>,
) -> Option<(BlockNode, usize)> {
    if options.positions || options.source_lines || !options.extensions.is_empty() {
        return None;
    }
    let first = *lines.get(start)?;
    if first[open.lang_start..].trim().starts_with('=') {
        return None;
    }
    let close = (start + 1..lines.len()).find(|&at| {
        is_fence_close(lines[at], open)
            || lines[at].starts_with(LAZY) && is_fence_close(strip_lazy(lines[at]), open)
    })?;
    if lines[close].starts_with(LAZY) {
        return None;
    }
    if lines[start + 1..close]
        .iter()
        .any(|line| line.starts_with(LAZY) || is_definition_placeholder(line))
    {
        return None;
    }
    let next = close + 1;
    if lines
        .get(next)
        .and_then(|line| caption_content(line))
        .is_some()
        || lines.get(next).is_some_and(|line| is_blank_line(line))
            && lines
                .get(next + 1)
                .and_then(|line| caption_content(line))
                .is_some()
    {
        return None;
    }
    let mut content = lines[start + 1..close].join("\n");
    if close > start + 1 {
        content.push('\n');
    }
    let lang = (open.lang_start < open.lang_end)
        .then(|| first[open.lang_start..open.lang_end].to_string());
    let title = open
        .title_start
        .zip(open.title_end)
        .map(|(begin, end)| first[begin..end].to_string());
    let label = open
        .label_start
        .zip(open.label_end)
        .map(|(begin, end)| first[begin..end].to_string());
    Some((
        BlockNode::CodeBlock(CodeBlock {
            attrs: None,
            lang,
            title,
            label,
            content,
            pos: None,
        }),
        next,
    ))
}

enum UnifiedListBlock<'a> {
    Paragraph(Vec<&'a str>),
    List(UnifiedListNode<'a>),
    Block(Box<BlockNode>),
}

struct UnifiedListItem<'a> {
    blocks: Vec<UnifiedListBlock<'a>>,
}

struct UnifiedListNode<'a> {
    kind: SimpleListKind,
    tight: bool,
    items: Vec<UnifiedListItem<'a>>,
}

struct UnifiedListFrame<'a> {
    node: UnifiedListNode<'a>,
    kind: SimpleListKind,
    indent: usize,
    content_indent: usize,
    paragraph: Vec<&'a str>,
    continuation_indent: Option<usize>,
    marker_fence_end: Option<usize>,
}

fn flush_unified_list_paragraph<'a>(frame: &mut UnifiedListFrame<'a>) -> Option<()> {
    if frame.paragraph.is_empty() {
        return Some(());
    }
    frame
        .node
        .items
        .last_mut()?
        .blocks
        .push(UnifiedListBlock::Paragraph(std::mem::take(
            &mut frame.paragraph,
        )));
    Some(())
}

fn close_unified_list_frame(frames: &mut Vec<UnifiedListFrame<'_>>) -> Option<()> {
    let mut child = frames.pop()?;
    flush_unified_list_paragraph(&mut child)?;
    frames
        .last_mut()?
        .node
        .items
        .last_mut()?
        .blocks
        .push(UnifiedListBlock::List(child.node));
    Some(())
}

fn resume_nested_list_after_blank(frames: &mut [UnifiedListFrame<'_>], next: &str) -> Option<()> {
    let next_indent = next.bytes().take_while(|byte| *byte == b' ').count();
    let frame = frames.last_mut()?;
    if next_indent == frame.content_indent {
        let content = &next[next_indent..];
        if strip_blockquote_prefix(content).is_some()
            || detect_fence_open(content).is_some()
            || detect_heading(content).is_some()
        {
            flush_unified_list_paragraph(frame)?;
            return Some(());
        }
        if !is_list_marker(content) && may_start_plain_paragraph(content) {
            flush_unified_list_paragraph(frame)?;
            frame.node.tight = false;
            return Some(());
        }
    }
    if next_indent == frame.indent
        && unified_list_marker(&next[next_indent..]).is_some_and(|(kind, _, _)| kind == frame.kind)
    {
        frame.node.tight = false;
        return Some(());
    }
    None
}

fn consume_pending_list_continuation<'a>(
    frames: &mut [UnifiedListFrame<'a>],
    line: &'a str,
    following: &[&str],
) -> Option<bool> {
    let Some(required_indent) = frames.last().and_then(|frame| frame.continuation_indent) else {
        return Some(false);
    };
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent != required_indent {
        return None;
    }
    if following
        .iter()
        .find(|next| !is_blank_line(next))
        .is_some_and(|next| next.starts_with([' ', '\t']))
    {
        return None;
    }
    let content = trim_ascii_end(&line[indent..]);
    if !may_start_plain_paragraph(content)
        || is_list_marker(content)
        || detect_fence_open(content).is_some()
        || detect_heading(content).is_some()
        || thematic_break_marker(content).is_some()
    {
        return None;
    }
    direct_list_image_line_is_supported(content)?;
    let frame = frames.last_mut()?;
    frame.continuation_indent = None;
    frame.paragraph.push(content);
    Some(true)
}

fn start_standalone_list_continuation(
    frames: &mut [UnifiedListFrame<'_>],
    indent: usize,
    decision: Option<continuation_marker::Decision>,
) -> Option<()> {
    let frame = frames.last_mut()?;
    if indent != frame.indent
        || !decision.is_some_and(|decision| {
            decision.owner == Some(continuation_marker::Kind::List)
                && decision.refusal.is_none()
                && !decision.first_block
        })
    {
        return None;
    }
    flush_unified_list_paragraph(frame)?;
    frame.continuation_indent = Some(indent);
    Some(())
}

fn start_marker_line_list_continuation(
    frames: &mut [UnifiedListFrame<'_>],
    decision: Option<continuation_marker::Decision>,
) -> Option<()> {
    owned_list_first_block(decision).then_some(())?;
    let follow_indent = frames.first()?.indent;
    frames.last_mut()?.continuation_indent = Some(follow_indent);
    Some(())
}

fn build_unified_list(node: UnifiedListNode<'_>, options: &Options<'_>) -> List {
    let mut list = deep_list_node(node.kind);
    list.tight = node.tight;
    list.items = node
        .items
        .into_iter()
        .map(|item| ListItem {
            attrs: None,
            checked: None,
            task_state: None,
            children: item
                .blocks
                .into_iter()
                .map(|block| match block {
                    UnifiedListBlock::Paragraph(lines) => {
                        let inlines = if lines.len() == 1 {
                            parse_event_inlines(lines[0], options)
                        } else {
                            parse_event_inlines(&lines.join("\n"), options)
                        };
                        BlockNode::Paragraph(Paragraph {
                            attrs: None,
                            children: inlines,
                            at_content_column: true,
                            block_image: false,
                            pos: None,
                        })
                    }
                    UnifiedListBlock::List(child) => {
                        BlockNode::List(build_unified_list(child, options))
                    }
                    UnifiedListBlock::Block(block) => *block,
                })
                .collect(),
            pos: None,
        })
        .collect();
    list
}

#[cold]
#[inline(never)]
fn parse_unified_list_quote(
    lines: &[&str],
    start: usize,
    content_indent: usize,
    base_depth: usize,
    frame_depth: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut quote_lines = Vec::new();
    let mut end = start;
    while let Some(&candidate) = lines.get(end) {
        let candidate_indent = candidate.bytes().take_while(|byte| *byte == b' ').count();
        if candidate_indent != content_indent {
            break;
        }
        let candidate = &candidate[candidate_indent..];
        if strip_blockquote_prefix(trim_ascii_end(candidate)).is_none() {
            break;
        }
        quote_lines.push(candidate);
        end += 1;
    }
    if lines.get(end).is_some_and(|next| {
        !is_blank_line(next)
            && next.bytes().take_while(|byte| *byte == b' ').count() >= content_indent
    }) {
        return None;
    }
    let quote_depth = base_depth.saturating_add(frame_depth.saturating_mul(2));
    if quote_depth >= MAX_NESTING_DEPTH
        || quote_lines.iter().any(|line| {
            let mut rest = *line;
            let mut marker_depth = 0usize;
            while let Some(next) = strip_blockquote_prefix(rest) {
                marker_depth += 1;
                rest = next;
            }
            // Reserve the quote's inner block, list, and item scopes.
            quote_depth.saturating_add(marker_depth).saturating_add(3) >= MAX_NESTING_DEPTH
        })
    {
        return None;
    }
    let (quote, consumed) = {
        let _depth = DepthScope::set(quote_depth);
        parse_marked_quote(&quote_lines, 0, options)?
    };
    (consumed == quote_lines.len()).then_some((quote, end))
}

#[cold]
#[inline(never)]
fn parse_unified_list_fence(
    lines: &[&str],
    start: usize,
    content_indent: usize,
    options: &Options<'_>,
) -> Option<(BlockNode, usize)> {
    let read = read_unified_list_fence(lines, start, content_indent, None, options)?;
    Some((read.block, read.end))
}

fn marker_fence_boundary(
    frames: &[UnifiedListFrame<'_>],
    line: &str,
    line_map: Option<&[Option<usize>]>,
    pos: usize,
) -> Option<bool> {
    if frames.len() != 1 || frames[0].marker_fence_end != Some(pos) {
        return None;
    }
    let frame = &frames[0];
    if line.bytes().take_while(|byte| *byte == b' ').count() >= frame.content_indent {
        return None;
    }
    let decision = list_continuation_decision(line_map, pos);
    Some(trim_ascii(line) == "+" && decision.is_some_and(refused_marker_fence_list_plus))
}

struct ListFenceRead {
    block: BlockNode,
    end: usize,
}

#[cold]
#[inline(never)]
fn read_unified_list_fence(
    lines: &[&str],
    start: usize,
    content_indent: usize,
    marker_content: Option<&str>,
    options: &Options<'_>,
) -> Option<ListFenceRead> {
    let prefix = " ".repeat(content_indent);
    let first = marker_content.or_else(|| lines.get(start)?.strip_prefix(prefix.as_str()))?;
    let open = detect_fence_open(first)?;
    let mut collected = vec![first];
    let mut end = start + 1;
    let mut closed = false;
    while let Some(&line) = lines.get(end) {
        let content = if is_blank_line(line) {
            if line.len() > content_indent || !line.bytes().all(|byte| byte == b' ') {
                return None;
            }
            ""
        } else {
            match line.strip_prefix(prefix.as_str()) {
                Some(content) => content,
                None => break,
            }
        };
        collected.push(content);
        end += 1;
        if is_fence_close(content, open) {
            closed = true;
            break;
        }
    }
    if marker_content.is_some() && (closed || end == lines.len())
        || marker_content.is_none() && !closed
    {
        return None;
    }
    let mut cur = LineCursor::new_with_cols(&collected, None, None);
    let mut block = *parse_fenced_block(&mut cur, open, options);
    if cur.pos != collected.len() {
        return None;
    }
    resolve_code_title(&mut block);
    Some(ListFenceRead { block, end })
}

#[inline(never)]
#[cfg(test)]
fn parse_unified_list(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(List, usize)> {
    parse_unified_list_mapped(lines, start, options, None)
}

fn direct_list_image_line_is_supported(content: &str) -> Option<()> {
    detect_block_image(content).is_none().then_some(())
}

struct UnifiedListRead {
    list: List,
    end: usize,
    marker_fence_exit: bool,
}

fn unified_list_paragraph_accepts(content: &str, has_paragraph: bool) -> bool {
    (if has_paragraph {
        may_continue_plain_paragraph(content)
    } else {
        may_start_plain_paragraph(content)
    }) && !is_list_marker(content)
        && detect_fence_open(content).is_none()
        && thematic_break_marker(content).is_none()
}

enum BlankListStep {
    Continue(usize),
    Break,
}

fn advance_unified_list_blank(
    lines: &[&str],
    pos: usize,
    frames: &mut Vec<UnifiedListFrame<'_>>,
    line_map: Option<&[Option<usize>]>,
) -> Option<BlankListStep> {
    let mut after = pos;
    while lines.get(after).is_some_and(|line| is_blank_line(line)) {
        after += 1;
    }
    let Some(&next) = lines.get(after) else {
        return Some(BlankListStep::Break);
    };
    if after - pos != 1 {
        if !next.starts_with([' ', '\t']) && !is_list_marker(next) {
            while frames.len() > 1 {
                close_unified_list_frame(frames)?;
            }
            return Some(BlankListStep::Break);
        }
        return None;
    }
    if frames.len() > 1 && is_plus_marker(next) {
        return None;
    }
    if frames.len() > 1 {
        if next.starts_with([' ', '\t']) {
            resume_nested_list_after_blank(frames, next)?;
            return Some(BlankListStep::Continue(after));
        }
        while frames.len() > 1 {
            close_unified_list_frame(frames)?;
        }
    }
    let frame = frames.last_mut()?;
    if let Some((kind, _, _)) = unified_list_marker(next) {
        if next.bytes().take_while(|byte| *byte == b' ').count() < frame.indent {
            return Some(BlankListStep::Break);
        }
        if kind != frame.kind {
            return Some(BlankListStep::Break);
        }
        frame.node.tight = false;
        return Some(BlankListStep::Continue(after));
    }
    let next_indent = next.bytes().take_while(|byte| *byte == b' ').count();
    if next_indent == frame.indent
        && is_plus_marker(&next[next_indent..])
        && list_continuation_decision(line_map, after).is_some_and(|decision| {
            decision.owner == Some(continuation_marker::Kind::List)
                && decision.refusal.is_none()
                && !decision.first_block
        })
    {
        let blocks = &frame.node.items.last()?.blocks;
        if (!frame.paragraph.is_empty() && blocks.is_empty())
            || (frame.paragraph.is_empty()
                && blocks.len() == 1
                && matches!(blocks[0], UnifiedListBlock::Paragraph(_)))
        {
            frame.node.tight = false;
        }
        return Some(BlankListStep::Continue(after));
    }
    if next_indent == frame.content_indent && unified_list_marker(&next[next_indent..]).is_some() {
        return Some(BlankListStep::Continue(after));
    }
    if is_list_marker(next) {
        if next_indent == frame.indent {
            if unified_list_marker(&next[next_indent..]).is_some_and(|(kind, ordinal, _)| {
                kind == frame.kind
                    && (kind != SimpleListKind::Ordered || ordinal == frame.node.items.len() + 1)
            }) {
                frame.node.tight = false;
                return Some(BlankListStep::Continue(after));
            }
            return None;
        }
        if next_indent > 0
            && next.as_bytes().get(next_indent) != Some(&b'\t')
            && next_indent < frames[0].content_indent
        {
            return Some(BlankListStep::Break);
        }
        return None;
    }
    if next_indent == frame.content_indent {
        flush_unified_list_paragraph(frame)?;
        if strip_blockquote_prefix(&next[next_indent..]).is_none()
            && detect_fence_open(&next[next_indent..]).is_none()
            && detect_heading(&next[next_indent..]).is_none()
            && !trim_ascii_start(&next[next_indent..]).starts_with("%% ")
        {
            frame.node.tight = false;
        }
        return Some(BlankListStep::Continue(after));
    }
    if next.starts_with([' ', '\t']) {
        if next.as_bytes().get(next_indent) != Some(&b'\t')
            && next_indent < frames[0].content_indent
        {
            while frames.len() > 1 {
                close_unified_list_frame(frames)?;
            }
            return Some(BlankListStep::Break);
        }
        return None;
    }
    Some(BlankListStep::Break)
}

#[cfg(test)]
fn parse_unified_list_mapped(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
) -> Option<(List, usize)> {
    let read = parse_unified_list_mapped_with_exit(lines, start, options, line_map)?;
    Some((read.list, read.end))
}

fn consume_unified_list_comment<'a>(
    lines: &[&'a str],
    pos: usize,
    frames: &mut Vec<UnifiedListFrame<'a>>,
    line_map: Option<&[Option<usize>]>,
    options: &Options<'_>,
) -> Option<usize> {
    let mut cursor = LineCursor::new_with_cols(lines, line_map, None);
    cursor.pos = pos;
    let CommentBlock::Consumed(node) = take_comment_block(&mut cursor, options) else {
        return None;
    };
    let frame = frames.last_mut()?;
    flush_unified_list_paragraph(frame)?;
    if pos > 0 && is_blank_line(lines[pos - 1]) {
        let mut after_comments = cursor.pos;
        while lines.get(after_comments).is_some_and(|next| {
            let indent = next.bytes().take_while(|byte| *byte == b' ').count();
            indent == frame.content_indent && trim_ascii_start(&next[indent..]).starts_with("%% ")
        }) {
            after_comments += 1;
        }
        if lines.get(after_comments).is_some_and(|next| {
            let indent = next.bytes().take_while(|byte| *byte == b' ').count();
            let content = trim_ascii_end(&next[indent..]);
            (indent == frame.content_indent
                && unified_list_paragraph_accepts(content, false)
                && detect_heading(content).is_none()
                && strip_blockquote_prefix(content).is_none())
                || (indent == frame.indent && unified_list_marker(content).is_some())
        }) {
            frame.node.tight = false;
        }
    }
    if let Some(node) = node {
        frame
            .node
            .items
            .last_mut()?
            .blocks
            .push(UnifiedListBlock::Block(node));
    }
    Some(cursor.pos)
}

fn unified_list_comment_at_content(
    line: &str,
    indent: usize,
    frames: &[UnifiedListFrame<'_>],
) -> bool {
    frames
        .last()
        .is_some_and(|frame| indent == frame.content_indent)
        && trim_ascii_start(&line[indent..]).starts_with("%% ")
}

fn lower_sibling_marker(line: &str, indent: usize, frames: &[UnifiedListFrame<'_>]) -> bool {
    frames.len() == 1 && indent < frames[0].indent && unified_list_marker(&line[indent..]).is_some()
}

fn lower_sibling_after_comment(
    lines: &[&str],
    pos: usize,
    indent: usize,
    frames: &[UnifiedListFrame<'_>],
) -> bool {
    frames.len() == 1
        && indent > frames[0].indent
        && indent < frames[0].content_indent
        && pos > 0
        && trim_ascii_start(lines[pos - 1]).starts_with("%% ")
}

fn marker_column_colon_continuation(
    line: &str,
    indent: usize,
    frames: &[UnifiedListFrame<'_>],
) -> bool {
    frames.len() == 1
        && frames[0].indent > 0
        && indent == frames[0].indent
        && !frames[0].paragraph.is_empty()
        && trim_ascii_end(&line[indent..]).starts_with(":::")
}

fn parse_unified_list_mapped_with_exit(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
) -> Option<UnifiedListRead> {
    let base_depth = NESTING_DEPTH.with(|depth| depth.get());
    // Leave room for the standard parser's list item and paragraph scopes.
    if base_depth.saturating_add(2) >= MAX_NESTING_DEPTH {
        return None;
    }
    let mut frames: Vec<UnifiedListFrame<'_>> = Vec::new();
    let mut pos = start;
    let mut marker_fence_exit = false;
    while let Some(&line) = lines.get(pos) {
        if let Some(should_close) = marker_fence_boundary(&frames, line, line_map, pos) {
            if should_close {
                marker_fence_exit = true;
                break;
            }
            return None;
        }
        if let Some(required_indent) = frames.last().and_then(|frame| frame.continuation_indent) {
            let indent = line.bytes().take_while(|byte| *byte == b' ').count();
            if indent == required_indent
                && strip_blockquote_prefix(trim_ascii_end(&line[indent..])).is_some()
            {
                let (quote, end) = parse_unified_list_quote(
                    lines,
                    pos,
                    indent,
                    base_depth,
                    frames.len(),
                    options,
                )?;
                let frame = frames.last_mut()?;
                frame.continuation_indent = None;
                frame
                    .node
                    .items
                    .last_mut()?
                    .blocks
                    .push(UnifiedListBlock::Block(Box::new(BlockNode::BlockQuote(
                        quote,
                    ))));
                pos = end;
                continue;
            }
        }
        if consume_pending_list_continuation(&mut frames, line, &lines[pos + 1..])? {
            pos += 1;
            continue;
        }
        if is_blank_line(line) {
            match advance_unified_list_blank(lines, pos, &mut frames, line_map)? {
                BlankListStep::Continue(after) => {
                    pos = after;
                    continue;
                }
                BlankListStep::Break => break,
            }
        }
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        if unified_list_comment_at_content(line, indent, &frames) {
            pos = consume_unified_list_comment(lines, pos, &mut frames, line_map, options)?;
            continue;
        }
        if lower_sibling_marker(line, indent, &frames) {
            break;
        }
        if frames.len() == 1 && indent < frames[0].indent {
            let content = trim_ascii_end(trim_ascii_start(line));
            if frames[0].paragraph.is_empty()
                || !(unified_list_paragraph_accepts(content, true)
                    || indent >= 2 && content.starts_with('|'))
            {
                return None;
            }
            direct_list_image_line_is_supported(content)?;
            frames[0].paragraph.push(content);
            pos += 1;
            continue;
        }
        if marker_column_colon_continuation(line, indent, &frames) {
            frames[0].paragraph.push(trim_ascii_end(&line[indent..]));
            pos += 1;
            continue;
        }
        let decision = trim_ascii_end(line)
            .ends_with('+')
            .then(|| list_continuation_decision(line_map, pos))
            .flatten();
        if is_plus_marker(&line[indent..]) {
            start_standalone_list_continuation(&mut frames, indent, decision)?;
            pos += 1;
            continue;
        }
        let marker = unified_list_marker(&line[indent..])
            .or_else(|| {
                let marker = strict_list_marker(&line[indent..])?;
                detect_fence_open(marker.2).map(|_| marker)
            })
            .or_else(|| plus_first_block_marker(&line[indent..], decision));
        if let Some((kind, ordinal, content)) = marker {
            if lower_sibling_after_comment(lines, pos, indent, &frames) {
                break;
            }
            while frames.len() > 1 && indent < frames.last()?.indent {
                close_unified_list_frame(&mut frames)?;
            }
            if frames.is_empty() {
                if indent > 4 || kind == SimpleListKind::Ordered && ordinal != 1 {
                    return None;
                }
                frames.push(UnifiedListFrame {
                    node: UnifiedListNode {
                        kind,
                        tight: true,
                        items: Vec::new(),
                    },
                    kind,
                    indent,
                    content_indent: indent
                        + if kind == SimpleListKind::Ordered {
                            3
                        } else {
                            2
                        },
                    paragraph: Vec::new(),
                    continuation_indent: None,
                    marker_fence_end: None,
                });
            } else if indent == frames.last()?.content_indent {
                if base_depth.saturating_add(frames.len() + 2) >= MAX_NESTING_DEPTH {
                    return None;
                }
                flush_unified_list_paragraph(frames.last_mut()?)?;
                if kind == SimpleListKind::Ordered && ordinal != 1 {
                    return None;
                }
                frames.push(UnifiedListFrame {
                    node: UnifiedListNode {
                        kind,
                        tight: true,
                        items: Vec::new(),
                    },
                    kind,
                    indent,
                    content_indent: indent
                        + if kind == SimpleListKind::Ordered {
                            3
                        } else {
                            2
                        },
                    paragraph: Vec::new(),
                    continuation_indent: None,
                    marker_fence_end: None,
                });
            } else if indent != frames.last()?.indent {
                return None;
            }
            let frame = frames.last_mut()?;
            if frame.kind != kind {
                if frames.len() == 1 && indent == 0 {
                    break;
                }
                return None;
            }
            if kind == SimpleListKind::Ordered && ordinal != frame.node.items.len() + 1 {
                return None;
            }
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .push(UnifiedListItem { blocks: Vec::new() });
            let mut content = content;
            while let Some((nested_kind, nested_ordinal, nested_content)) =
                strict_list_marker(content)
            {
                if nested_kind == SimpleListKind::Ordered && nested_ordinal != 1 {
                    return None;
                }
                if base_depth.saturating_add(frames.len() + 2) >= MAX_NESTING_DEPTH {
                    return None;
                }
                let nested_indent = frames.last()?.content_indent;
                frames.push(UnifiedListFrame {
                    node: UnifiedListNode {
                        kind: nested_kind,
                        tight: true,
                        items: vec![UnifiedListItem { blocks: Vec::new() }],
                    },
                    kind: nested_kind,
                    indent: nested_indent,
                    content_indent: nested_indent
                        + if nested_kind == SimpleListKind::Ordered {
                            3
                        } else {
                            2
                        },
                    paragraph: Vec::new(),
                    continuation_indent: None,
                    marker_fence_end: None,
                });
                content = nested_content;
            }
            if is_plus_marker(content) {
                start_marker_line_list_continuation(&mut frames, decision)?;
            } else if detect_fence_open(content).is_some() {
                if frames.len() != 1 {
                    return None;
                }
                let frame = frames.last_mut()?;
                let read = read_unified_list_fence(
                    lines,
                    pos,
                    frame.content_indent,
                    Some(content),
                    options,
                )?;
                frame
                    .node
                    .items
                    .last_mut()?
                    .blocks
                    .push(UnifiedListBlock::Block(Box::new(read.block)));
                frame.marker_fence_end = Some(read.end);
                pos = read.end;
                continue;
            } else {
                direct_list_image_line_is_supported(content)?;
                frames.last_mut()?.paragraph.push(content);
            }
            pos += 1;
            continue;
        }
        if indent == 0 && frames.len() == 1 {
            return None;
        }
        let active_frame_depth = frames.len();
        let frame = frames.last_mut()?;
        if indent != frame.content_indent {
            return None;
        }
        let content = trim_ascii_end(&line[indent..]);
        direct_list_image_line_is_supported(content)?;
        if detect_fence_open(content).is_some() {
            let (block, end) = parse_unified_list_fence(lines, pos, frame.content_indent, options)?;
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .last_mut()?
                .blocks
                .push(UnifiedListBlock::Block(Box::new(block)));
            pos = end;
            continue;
        }
        if content.starts_with('>') {
            let (quote, end) = parse_unified_list_quote(
                lines,
                pos,
                frame.content_indent,
                base_depth,
                active_frame_depth,
                options,
            )?;
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .last_mut()?
                .blocks
                .push(UnifiedListBlock::Block(Box::new(BlockNode::BlockQuote(
                    quote,
                ))));
            pos = end;
            continue;
        }
        if let Some((level, text)) = detect_heading(content) {
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .last_mut()?
                .blocks
                .push(UnifiedListBlock::Block(Box::new(BlockNode::Heading(
                    Heading {
                        attrs: None,
                        level,
                        children: parse_event_inlines(trim_ascii_end(text), options),
                        pos: None,
                    },
                ))));
            pos += 1;
            continue;
        }
        if !unified_list_paragraph_accepts(content, !frame.paragraph.is_empty()) {
            return None;
        }
        frame.paragraph.push(content);
        pos += 1;
    }
    if frames
        .iter()
        .any(|frame| frame.continuation_indent.is_some())
    {
        return None;
    }
    while frames.len() > 1 {
        close_unified_list_frame(&mut frames)?;
    }
    let mut root = frames.pop()?;
    flush_unified_list_paragraph(&mut root)?;
    Some(UnifiedListRead {
        list: build_unified_list(root.node, options),
        end: pos,
        marker_fence_exit,
    })
}

#[cfg(test)]
mod unified_list_tests {
    use super::*;

    #[test]
    fn marker_fence_frame_stops_at_refused_plus() {
        let source = "- ```\n+\n  x\n";
        let _markers = continuation_marker::Guard::install(source);
        let lines: Vec<&str> = source.lines().collect();
        let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
        let (list, end) =
            parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map)).unwrap();
        assert_eq!(end, 1);
        assert!(matches!(list.items[0].children[0], BlockNode::CodeBlock(_)));
    }

    #[test]
    fn shared_owner_builds_item_headings() {
        for source in ["- a\n  # H\n", "- a\n\n  # H\n"] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default()).unwrap();
            assert_eq!(consumed, lines.len());
            assert!(list.tight);
            assert!(matches!(list.items[0].children[1], BlockNode::Heading(_)));
        }
    }

    #[test]
    fn shared_owner_builds_marked_quote() {
        for source in ["- one\n  > q\n", "- one\n\n  > q\n"] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default()).unwrap();
            assert_eq!(consumed, lines.len());
            assert!(list.tight);
            assert!(matches!(
                list.items[0].children[1],
                BlockNode::BlockQuote(_)
            ));
        }
        let lazy = "- one\n  > q\n  lazy continuation\n";
        let lines: Vec<&str> = lazy.lines().collect();
        assert!(parse_unified_list(&lines, 0, &Options::default()).is_none());

        let trailing_tab = "- one\n\n  >\t\n";
        let lines: Vec<&str> = trailing_tab.lines().collect();
        assert!(parse_unified_list(&lines, 0, &Options::default()).is_none());
    }

    #[test]
    fn post_blank_tab_does_not_close_list_at_space_count() {
        let source = "- one\n\n \t> q\n";
        let lines: Vec<&str> = source.lines().collect();
        assert!(parse_unified_list(&lines, 0, &Options::default()).is_none());
    }

    #[test]
    fn shared_owner_builds_closed_fences() {
        for source in [
            "- a\n  ```rs\n  let x = 1;\n  ```\n",
            "- a\n\n  ```rs\n  let x = 1;\n  ```\n",
            "- a\n\n  ```rs\n  let x = 1;\n  ```\n\n- b\n",
            "- a\n  ```rs\n  let x = 1;\n  ```\n\n  following\n",
            "- a\n  ```=latex\n  \\x\n  ```\n",
            "- a\n  ```rs\n  let x = 1;\n  ```\n- b\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default()).unwrap();
            assert_eq!(consumed, lines.len());
            let actual = BlockNode::List(list);
            let expected = parse_with_options(source, &Options::default());
            assert_eq!(actual, expected.children[0], "{source:?}");
        }
        let mut checked = 0;
        for marker in ["```", "~~~~", "````"] {
            for before in ["", "\n"] {
                for after in ["", "\n", "\n- b\n"] {
                    for body in ["code", "", "code\n  more"] {
                        let source =
                            format!("- a\n{before}  {marker}rs\n  {body}\n  {marker}\n{after}");
                        let lines: Vec<&str> = source.lines().collect();
                        let Some((list, consumed)) =
                            parse_unified_list(&lines, 0, &Options::default())
                        else {
                            continue;
                        };
                        if consumed != lines.len() {
                            continue;
                        }
                        let expected = parse_with_options(&source, &Options::default());
                        assert_eq!(BlockNode::List(list), expected.children[0], "{source:?}");
                        checked += 1;
                    }
                }
            }
        }
        for source in [
            "- a\n  ```rs\n  code\n  ```",
            "- a\n  ```=latex\n  \\x\n  ```\n- b\n",
            "- a\n  ```csv\n  x,y\n  1,2\n  ```\n",
            "- a\n  ```\n\n  code\n  ```\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default())
                .unwrap_or_else(|| panic!("{source:?}"));
            assert_eq!(consumed, lines.len(), "{source:?}");
            let expected = parse_with_options(source, &Options::default());
            assert_eq!(BlockNode::List(list), expected.children[0], "{source:?}");
            checked += 1;
        }
        assert!(checked >= 20);
        for blank in ["   ", "      ", "\t", "\x0c"] {
            let source = format!("- a\n  ```\n  x\n{blank}\n  y\n  ```\n");
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_unified_list(&lines, 0, &Options::default()).is_none(),
                "{source:?}"
            );
            // A form feed is no blank, so it ends the item and the unclosed fence
            // with it (carve#4018); the walk then moves the closer, and the event
            // route declines a document the walk moves.
            if crate::parse::walk_moves_lines(&source) {
                assert!(parse_document(&source).is_none(), "{source:?}");
                continue;
            }
            let (actual, _) = parse_document(&source).unwrap();
            let expected = parse_with_options(&source, &Options::default());
            assert_eq!(
                crate::ast_json::to_json(&actual),
                crate::ast_json::to_json(&expected)
            );
        }
    }

    #[test]
    fn shared_owner_accepts_loose_siblings_and_declines_lazy_lines() {
        let options = Options::default();
        for source in [
            "- a\n\n  second\n",
            "- a\n  - nested\n\n- b\n",
            "1. a\n   - nested\n\n2. b\n",
            "- a\n\n\n",
            "- a\n  - nested\n\n\nPara\n",
            "- root\n  * middle\n    - leaf\n",
            "- a\n  more\n  - child\n",
            "* a\n  more\n  - child\n",
            "1. a\n   more\n   - child\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_unified_list(&lines, 0, &options).is_some(),
                "{source:?}"
            );
        }
        for source in [
            "- a\nlazy line\n",
            "- a\n\n   wrong indent\n",
            "- a\n\n\n- b\n",
            "- a\n  - n\n\n\n  x\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_unified_list(&lines, 0, &options).is_none(),
                "{source:?}"
            );
        }
        let source = "- a\n  - b\n    - c\n";
        let lines: Vec<&str> = source.lines().collect();
        let _depth = DepthScope::set(MAX_NESTING_DEPTH - 5);
        assert!(parse_unified_list(&lines, 0, &options).is_some());
        drop(_depth);
        let _depth = DepthScope::set(MAX_NESTING_DEPTH - 4);
        assert!(parse_unified_list(&lines, 0, &options).is_none());
    }

    #[test]
    fn shared_owner_keeps_blank_separated_nested_item_content() {
        let options = Options::default();
        for source in [
            "- - A\n\n    second\n  - B\n",
            "- - A\n\n  - B\n",
            "- - A\n\n    second\n\n  - B\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) =
                parse_unified_list(&lines, 0, &options).unwrap_or_else(|| panic!("{source:?}"));
            assert_eq!(consumed, lines.len(), "{source:?}");
            assert_eq!(
                BlockNode::List(list),
                parse_with_options(source, &options).children[0]
            );
        }
    }

    #[test]
    fn raw_fence_does_not_loosen_nested_list() {
        let source = "- a\n\n  - b\n\n    ```=latex\n    \\x\n    ```\n";
        let lines: Vec<&str> = source.lines().collect();
        let direct = parse_unified_list(&lines, 0, &Options::default());
        assert!(direct.is_some());
        let (candidate, _) = parse_document(source).unwrap();
        let standard = parse_with_options(source, &Options::default());
        assert_eq!(candidate, standard);
    }

    #[test]
    fn marker_line_nested_owner_matrix_matches_full_ast() {
        let options = Options::default();
        let mut checked = 0;
        for outer in ["-", "1."] {
            for inner in ["-", "1."] {
                let content_indent = outer.len() + inner.len() + 2;
                let sibling = if inner == "1." { "2." } else { "-" };
                for blank in ["", "\n"] {
                    for continuation in ["more", "# heading", "> quote", "```\ncode\n```"] {
                        for has_sibling in [false, true] {
                            let prefix = " ".repeat(content_indent);
                            let continuation = continuation
                                .lines()
                                .map(|line| format!("{prefix}{line}\n"))
                                .collect::<String>();
                            let mut source = format!("{outer} {inner} a\n{blank}{continuation}");
                            if has_sibling {
                                source.push_str(&format!(
                                    "{}{} b\n",
                                    " ".repeat(outer.len() + 1),
                                    sibling
                                ));
                            }
                            let lines: Vec<&str> = source.lines().collect();
                            if let Some((_, consumed)) = parse_unified_list(&lines, 0, &options) {
                                if consumed == lines.len() {
                                    checked += 1;
                                    let expected = parse_with_options(&source, &options);
                                    assert_eq!(expected.children.len(), 1, "{source:?}");
                                    let (candidate, used) = parse_document(&source).unwrap();
                                    assert!(used, "{source:?}");
                                    assert_eq!(candidate, expected, "{source:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked >= 20, "only {checked} generated cases were owned");
    }
}

fn apply_event_leading_attrs(
    block: &mut BlockNode,
    attrs: Attrs,
    cur: &LineCursor<'_>,
    start_line: usize,
    options: &Options<'_>,
) {
    apply_attrs_to_block(block, attrs);
    promote_delimited_code(block, cur, start_line, options);
    resolve_code_title(block);
}

fn parse_simple_quote(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut parts = Vec::new();
    let mut end = start;
    while let Some(line) = lines.get(end) {
        let Some(content) = line.strip_prefix("> ") else {
            break;
        };
        if content.is_empty()
            || !may_start_plain_paragraph(content)
            || detect_heading(content).is_some()
            || is_list_marker(content)
            || is_definition_list_start(content)
            || detect_fence_open(content).is_some()
            || thematic_break_marker(content).is_some()
        {
            return None;
        }
        parts.push(trim_ascii_end(content));
        end += 1;
    }
    if parts.is_empty() {
        return None;
    }
    let mut after = end;
    while lines.get(after).is_some_and(|line| is_blank_line(line)) {
        after += 1;
    }
    if after == end && end < lines.len()
        || lines.get(after).is_some_and(|line| line.starts_with('^'))
    {
        return None;
    }
    let joined = parts.join("\n");
    Some((
        BlockQuote {
            attrs: None,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_event_inlines(&joined, options),
                at_content_column: true,
                block_image: false,
                pos: None,
            })],
            fenced: false,
            pos: None,
        },
        end,
    ))
}

fn parse_marked_quote(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut parts = Vec::new();
    let mut end = start;
    while let Some(line) = lines.get(end) {
        let Some(content) = strip_blockquote_prefix(line) else {
            break;
        };
        parts.push(content);
        end += 1;
    }
    if parts.is_empty() {
        return None;
    }
    // A final blank quote marker can become a synthetic payload newline.
    if parts.last().is_some_and(|line| is_blank_line(line)) {
        let mut open = None;
        for line in &parts {
            if let Some(fence) = open {
                if is_fence_close(line, fence) {
                    open = None;
                }
            } else {
                open = detect_fence_open(line);
            }
        }
        if open.is_some() {
            return None;
        }
    }
    let mut after = end;
    while lines.get(after).is_some_and(|line| is_blank_line(line)) {
        after += 1;
    }
    if (after == end && end < lines.len())
        || lines.get(after).is_some_and(|line| line.starts_with('^'))
    {
        return None;
    }
    let children = parse_blocks(&parts, options)
        .or_else(|| {
            let (list, consumed) = parse_simple_list(&parts, 0, options)?;
            (consumed == parts.len()).then(|| vec![BlockNode::List(list)])
        })
        .or_else(|| {
            if NESTING_DEPTH.with(|depth| depth.get() >= MIXED_QUOTE_DEPTH_LIMIT) {
                return None;
            }
            let _depth = DepthGuard::enter()?;
            parse_mixed_blocks::<false, false>(&parts, options, false)
        })?;
    Some((
        BlockQuote {
            attrs: None,
            children,
            fenced: false,
            pos: None,
        },
        end,
    ))
}

fn parse_simple_lazy_quote(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut parts = Vec::new();
    let mut end = start;
    let mut lazy = false;
    while let Some(line) = lines.get(end) {
        let content = if !lazy { line.strip_prefix("> ") } else { None };
        let text = content.unwrap_or(line);
        if !may_continue_plain_paragraph(text)
            || detect_heading(text).is_some()
            || is_list_marker(text)
            || is_definition_list_start(text)
            || (content.is_none() && detect_abbreviation_def(text).is_some())
            || detect_fence_open(text).is_some()
            || thematic_break_marker(text).is_some()
        {
            break;
        }
        if content.is_none() {
            lazy = true;
        }
        parts.push(trim_ascii_end(text));
        end += 1;
    }
    if !lazy || parts.is_empty() {
        return None;
    }
    let mut after = end;
    while lines.get(after).is_some_and(|line| is_blank_line(line)) {
        after += 1;
    }
    if after == end && end < lines.len()
        || lines.get(after).is_some_and(|line| line.starts_with('^'))
    {
        return None;
    }
    Some((
        BlockQuote {
            attrs: None,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_event_inlines(&parts.join("\n"), options),
                at_content_column: true,
                block_image: false,
                pos: None,
            })],
            fenced: false,
            pos: None,
        },
        end,
    ))
}

#[cfg(test)]
mod simple_quote_tests {
    use super::*;

    #[test]
    fn simple_marked_quote_builds_public_nodes() {
        let source = "> plain *text*\n> more words\n";
        let lines: Vec<&str> = source.lines().collect();
        let (quote, end) = parse_simple_quote(&lines, 0, &Options::default()).unwrap();
        assert_eq!(end, 2);
        let expected = parse_with_options(source, &Options::default());
        assert_eq!(BlockNode::BlockQuote(quote), expected.children[0]);
    }

    #[test]
    fn lazy_lines_and_captions_keep_the_existing_reader() {
        let options = Options::default();
        for source in [
            "> quoted\nlazy line\n",
            "> quoted\n\n^ caption\n",
            "> # Heading\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_simple_quote(&lines, 0, &options).is_none(),
                "{source:?}"
            );
        }
    }

    #[test]
    fn fully_marked_paragraphs_use_the_recursive_quote_path() {
        let source = "> first\n>\n> second\n";
        let lines: Vec<&str> = source.lines().collect();
        let options = Options::default();
        assert!(parse_simple_quote(&lines, 0, &options).is_none());
        let (quote, end) = parse_marked_quote(&lines, 0, &options).unwrap();
        assert_eq!(end, 3);
        let expected = parse_with_options(source, &options);
        assert_eq!(BlockNode::BlockQuote(quote), expected.children[0]);
    }

    #[test]
    fn fenced_quote_content_uses_the_recursive_quote_path() {
        let options = Options::default();
        for source in ["> ```\n> code\n> ```\n", "> ```=html\n> x\n> ```\n"] {
            let lines: Vec<&str> = source.lines().collect();
            let (quote, end) = parse_marked_quote(&lines, 0, &options).unwrap();
            assert_eq!(end, lines.len(), "{source:?}");
            let expected = parse_with_options(source, &options);
            assert_eq!(
                BlockNode::BlockQuote(quote),
                expected.children[0],
                "{source:?}"
            );
        }
        let unclosed = "> ```=html\n>\n";
        let lines: Vec<&str> = unclosed.lines().collect();
        assert!(parse_marked_quote(&lines, 0, &options).is_none());
    }
}

fn may_start_plain_paragraph(line: &str) -> bool {
    line.as_bytes().first().is_some_and(|first| {
        first.is_ascii_alphanumeric()
            || matches!(first, b'*' | b'`' | b'"' | b'\'' | b'(')
            || may_start_extended_plain_paragraph(*first)
            || *first == b'>' && strip_blockquote_prefix(line).is_none()
    })
}

#[cold]
#[inline(never)]
fn may_start_extended_plain_paragraph(first: u8) -> bool {
    matches!(
        first,
        b'/' | b'_' | b'~' | b'=' | b'\\' | b'[' | b'!' | b'#'
    )
}

fn may_continue_plain_paragraph(line: &str) -> bool {
    (may_start_plain_paragraph(line) && detect_heading(line).is_none())
        || line.starts_with(": ")
        || line.starts_with('(')
}

fn parse_event_inlines(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    if !options.extensions.is_empty() {
        return parse_inline_with_options(text, options);
    }
    if !options.positions && !options.source_lines {
        if let Some(inlines) = parse_plain_strong_term_prefix(text, options) {
            return inlines;
        }
    }
    if let Some(inner) = text
        .strip_prefix("**")
        .and_then(|text| text.strip_suffix("**"))
    {
        if !inner.is_empty()
            && inner
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b' ')
        {
            return vec![InlineNode::text(text.to_string())];
        }
    }
    let bytes = text.as_bytes();
    let plain = (if text.len() < 128 {
        !text.is_empty()
            && !text.contains("..")
            && !text.contains("(c)")
            && !text.contains("(r)")
            && !text.contains("(tm)")
            && bytes.iter().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b' ' | b'\n' | b'.' | b',' | b';' | b'!' | b'?' | b'(' | b')'
                    )
            })
    } else {
        long_literal_inline(text)
    }) || leading_double_star_literal_inline(text);
    if !plain {
        return parse_inline_with_options(text, options);
    }
    let mut out = Vec::new();
    push_literal_lines(&mut out, text);
    out
}

fn push_literal_lines(out: &mut Vec<InlineNode>, text: &str) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push(InlineNode::SoftBreak(Break { pos: None }));
        }
        if !line.is_empty() {
            out.push(InlineNode::text(line.to_string()));
        }
    }
}

fn parse_plain_strong_term_prefix(text: &str, options: &Options<'_>) -> Option<Vec<InlineNode>> {
    let rest = text.strip_prefix('*')?;
    let (term, tail) = rest.split_once("*\n")?;
    if !tail.starts_with(": ")
        || term.is_empty()
        || !term
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'(' | b')'))
        || !term.as_bytes().first()?.is_ascii_alphanumeric()
        || !term.as_bytes().last()?.is_ascii_alphanumeric()
        || term.contains("(c)")
        || term.contains("(r)")
        || term.contains("(tm)")
    {
        return None;
    }
    let mut inlines = Vec::with_capacity(3);
    inlines.push(InlineNode::Emphasis(Emphasis {
        attrs: None,
        kind: EmphasisKind::Strong,
        children: vec![InlineNode::text(term.to_string())],
        pos: None,
    }));
    inlines.push(InlineNode::SoftBreak(Break { pos: None }));
    if literal_term_tail(tail) {
        push_literal_lines(&mut inlines, tail);
    } else {
        inlines.extend(parse_inline_with_options(tail, options));
    }
    Some(inlines)
}

/// Definition text after a strong term that inline parsing would keep literal:
/// `: ` markers at line starts plus a conservative subset of plain bytes.
/// Anything outside the subset falls back to the general inline parser.
fn literal_term_tail(tail: &str) -> bool {
    let bytes = tail.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte.is_ascii_alphanumeric() || !byte.is_ascii() {
            continue;
        }
        let literal = match byte {
            b'\n' | b' ' | b'.' | b',' | b';' | b'?' | b'(' | b')' => true,
            b'!' => bytes.get(index + 1) != Some(&b'='),
            b'-' => bytes.get(index + 1) != Some(&b'-'),
            b':' if index == 0 || bytes[index - 1] == b'\n' => bytes.get(index + 1) == Some(&b' '),
            b':' => bytes[index - 1].is_ascii_alphanumeric(),
            _ => false,
        };
        if !literal {
            return false;
        }
    }
    !tail.contains("..") && !tail.contains("(c)") && !tail.contains("(r)") && !tail.contains("(tm)")
}

/// A leading double-star label and later balanced double-star pairs are
/// literal when the rest of the run contains only literal inline bytes.
fn leading_double_star_literal_inline(text: &str) -> bool {
    if !text.starts_with("**") || text.starts_with("***") {
        return false;
    }
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut pairs = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte.is_ascii_alphanumeric()
            || !byte.is_ascii()
            || matches!(
                byte,
                b' ' | b'\n' | b'.' | b',' | b';' | b'!' | b'?' | b'(' | b')'
            )
        {
            index += 1;
            continue;
        }
        if byte == b'*'
            && bytes.get(index + 1) == Some(&b'*')
            && bytes.get(index + 2) != Some(&b'*')
            && (index == 0 || bytes[index - 1] != b'*')
        {
            pairs += 1;
            index += 2;
            continue;
        }
        if byte == b':'
            && index > 0
            && bytes[index - 1] == b'\n'
            && bytes.get(index + 1) == Some(&b' ')
        {
            index += 1;
            continue;
        }
        if byte == b'-' && bytes.get(index + 1) != Some(&b'-') {
            index += 1;
            continue;
        }
        return false;
    }
    pairs > 0
        && pairs % 2 == 0
        && !text.contains("..")
        && !text.contains("(c)")
        && !text.contains("(r)")
        && !text.contains("(tm)")
}

/// The long route accepts only bytes that the current inline dispatch treats
/// as literal. Review this gate when inline constructs or symbol boundaries change.
fn long_literal_inline(text: &str) -> bool {
    if text.len() < 128 {
        return false;
    }
    let bytes = text.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte.is_ascii_alphanumeric() || !byte.is_ascii() {
            continue;
        }
        let literal = match byte {
            b'\n' | b' ' | b'.' | b',' | b';' | b'?' | b'&' | b'(' | b')' => true,
            b'!' => bytes.get(index + 1) != Some(&b'='),
            b'-' => bytes.get(index + 1) != Some(&b'-'),
            b':' | b'#' | b'@' => {
                index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_')
            }
            b'\\' | b'%' | b'`' | b'$' | b'{' | b'}' | b'[' | b']' | b'<' | b'>' | b'/' | b'*'
            | b'_' | b'~' | b'=' | b'^' | b'+' | b'\'' | b'"' => false,
            _ => false,
        };
        if !literal {
            return false;
        }
    }
    !text.contains("...")
        && !text.contains("(c)")
        && !text.contains("(r)")
        && !text.contains("(tm)")
}

#[cfg(test)]
mod inline_lexer_tests {
    use super::{
        leading_double_star_literal_inline, literal_term_tail, long_literal_inline,
        parse_event_inlines, parse_plain_strong_term_prefix,
    };
    use crate::parse::parse_inline_with_options;
    use crate::Options;

    #[test]
    fn literal_term_tail_matches_general_scanner() {
        let options = Options::default();
        for tail in [
            ": A definition follows.",
            ": Note: the spec, e.g. this (one); really?",
            ": Two lines\nof text!",
            ": First\n: Second",
            ": café, 日本語",
            ": well-formed",
            ": a -- b",
            ": a != b",
            ": wait...",
            ": (c) holder",
            ": foo:smile:",
            ": a :b",
            ":x",
            ": `code`",
            ": it's",
            ": *strong* word",
            ": _emphasis_ word",
            ": see [link](url)",
            ": {attr} text",
            ": back\\slash",
            ": \"quoted\" text",
            ": 50% off",
        ] {
            let text = format!("*Term*\n{tail}");
            assert_eq!(
                parse_event_inlines(&text, &options),
                parse_inline_with_options(&text, &options),
                "{text:?} literal={}",
                literal_term_tail(tail)
            );
        }
        assert!(literal_term_tail(": Note: the spec, e.g. this."));
        assert!(!literal_term_tail(": a -- b"));
        assert!(!literal_term_tail(": a :b"));
        assert!(!literal_term_tail(": `code`"));
    }

    #[test]
    fn strong_term_prefix_matches_general_scanner_for_term_shapes() {
        let options = Options::default();
        for term in [
            "Term",
            "Term 2",
            "Version 1 (beta)",
            "f(x) 42",
            "A (b) c",
            "Brand (c)",
            "Mark (r) here",
            "Name(tm)",
            "x (c) y",
            "(c) start",
            "Brand (C)",
        ] {
            let text = format!("*{term}*\n: A definition follows.");
            if ["(c)", "(r)", "(tm)"]
                .iter()
                .any(|symbol| term.contains(symbol))
            {
                assert!(
                    parse_plain_strong_term_prefix(&text, &options).is_none(),
                    "{text:?} admitted a symbol term"
                );
            }
            assert_eq!(
                parse_event_inlines(&text, &options),
                parse_inline_with_options(&text, &options),
                "{text:?}"
            );
        }
        assert!(parse_plain_strong_term_prefix("*f(x) 42*\n: Def.", &options).is_some());
    }

    #[test]
    fn literal_term_tail_differential_over_small_alphabet() {
        const ALPHABET: [&str; 15] = [
            " ", "\n", ":", "-", "!", ".", "(", ")", "a", "c", "m", "r", "t", "Z", "é",
        ];
        // Multi-byte shapes the exhaustive pass is too short to combine.
        const CHUNKS: [&str; 10] = [
            "(c)", "(r)", "(tm)", "(C)", "...", "--", "\n: ", "a:b", ":smile:", " \n",
        ];
        let options = Options::default();
        let (mut accepted, mut rejected) = (0usize, 0usize);
        let mut check = |body: &str| {
            let tail = format!(": {body}");
            let text = format!("*Term*\n{tail}");
            assert!(
                parse_plain_strong_term_prefix(&text, &options).is_some(),
                "{text:?} missed the term prefix"
            );
            let literal = literal_term_tail(&tail);
            if literal {
                accepted += 1;
            } else {
                rejected += 1;
            }
            assert_eq!(
                parse_event_inlines(&text, &options),
                parse_inline_with_options(&text, &options),
                "{text:?} literal={literal}"
            );
        };
        check("");
        for a in ALPHABET {
            check(a);
            for b in ALPHABET {
                check(&format!("{a}{b}"));
                for c in ALPHABET {
                    check(&format!("{a}{b}{c}"));
                }
            }
        }
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = |bound: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % bound as u64) as usize
        };
        for _ in 0..2000 {
            let mut body = String::new();
            for _ in 0..2 + next(7) {
                let pick = next(ALPHABET.len() + CHUNKS.len());
                body.push_str(if pick < ALPHABET.len() {
                    ALPHABET[pick]
                } else {
                    CHUNKS[pick - ALPHABET.len()]
                });
            }
            check(&body);
        }
        assert!(
            accepted > 200 && rejected > 200,
            "{accepted} accepted, {rejected} rejected"
        );
    }

    #[test]
    fn leading_double_star_gate_matches_general_scanner() {
        let options = Options::default();
        for first in [
            "**a**",
            "**a b**",
            "**éclair**",
            "**a*b**",
            "**a***",
            "***a***",
        ] {
            for tail in [
                "",
                " and more.",
                " and **other**.",
                "\n: A definition follows.",
                "\nMore text.",
                " (c)",
                " ...",
                " `code`",
                " [link](url)",
                " -- dash",
            ] {
                let text = format!("{first}{tail}");
                if leading_double_star_literal_inline(&text) {
                    assert_eq!(
                        parse_event_inlines(&text, &options),
                        parse_inline_with_options(&text, &options),
                        "{text:?}"
                    );
                }
            }
        }
        assert!(leading_double_star_literal_inline(
            "**Core syntax**\n: The specification defines it."
        ));
        assert!(leading_double_star_literal_inline(
            "**Tier 2** and **Tier 3**."
        ));
        assert!(!leading_double_star_literal_inline("***strong***"));
        assert!(!leading_double_star_literal_inline("**label** `code`"));
        for left in [
            "", "a", " ", ".", "-", "!", "?", "(", ")", ": ", "\n: ", "**b**", "é",
        ] {
            for right in [
                "", "a", " ", ".", "-", "!", "?", "(", ")", ": ", "\n: ", "**b**", "é",
            ] {
                let text = format!("**a**{left}{right}");
                if leading_double_star_literal_inline(&text) {
                    assert_eq!(
                        parse_event_inlines(&text, &options),
                        parse_inline_with_options(&text, &options),
                        "{text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn long_literal_gate_admits_only_safe_text() {
        assert!(!long_literal_inline(&"a".repeat(127)));
        assert!(long_literal_inline(&"a".repeat(128)));
        for safe in [
            "café, 日本語",
            "carve#529",
            "foo:bar",
            "a@b",
            "a & b",
            "-x",
            "x!",
        ] {
            assert!(
                long_literal_inline(&format!("{} {safe}", "a".repeat(128))),
                "{safe}"
            );
        }
        for syntax in [
            " @user",
            " #tag",
            " :symbol:",
            " !=",
            "--",
            "...",
            "(c)",
            "*strong*",
            "\t",
            "\r",
            "|",
            "é:x",
        ] {
            assert!(
                !long_literal_inline(&format!("{} {syntax}", "a".repeat(128))),
                "{syntax}"
            );
        }
        for marker in ["#tag", ":symbol:", "@mention"] {
            assert!(!long_literal_inline(&format!(
                "{marker} {}",
                "a".repeat(128)
            )));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SimpleListKind {
    Bullet(u8),
    Ordered,
}

#[cold]
#[inline(never)]
fn parse_simple_definition_list(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(DefinitionList, usize)> {
    if start != 0 {
        return None;
    }
    parse_simple_definition_list_prefix(lines, start, options, true)
}

#[cold]
#[inline(never)]
fn parse_simple_definition_list_prefix(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
    require_end: bool,
) -> Option<(DefinitionList, usize)> {
    if options.positions || options.source_lines || !options.extensions.is_empty() {
        return None;
    }
    let mut pos = start;
    let mut entries = Vec::new();
    while let Some(line) = lines.get(pos) {
        let term = trim_ascii_end(trim_ascii_start(line.strip_prefix(":: ")?));
        if !term.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
            return None;
        }
        pos += 1;
        let def = trim_ascii_end(trim_ascii_start(lines.get(pos)?.strip_prefix(":  ")?));
        if !def.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) || is_list_marker(def) {
            return None;
        }
        pos += 1;
        entries.push((term, def));
        if !lines.get(pos).is_some_and(|line| line.starts_with(":: ")) {
            break;
        }
    }
    if entries.is_empty()
        || (require_end && lines[pos..].iter().any(|line| !is_blank_line(line)))
        || (!require_end && lines.get(pos).is_some_and(|line| !is_blank_line(line)))
        || (!require_end
            && lines[pos..]
                .iter()
                .find(|line| !is_blank_line(line))
                .is_some_and(|line| line.starts_with([' ', '\t', ':'])))
    {
        return None;
    }
    let items = entries
        .into_iter()
        .map(|(term, def)| DefinitionItem {
            terms: vec![DefinitionTerm {
                attrs: None,
                children: parse_event_inlines(term, options),
                pos: None,
            }],
            definitions: vec![DefinitionDef {
                attrs: None,
                children: vec![BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: parse_event_inlines(def, options),
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                })],
                pos: None,
            }],
            pos: None,
        })
        .collect();
    Some((
        DefinitionList {
            attrs: None,
            items,
            loose: false,
            pos: None,
        },
        pos,
    ))
}

fn simple_list_marker(line: &str) -> Option<(SimpleListKind, usize, &str)> {
    let (kind, ordinal, content) = strict_list_marker(line)?;
    simple_list_content(content).then_some((kind, ordinal, content))
}

fn simple_list_content(content: &str) -> bool {
    !(content.starts_with('[')
        || is_list_marker(content)
        || detect_heading(content).is_some()
        || detect_fence_open(content).is_some()
        || thematic_break_marker(content).is_some()
        || !may_start_plain_paragraph(content))
}

fn strict_list_marker(line: &str) -> Option<(SimpleListKind, usize, &str)> {
    let bytes = line.as_bytes();
    let (kind, ordinal, marker_width) = match bytes {
        [b'-' | b'*', b' ', b' ', ..] => return None,
        [b'-' | b'*', b' ', ..] => (SimpleListKind::Bullet(bytes[0]), 0, 2),
        [digit @ b'1'..=b'9', b'.', b' ', b' ', ..] => {
            let _ = digit;
            return None;
        }
        [digit @ b'1'..=b'9', b'.', b' ', ..] => {
            (SimpleListKind::Ordered, (digit - b'0') as usize, 3)
        }
        _ => return None,
    };
    let content = trim_ascii_end(&line[marker_width..]);
    if content.is_empty() {
        return None;
    }
    Some((kind, ordinal, content))
}

fn unified_list_marker(line: &str) -> Option<(SimpleListKind, usize, &str)> {
    let marker = strict_list_marker(line)?;
    if simple_list_content(marker.2) {
        return Some(marker);
    }
    let mut content = marker.2;
    let mut nested = false;
    while let Some((kind, ordinal, inner)) = strict_list_marker(content) {
        if kind == SimpleListKind::Ordered && ordinal != 1 {
            return None;
        }
        nested = true;
        content = inner;
    }
    (nested && simple_list_content(content)).then_some(marker)
}

fn list_continuation_decision(
    line_map: Option<&[Option<usize>]>,
    pos: usize,
) -> Option<continuation_marker::Decision> {
    line_map
        .and_then(|map| map.get(pos).copied().flatten())
        .and_then(continuation_marker::decision)
}

fn plus_first_block_marker(
    line: &str,
    decision: Option<continuation_marker::Decision>,
) -> Option<(SimpleListKind, usize, &str)> {
    let decision = decision?;
    if decision.owner != Some(continuation_marker::Kind::List)
        || decision.refusal.is_some()
        || !decision.first_block
    {
        return None;
    }
    let first = strict_list_marker(line)?;
    let mut content = first.2;
    while let Some((kind, ordinal, inner)) = strict_list_marker(content) {
        if kind == SimpleListKind::Ordered && ordinal != 1 {
            return None;
        }
        content = inner;
    }
    is_plus_marker(content).then_some(first)
}

fn owned_list_first_block(decision: Option<continuation_marker::Decision>) -> bool {
    decision.is_some_and(|decision| {
        decision.owner == Some(continuation_marker::Kind::List)
            && decision.refusal.is_none()
            && decision.first_block
    })
}

fn parse_simple_list(lines: &[&str], start: usize, options: &Options<'_>) -> Option<(List, usize)> {
    let (kind, first_number, _) = simple_list_marker(lines.get(start)?)?;
    if kind == SimpleListKind::Ordered && first_number != 1 {
        return None;
    }
    let content_column = if kind == SimpleListKind::Ordered {
        3
    } else {
        2
    };
    let mut pending_items = Vec::new();
    let mut pos = start;
    while let Some((item_kind, ordinal, content)) =
        lines.get(pos).and_then(|line| simple_list_marker(line))
    {
        if item_kind != kind {
            break;
        }
        if kind == SimpleListKind::Ordered && ordinal != pending_items.len() + 1 {
            return None;
        }
        let mut item_lines = vec![content];
        pos += 1;
        while let Some(&line) = lines.get(pos) {
            if is_blank_line(line) || simple_list_marker(line).is_some() {
                break;
            }
            let prefix = &line.as_bytes()[..line.len().min(content_column)];
            if prefix.iter().all(|byte| *byte == b' ') && prefix.len() == content_column {
                let content = trim_ascii_end(trim_ascii_start(line));
                if !may_continue_plain_paragraph(content)
                    || is_list_marker(content)
                    || detect_fence_open(content).is_some()
                    || thematic_break_marker(content).is_some()
                    || detect_heading(content).is_some()
                {
                    return None;
                }
                item_lines.push(content);
                pos += 1;
            } else {
                return None;
            }
        }
        pending_items.push(item_lines);
        if lines.get(pos).is_some_and(|line| is_blank_line(line)) {
            let mut next = pos;
            while lines.get(next).is_some_and(|line| is_blank_line(line)) {
                next += 1;
            }
            if lines
                .get(next)
                .is_some_and(|line| is_list_marker(line) || line.starts_with([' ', '\t']))
            {
                return None;
            }
            break;
        }
        if lines
            .get(pos)
            .and_then(|line| simple_list_marker(line))
            .is_none()
        {
            break;
        }
    }
    let items = pending_items
        .into_iter()
        .map(|item_lines: Vec<&str>| {
            let inlines = if item_lines.len() == 1 {
                parse_event_inlines(item_lines[0], options)
            } else {
                parse_event_inlines(&item_lines.join("\n"), options)
            };
            ListItem {
                attrs: None,
                checked: None,
                task_state: None,
                children: vec![BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: inlines,
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                })],
                pos: None,
            }
        })
        .collect();
    Some((
        List {
            attrs: None,
            ordered: kind == SimpleListKind::Ordered,
            start: None,
            ol_type: None,
            bare_marker: false,
            delim: None,
            bullet_char: match kind {
                SimpleListKind::Bullet(b'*') => Some('*'),
                _ => None,
            },
            tight: true,
            items,
            pos: None,
        },
        pos,
    ))
}

fn deep_list_node(kind: SimpleListKind) -> List {
    List {
        attrs: None,
        ordered: kind == SimpleListKind::Ordered,
        start: None,
        ol_type: None,
        bare_marker: false,
        delim: None,
        bullet_char: match kind {
            SimpleListKind::Bullet(b'*') => Some('*'),
            _ => None,
        },
        tight: true,
        items: Vec::new(),
        pos: None,
    }
}

/// Match only the characters normalized by the event route.
fn needs_event_normalization(source: &str) -> bool {
    let bytes = source.as_bytes();
    if memchr::memchr2(b'\r', b'\0', bytes).is_some() {
        return true;
    }
    let mut start = 0;
    while let Some(offset) = memchr::memchr2(0xee, 0xef, &bytes[start..]) {
        let at = start + offset;
        if bytes[at..].starts_with(b"\xef\xbb\xbf")
            || bytes[at..].starts_with(b"\xee\x80\x85")
            || bytes[at..].starts_with(b"\xee\x80\x86")
        {
            return true;
        }
        start = at + 1;
    }
    false
}

fn has_fence_candidate_without_final_newline(source: &str) -> bool {
    if source.ends_with('\n') || !source.contains(['`', '~']) {
        return false;
    }
    if source.contains("[^") && (source.contains("```") || source.contains("~~~")) {
        return true;
    }
    source.lines().any(|line| {
        let mut content = trim_ascii_start(line);
        loop {
            if detect_fence_open(content).is_some() {
                return true;
            }
            if let Some(body) = strip_blockquote_prefix(content) {
                content = trim_ascii_start(body);
            } else if let Some(marker) = detect_list_marker_full(content) {
                content = trim_ascii_start(marker.content);
            } else {
                return false;
            }
        }
    })
}

pub(super) fn parse_document(source: &str) -> Option<(Document, bool)> {
    let original_source = source;
    let source_len = source.len();
    let normalized = if needs_event_normalization(source) {
        Some(normalize_source(source))
    } else {
        None
    };
    let source = normalized.as_deref().unwrap_or(source);
    // Rule A: read the walked text, as the standard root does.
    let owned = own_lines(source);
    let _owned_edits = EditsGuard::install(owned.as_ref());
    let source = owned.as_ref().map_or(source, |o| o.source.as_str());
    if has_deep_quote_chain(source) {
        // Only the option-aware route parses quote bodies on the worklist.
        #[cfg(feature = "experimental-event-parser-options")]
        return parse_document_with_custom_options(
            original_source,
            &Options::default(),
            ParseMode::Html,
        )
        .map(|(document, _, admitted)| (document, admitted));
        #[cfg(not(feature = "experimental-event-parser-options"))]
        {
            let _ = original_source;
            return None;
        }
    }
    if has_marker_description_comment_risk(source) {
        // The authored text: the options root walks it again.
        #[cfg(feature = "experimental-event-parser-options")]
        return parse_document_with_custom_options(
            original_source,
            &Options::default(),
            ParseMode::Html,
        )
        .map(|(document, _, admitted)| (document, admitted));
        #[cfg(not(feature = "experimental-event-parser-options"))]
        return None;
    }
    let may_have_link_defs = source.contains("]:");
    let may_have_abbrevs = source.contains("*[");
    if source.contains("[^") {
        return parse_document_with_footnotes(source, source_len);
    }
    if has_fence_candidate_without_final_newline(source) {
        return None;
    }
    let options = Options::default();
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, false);
    let (stripped_body, link_defs) = if may_have_link_defs {
        let mut budget = probe_budget_for(body.len());
        extract_link_defs_guarded(body, &options, &mut budget)
    } else {
        (String::new(), BTreeMap::new())
    };
    let has_link_defs = !link_defs.is_empty();
    let body_source = if has_link_defs {
        stripped_body.as_str()
    } else {
        body
    };
    let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let root_line_map: Option<Vec<Option<usize>>> = continuation_marker::active().then(|| {
        (body_start_line..body_start_line + body_source.lines().count())
            .map(Some)
            .collect()
    });
    let (direct, link_defs, needs_late_resolution) = {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = body_source.lines().collect();
        if has_link_defs {
            with_active_link_defs(link_defs, || {
                parse_blocks(&lines, &options).or_else(|| {
                    if may_have_abbrevs {
                        parse_mixed_blocks_mapped::<true, true>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                            true,
                        )
                    } else {
                        parse_mixed_blocks_mapped::<true, false>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                            true,
                        )
                    }
                })
            })
        } else {
            (
                parse_blocks(&lines, &options).or_else(|| {
                    if lines
                        .first()
                        .is_some_and(|line| is_definition_list_start(line))
                    {
                        if let Some((list, _)) = parse_simple_definition_list(&lines, 0, &options) {
                            return Some(vec![BlockNode::DefinitionList(list)]);
                        }
                    }
                    if may_have_abbrevs {
                        parse_mixed_blocks_mapped::<false, true>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                            true,
                        )
                    } else {
                        parse_mixed_blocks_mapped::<false, false>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                            true,
                        )
                    }
                }),
                link_defs,
                false,
            )
        }
    };
    if has_link_defs && direct.is_none() {
        return None;
    }
    let used_event_route = direct.is_some();
    let children = direct.unwrap_or_else(|| {
        let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let line_count = body.lines().count();
        let joined = joined_source(&body.lines().collect::<Vec<_>>());
        let mapped = MappedSource {
            source: joined,
            line_map: (body_start_line..body_start_line + line_count)
                .map(Some)
                .collect(),
            col_map: Vec::new(),
            reached: Vec::new(),
            opaque_blank_lines: Vec::new(),
            discarded_outer_blank: false,
            outer_plus_after_blank: false,
            item_marker_at_start: false,
            marker_tail: None,
            in_quoted_body: false,
            sublists_carry_authored_base: false,
        };
        parse_mapped_source_at_document_level(&mapped, &options)
    });
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children,
        source_len,
        ingest_payload_len: 0,
    };
    if needs_late_resolution || source.contains("][") {
        let headings = heading_reference_index_for_document(&doc, options.heading_id_options());
        resolve_reference_links(&mut doc, &link_defs, &headings);
    }
    append_link_reference_definitions(&mut doc, &link_defs, source, None);
    if may_have_abbrevs {
        apply_abbreviations(&mut doc);
    }
    if source.contains("![") {
        promote_block_images(&mut doc.children, false, true);
    }
    if source.contains('^') {
        number_crossref_captions(&mut doc);
    }
    let crossref_index = if source.contains("</#") {
        let (index, ids) =
            crossref_index_and_heading_ids_for_document(&doc, options.heading_id_options());
        stamp_generated_heading_ids_from(&mut doc, ids);
        Some(index)
    } else {
        stamp_generated_heading_ids(&mut doc, options.heading_id_options());
        None
    };
    let _ = crate::render::collect_footnotes(&mut doc, false);
    coalesce_text_runs(&mut doc);
    if let Some(index) = crossref_index {
        fill_crossref_hrefs_from_index(&mut doc, index);
    }
    Some((doc, used_event_route))
}

fn parse_document_with_footnotes(source: &str, source_len: usize) -> Option<(Document, bool)> {
    let may_have_abbrevs = source.contains("*[");
    if needs_event_normalization(source) || has_fence_candidate_without_final_newline(source) {
        return None;
    }
    let options = Options::default();
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, false);
    let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let (mapped, mut footnote_sources, mut footnote_def_pos, mut note_links) =
        extract_footnote_defs(body, body_start_line, false, false, None, &options);
    if footnote_sources
        .values()
        .any(|note| note.source.contains("[^"))
    {
        extract_nested_footnote_defs(
            &mut footnote_sources,
            &mut footnote_def_pos,
            &mut note_links,
            &options,
        );
    }
    let mut budget = probe_budget_for(mapped.source.len());
    let (body_source, mut link_defs) =
        extract_link_defs_guarded(&mapped.source, &options, &mut budget);
    for def in link_defs.values_mut() {
        if let Some(line) = def.line {
            def.line = mapped
                .line_map
                .get(line)
                .and_then(|mapped| *mapped)
                .map(|authored| authored - 1)
                .or(def.line);
        }
    }
    for (label, def) in note_links {
        link_defs.entry(label).or_insert(def);
    }
    let mapped = remap_source(body_source, &mapped);
    let (parsed, link_defs, needs_late_resolution) = with_active_link_defs(link_defs, || {
        let _depth = DepthGuard::enter()?;
        let footnote_defs = footnote_sources
            .into_iter()
            .map(|(label, note)| {
                let children = if note.source.trim_end_matches([' ', '\t', '\n']) == "{empty}" {
                    Vec::new()
                } else {
                    let lines: Vec<&str> = note.source.lines().collect();
                    parse_blocks(&lines, &options)
                        .unwrap_or_else(|| parse_mapped_source(&note, &options))
                };
                (label, children)
            })
            .collect();
        let lines: Vec<&str> = mapped.source.lines().collect();
        let children = parse_blocks(&lines, &options).or_else(|| {
            if may_have_abbrevs {
                parse_mixed_blocks_mapped::<true, true>(
                    &lines,
                    &options,
                    Some(&mapped.line_map),
                    true,
                )
            } else {
                parse_mixed_blocks_mapped::<true, false>(
                    &lines,
                    &options,
                    Some(&mapped.line_map),
                    true,
                )
            }
        })?;
        Some((children, footnote_defs))
    });
    let (children, footnote_defs) = parsed?;
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
        children,
        source_len,
        ingest_payload_len: 0,
    };
    if needs_late_resolution || source.contains("][") {
        let headings = heading_reference_index_for_document(&doc, options.heading_id_options());
        resolve_reference_links(&mut doc, &link_defs, &headings);
    }
    append_link_reference_definitions(&mut doc, &link_defs, source, None);
    if may_have_abbrevs {
        apply_abbreviations(&mut doc);
    }
    if source.contains("![") {
        promote_block_images(&mut doc.children, false, true);
        for blocks in doc.footnote_defs.values_mut() {
            promote_block_images(blocks, false, true);
        }
    }
    if source.contains('^') {
        number_crossref_captions(&mut doc);
    }
    stamp_generated_heading_ids(&mut doc, options.heading_id_options());
    let _ = crate::render::collect_footnotes(&mut doc, false);
    coalesce_text_runs(&mut doc);
    if source.contains("</#") {
        fill_crossref_hrefs(&mut doc, options.heading_id_options());
    }
    Some((doc, true))
}

#[cfg(feature = "experimental-event-parser-options")]
#[cold]
#[inline(never)]
pub(super) fn parse_document_with_options(
    source: &str,
    options: &Options<'_>,
) -> Option<(Document, bool)> {
    parse_document_with_custom_options(source, options, ParseMode::Html)
        .map(|(doc, _, used)| (doc, used))
}

#[cfg(all(feature = "experimental-event-parser-options", test))]
pub(super) fn parse_document_with_mode(
    source: &str,
    options: &Options<'_>,
    mode: ParseMode,
) -> Option<(Document, Option<CrossrefIndex>, bool)> {
    parse_document_with_custom_options(source, options, mode)
}

fn has_deep_quote_chain(source: &str) -> bool {
    if !source.contains("> ") && !source.contains(">>") && !source.contains(">\t") {
        return false;
    }
    source.lines().any(|line| {
        let mut rest = line;
        for _ in 0..64 {
            let Some(next) = strip_blockquote_prefix(trim_ascii_start(rest)) else {
                return false;
            };
            rest = next;
        }
        true
    })
}

/// Whether a document with a deep quote chain keeps its quote levels on the
/// worklist. A quote level stays eager, one native frame chain per level, only
/// where `quote_body_deferrable` refuses it, and every refusal needs a line of
/// its own: a caption, or, when the parse order is observed, the first line
/// after the quote's last marked line, which is shallower than that line. A
/// `+` line may split the parts of every quote around it, so it counts once
/// per level. A quote marker after other text on its line (`- > >`) or a
/// colon body is not proven here, so those decline. So does any line deeper
/// than an earlier line that may open a list item or definition body: such a
/// body parses its quotes eagerly, outside the deferral and this budget.
#[cfg(feature = "experimental-event-parser-options")]
pub(super) fn deep_quote_chain_owned(source: &str, options: &Options<'_>) -> bool {
    // Footnote bodies parse on their own route.
    if source.contains("[^") {
        return false;
    }
    let observed = parse_order_observed(options);
    let mut eager = 0usize;
    let mut previous = 0usize;
    // The shallowest depth at which a line may have opened an item body.
    let mut item_depth: Option<usize> = None;
    for line in source.lines() {
        let mut rest = trim_ascii_start(line);
        let mut depth = 0usize;
        while let Some(next) = strip_blockquote_prefix(rest) {
            rest = trim_ascii_start(next);
            depth += 1;
        }
        if rest.contains('>') || rest.contains(":::") {
            return false;
        }
        // Indentation is trimmed above, so a quote an item body hosts always
        // counts deeper than the item's own line.
        if item_depth.is_some_and(|item| depth > item) {
            return false;
        }
        let trimmed = trim_ascii_end(rest);
        if may_open_item_body(trimmed) {
            item_depth = Some(item_depth.map_or(depth, |item| item.min(depth)));
        }
        if trimmed == "+" || trimmed.ends_with(" +") {
            eager += depth + 1;
        } else if caption_content(rest).is_some() || (observed && depth < previous) {
            eager += 1;
        }
        if eager > DEEP_QUOTE_EAGER_BUDGET {
            return false;
        }
        previous = depth;
    }
    true
}

/// Over-approximates every list marker (bullet, decimal, alphabetic, roman,
/// bare dot) and definition marker (`::`, `:`) that could open a body.
#[cfg(feature = "experimental-event-parser-options")]
fn may_open_item_body(line: &str) -> bool {
    let bytes = line.as_bytes();
    match bytes.first() {
        Some(b'-' | b'*' | b'.' | b':') => true,
        // A bare `+` is the quote continuation marker, counted on its own.
        Some(b'+') => bytes.len() > 1,
        Some(byte) if byte.is_ascii_alphanumeric() => {
            let run = bytes
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric())
                .count();
            matches!(bytes.get(run), Some(b'.' | b')' | b'{'))
        }
        _ => false,
    }
}

fn has_marker_description_comment_risk(source: &str) -> bool {
    if !source.contains("%%%") || !source.contains("::") {
        return false;
    }
    source.lines().any(|line| {
        detect_list_marker_full(trim_ascii_start(line))
            .is_some_and(|marker| marker.content.starts_with(":: "))
    })
}

#[cfg(feature = "experimental-event-parser-options")]
pub(super) fn parse_document_with_custom_options(
    source: &str,
    options: &Options<'_>,
    mode: ParseMode,
) -> Option<(Document, Option<CrossrefIndex>, bool)> {
    if NESTING_DEPTH.with(|depth| depth.get() >= MAX_NESTING_DEPTH) {
        return None;
    }
    let original_source = source;
    let source_len = source.len();
    let normalized = if needs_event_normalization(source) {
        Some(normalize_source(source))
    } else {
        None
    };
    let source = normalized.as_deref().unwrap_or(source);
    // Rule A: read the walked text, as the standard root does. The guard maps
    // walked columns back to the authored lines in the offset pass.
    let owned = own_lines(source);
    let _owned_edits = EditsGuard::install(owned.as_ref());
    let source = owned.as_ref().map_or(source, |o| o.source.as_str());
    if has_deep_quote_chain(source) && !deep_quote_chain_owned(source, options) {
        return None;
    }
    let may_have_link_defs = source.contains("]:") || !options.extensions.is_empty();
    let may_have_abbrevs = source.contains("*[");
    if source.contains("[^") {
        let (_, _, body) = split_frontmatter(source, options.positions);
        if body.contains("[^") {
            return parse_document_with_footnotes_options(
                source,
                original_source,
                source_len,
                options,
                mode,
            );
        }
    }
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, options.positions);
    let (stripped_body, mut link_defs) = if may_have_link_defs {
        let mut budget = probe_budget_for(body.len());
        extract_link_defs_guarded(body, options, &mut budget)
    } else {
        (String::new(), BTreeMap::new())
    };
    let has_link_defs = !link_defs.is_empty();
    let body_source = if has_link_defs {
        stripped_body.as_str()
    } else {
        body
    };
    let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    if options.positions && has_link_defs {
        for def in link_defs.values_mut() {
            def.line = def.line.map(|line| line + body_start_line - 1);
        }
    }
    let marker_description_comment = has_marker_description_comment_risk(source);
    let root_line_map: Option<Vec<Option<usize>>> = (marker_description_comment
        || continuation_marker::active()
        || options.source_lines
        || options.positions
        || !source.ends_with('\n'))
    .then(|| {
        (body_start_line..body_start_line + body_source.lines().count())
            .map(Some)
            .collect()
    });
    let (direct, link_defs, needs_late_resolution) = {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = body_source.lines().collect();
        if options.positions {
            with_active_link_defs(link_defs, || {
                if has_link_defs {
                    if may_have_abbrevs {
                        parse_custom_mixed_blocks::<true, true>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            marker_description_comment,
                        )
                    } else {
                        parse_custom_mixed_blocks::<true, false>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            marker_description_comment,
                        )
                    }
                } else if may_have_abbrevs {
                    parse_custom_mixed_blocks::<false, true>(
                        &lines,
                        options,
                        root_line_map.as_deref(),
                        marker_description_comment,
                    )
                } else {
                    parse_positioned_simple_blocks(&lines, options, root_line_map.as_deref()?)
                        .or_else(|| {
                            parse_custom_mixed_blocks::<false, false>(
                                &lines,
                                options,
                                root_line_map.as_deref(),
                                marker_description_comment,
                            )
                        })
                }
            })
        } else if has_link_defs {
            with_active_link_defs(link_defs, || {
                parse_blocks_mapped(&lines, options, root_line_map.as_deref()).or_else(|| {
                    if may_have_abbrevs {
                        parse_custom_mixed_blocks::<true, true>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            marker_description_comment,
                        )
                    } else {
                        parse_custom_mixed_blocks::<true, false>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            marker_description_comment,
                        )
                    }
                })
            })
        } else {
            (
                parse_blocks_mapped(&lines, options, root_line_map.as_deref()).or_else(|| {
                    if options.extensions.is_empty()
                        && lines
                            .first()
                            .is_some_and(|line| is_definition_list_start(line))
                    {
                        if let Some((list, _)) = parse_simple_definition_list(&lines, 0, options) {
                            return Some(vec![BlockNode::DefinitionList(list)]);
                        }
                    }
                    if may_have_abbrevs {
                        parse_custom_mixed_blocks::<false, true>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            marker_description_comment,
                        )
                    } else {
                        parse_custom_mixed_blocks::<false, false>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            marker_description_comment,
                        )
                    }
                }),
                link_defs,
                false,
            )
        }
    };
    let used_event_route = direct.is_some();
    let mut children = match direct {
        Some(children) => children,
        None if has_link_defs || options.positions => return None,
        None => {
            let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            let line_count = body.lines().count();
            let joined = joined_source(&body.lines().collect::<Vec<_>>());
            let mapped = MappedSource {
                source: joined,
                line_map: (body_start_line..body_start_line + line_count)
                    .map(Some)
                    .collect(),
                col_map: Vec::new(),
                reached: Vec::new(),
                opaque_blank_lines: Vec::new(),
                discarded_outer_blank: false,
                outer_plus_after_blank: false,
                item_marker_at_start: false,
                marker_tail: None,
                in_quoted_body: false,
                sublists_carry_authored_base: false,
            };
            parse_mapped_source_at_document_level(&mapped, options)
        }
    };
    if options.positions {
        let line_starts = CodepointLineStarts::original(original_source);
        fill_offsets(&mut children, &line_starts);
        narrow_to_last_placed_child(&mut children, &original_source.lines().collect::<Vec<_>>());
    }
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children,
        source_len,
        ingest_payload_len: 0,
    };
    if needs_late_resolution || source.contains("][") {
        let headings = heading_reference_index_for_document(&doc, options.heading_id_options());
        resolve_reference_links(&mut doc, &link_defs, &headings);
    }
    let original_line_starts = options
        .positions
        .then(|| CodepointLineStarts::original(original_source));
    if !matches!(mode, ParseMode::HtmlFacade) {
        append_link_reference_definitions(
            &mut doc,
            &link_defs,
            source,
            original_line_starts.as_ref(),
        );
    }
    let mut crossref_index = if matches!(mode, ParseMode::Html | ParseMode::HtmlFacade) {
        if may_have_abbrevs {
            apply_abbreviations(&mut doc);
        }
        if source.contains("![") {
            promote_block_images(&mut doc.children, false, true);
        }
        let caption_titles = if source.contains('^') {
            number_crossref_captions(&mut doc)
        } else {
            BTreeMap::new()
        };
        let index = if matches!(mode, ParseMode::HtmlFacade) {
            let (mut index, _) = heading_index(
                &doc.children,
                &doc.footnote_defs,
                options.heading_id_options(),
                source.contains("</#"),
                false,
            );
            index.extend_titles(caption_titles);
            stamp_generated_heading_ids(&mut doc, options.heading_id_options());
            Some(index)
        } else if source.contains("</#") {
            let (index, ids) =
                crossref_index_and_heading_ids_for_document(&doc, options.heading_id_options());
            stamp_generated_heading_ids_from(&mut doc, ids);
            Some(index)
        } else {
            stamp_generated_heading_ids(&mut doc, options.heading_id_options());
            None
        };
        if matches!(mode, ParseMode::Html) {
            let _ = crate::render::collect_footnotes(&mut doc, false);
        }
        index
    } else {
        if source.contains("![") {
            promote_block_images(&mut doc.children, true, true);
        }
        None
    };
    for ext in &options.extensions {
        doc = ext.after_parse(doc);
    }
    if !matches!(mode, ParseMode::HtmlFacade) {
        coalesce_text_runs(&mut doc);
    }
    if !matches!(mode, ParseMode::HtmlFacade) {
        if !options.extensions.is_empty() {
            fill_crossref_hrefs(&mut doc, options.heading_id_options());
        } else {
            if let Some(index) = crossref_index.take() {
                fill_crossref_hrefs_from_index(&mut doc, index);
            } else if matches!(mode, ParseMode::Carve) && source.contains("</#") {
                fill_crossref_hrefs(&mut doc, options.heading_id_options());
            }
        }
    }
    if matches!(mode, ParseMode::HtmlFacade) && !options.extensions.is_empty() {
        crossref_index = Some(crossref_index_for_document(
            &doc,
            options.heading_id_options(),
        ));
    }
    Some((doc, crossref_index, used_event_route))
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_document_with_footnotes_options(
    source: &str,
    original_source: &str,
    source_len: usize,
    options: &Options<'_>,
    mode: ParseMode,
) -> Option<(Document, Option<CrossrefIndex>, bool)> {
    if NESTING_DEPTH.with(|depth| depth.get() >= MAX_NESTING_DEPTH) {
        return None;
    }
    let may_have_abbrevs = source.contains("*[");
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, options.positions);
    let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let (mapped, mut footnote_sources, mut footnote_def_pos, mut note_links) =
        extract_footnote_defs(
            body,
            body_start_line,
            options.positions,
            false,
            None,
            options,
        );
    if footnote_sources
        .values()
        .any(|note| note.source.contains("[^"))
    {
        extract_nested_footnote_defs(
            &mut footnote_sources,
            &mut footnote_def_pos,
            &mut note_links,
            options,
        );
    }
    if options.positions && mapped.col_map.iter().any(|column| *column != Some(0)) {
        return None;
    }
    let mut budget = probe_budget_for(mapped.source.len());
    let (body_source, mut link_defs) =
        extract_link_defs_guarded(&mapped.source, options, &mut budget);
    for def in link_defs.values_mut() {
        if let Some(line) = def.line {
            def.line = mapped
                .line_map
                .get(line)
                .and_then(|mapped| *mapped)
                .map(|authored| authored - 1)
                .or(def.line);
        }
    }
    for (label, def) in note_links {
        link_defs.entry(label).or_insert(def);
    }
    let mapped = remap_source(body_source, &mapped);
    let (parsed, link_defs, needs_late_resolution) = with_active_link_defs(link_defs, || {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = mapped.source.lines().collect();
        let footnote_defs = footnote_sources
            .into_iter()
            .map(|(label, note)| {
                let children = if note.source.trim_end_matches([' ', '\t', '\n']) == "{empty}" {
                    Vec::new()
                } else {
                    let lines: Vec<&str> = note.source.lines().collect();
                    if options.positions || options.source_lines {
                        parse_mapped_source(&note, options)
                    } else {
                        parse_blocks(&lines, options)
                            .unwrap_or_else(|| parse_mapped_source(&note, options))
                    }
                };
                (label, children)
            })
            .collect();
        let children = if options.positions || options.source_lines {
            if may_have_abbrevs {
                parse_mixed_blocks_mapped::<true, true>(
                    &lines,
                    options,
                    Some(&mapped.line_map),
                    true,
                )?
            } else {
                parse_mixed_blocks_mapped::<true, false>(
                    &lines,
                    options,
                    Some(&mapped.line_map),
                    true,
                )?
            }
        } else {
            parse_blocks(&lines, options).or_else(|| {
                if may_have_abbrevs {
                    parse_mixed_blocks_mapped::<true, true>(
                        &lines,
                        options,
                        Some(&mapped.line_map),
                        true,
                    )
                } else {
                    parse_mixed_blocks_mapped::<true, false>(
                        &lines,
                        options,
                        Some(&mapped.line_map),
                        true,
                    )
                }
            })?
        };
        Some((children, footnote_defs))
    });
    let (children, footnote_defs) = parsed?;
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
        children,
        source_len,
        ingest_payload_len: 0,
    };
    if options.positions {
        let starts = CodepointLineStarts::original(original_source);
        fill_offsets(&mut doc.children, &starts);
        let source_lines: Vec<&str> = original_source.lines().collect();
        narrow_to_last_placed_child(&mut doc.children, &source_lines);
        for blocks in doc.footnote_defs.values_mut() {
            fill_offsets(blocks, &starts);
            narrow_to_last_placed_child(blocks, &source_lines);
        }
        for pos in doc.footnote_def_pos.values_mut() {
            apply_offsets(pos, &starts);
        }
        for (label, pos) in &mut doc.footnote_def_pos {
            let last = doc
                .footnote_defs
                .get(label)
                .and_then(|blocks| blocks.iter().rev().find_map(crate::ast_json::block_pos))
                .cloned();
            end_at_last_placed_child(pos, last);
        }
    }
    if needs_late_resolution || source.contains("][") {
        let headings = heading_reference_index_for_document(&doc, options.heading_id_options());
        resolve_reference_links(&mut doc, &link_defs, &headings);
    }
    let original_line_starts = options
        .positions
        .then(|| CodepointLineStarts::original(original_source));
    if !matches!(mode, ParseMode::HtmlFacade) {
        append_link_reference_definitions(
            &mut doc,
            &link_defs,
            source,
            original_line_starts.as_ref(),
        );
    }
    let mut caption_titles = BTreeMap::new();
    if matches!(mode, ParseMode::Html | ParseMode::HtmlFacade) {
        if may_have_abbrevs {
            apply_abbreviations(&mut doc);
        }
        if source.contains("![") {
            promote_block_images(&mut doc.children, false, true);
            for blocks in doc.footnote_defs.values_mut() {
                promote_block_images(blocks, false, true);
            }
        }
        caption_titles = if source.contains('^') {
            number_crossref_captions(&mut doc)
        } else {
            BTreeMap::new()
        };
    }
    let mut crossref_index = if matches!(mode, ParseMode::HtmlFacade) {
        let (mut index, _) = heading_index(
            &doc.children,
            &doc.footnote_defs,
            options.heading_id_options(),
            source.contains("</#"),
            false,
        );
        index.extend_titles(caption_titles);
        stamp_generated_heading_ids(&mut doc, options.heading_id_options());
        Some(index)
    } else {
        if matches!(mode, ParseMode::Html) {
            stamp_generated_heading_ids(&mut doc, options.heading_id_options());
        }
        None
    };
    if matches!(mode, ParseMode::Html) {
        let _ = crate::render::collect_footnotes(&mut doc, false);
    }
    if matches!(mode, ParseMode::Carve) && source.contains("![") {
        promote_block_images(&mut doc.children, true, true);
        for blocks in doc.footnote_defs.values_mut() {
            promote_block_images(blocks, true, true);
        }
    }
    for ext in &options.extensions {
        doc = ext.after_parse(doc);
    }
    if !matches!(mode, ParseMode::HtmlFacade) {
        coalesce_text_runs(&mut doc);
    }
    if !matches!(mode, ParseMode::HtmlFacade)
        && (!options.extensions.is_empty() || source.contains("</#"))
    {
        fill_crossref_hrefs(&mut doc, options.heading_id_options());
    }
    if matches!(mode, ParseMode::HtmlFacade) && !options.extensions.is_empty() {
        crossref_index = Some(crossref_index_for_document(
            &doc,
            options.heading_id_options(),
        ));
    }
    Some((doc, crossref_index, true))
}

#[cfg(test)]
mod link_definition_tests {
    use super::*;

    #[test]
    fn nested_parse_installs_its_own_definition_placeholders() {
        let _outer = PlaceholderGuard::install(['\u{e007}', '\u{e008}']);
        let source = "```\n%%\u{e007}\n```\n\n[ref]: /target\n\nSee [ref][]\n";
        let (actual, admitted) = parse_document(source).unwrap();
        assert!(admitted);
        let expected = parse_with_options(source, &Options::default());
        assert_eq!(
            crate::ast_json::to_json(&actual),
            crate::ast_json::to_json(&expected)
        );
    }

    #[test]
    fn nested_parse_without_definitions_resets_placeholder_pair() {
        let _outer = PlaceholderGuard::install(['\u{e007}', '\u{e008}']);
        let source = "A paragraph\n\n%%\u{e007}\n";
        let (actual, admitted) = parse_document(source).unwrap();
        assert!(admitted);
        let expected = parse_with_options(source, &Options::default());
        assert_eq!(
            crate::ast_json::to_json(&actual),
            crate::ast_json::to_json(&expected)
        );
    }
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
mod marker_description_comment_owner_tests {
    use super::*;

    // Rule A moves these comments; both roots must read the walked text.
    #[test]
    fn event_root_owns_comments_the_walk_moves() {
        let body = "1. :: t\n   : para\n  %%%\n  %%%\ntail\n";
        for source in [
            body.to_owned(),
            "1. :: t\n   : para\n  %%%\n  c\n  %%%\n```\nx\n".to_owned(),
            format!("[ref]: /target\n\n{body}"),
            format!("*[abbr]: expansion\n\n{body}"),
        ] {
            let expected = {
                let _standard_root = StandardEventRootGuard::install();
                let _standard_nested = StandardNestedReaderGuard::install();
                parse_with_options(&source, &Options::default())
            };
            let (actual, used) = parse_document(&source).expect("event root");
            assert!(used, "{source:?}");
            assert_eq!(actual, expected, "{source:?}");
            for positions in [false, true] {
                let options = Options::default().with_positions(positions);
                let expected = {
                    let _standard_root = StandardEventRootGuard::install();
                    let _standard_nested = StandardNestedReaderGuard::install();
                    parse_with_options(&source, &options)
                };
                let (actual, _, used) =
                    parse_document_with_custom_options(&source, &options, ParseMode::Html)
                        .expect("event options root");
                assert!(used, "positions={positions} source={source:?}");
                assert_eq!(actual, expected, "positions={positions} source={source:?}");
            }
        }
    }

    #[test]
    fn comment_description_owner_matrix_matches_standard_reader() {
        for marker in ["1. ", "- "] {
            for comment in ["  %%%\n  %%%\n", "  %%%\n  note\n  %%%\n"] {
                for tail in ["tail\n", "```\nx\n", "2. next\n"] {
                    for prefix in ["", "[ref]: /target\n\n", "*[abbr]: expansion\n\n"] {
                        let source = format!("{prefix}{marker}:: t\n   : para\n{comment}{tail}");
                        for (mode_index, mode) in
                            [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve]
                                .into_iter()
                                .enumerate()
                        {
                            for positions in [false, true] {
                                for source_lines in [false, true] {
                                    let options = Options::default()
                                        .with_positions(positions)
                                        .with_source_lines(source_lines);
                                    let Some((candidate, _, admitted)) =
                                        parse_document_with_custom_options(&source, &options, mode)
                                    else {
                                        continue;
                                    };
                                    if !admitted {
                                        continue;
                                    }
                                    let expected = {
                                        let _standard_root = StandardEventRootGuard::install();
                                        let _standard_nested = StandardNestedReaderGuard::install();
                                        parse_with_options_mode_and_index(&source, &options, mode).0
                                    };
                                    assert_eq!(
                                        candidate, expected,
                                        "mode={mode_index} positions={positions} source_lines={source_lines} source={source:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
