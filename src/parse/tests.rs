use super::*;

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn event_owned_pipeline_matches_public_modes() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut admitted = 0;
    for entry in std::fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension() != Some(std::ffi::OsStr::new("crv")) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for positions in [false, true] {
            for source_lines in [false, true] {
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines);
                for mode in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve] {
                    let expected = {
                        let _standard_nested = StandardNestedReaderGuard::install();
                        parse_with_options_mode_and_index(&source, &options, mode).0
                    };
                    let (actual, _, used) =
                        event_pipeline::parse_document_with_mode(&source, &options, mode)
                            .unwrap_or_else(|| panic!("{} declined", path.display()));
                    assert!(
                        used,
                        "{}; positions={positions}; source_lines={source_lines}",
                        path.display()
                    );
                    assert_eq!(
                        actual,
                        expected,
                        "{}; positions={positions}; source_lines={source_lines}",
                        path.display()
                    );
                    admitted += 1;
                }
            }
        }
    }
    eprintln!("mode-aware event pipeline admitted {admitted}");
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn nested_event_cursor_keeps_mapped_eof_and_crlf_parity() {
    for source in [
        "- head\n  - child",
        "> quote\n> - item",
        "::: note\n- item\n:::",
        "[^note]: first\n    - child\n\nUse [^note].",
        "- head\r\n  ```\r\n  body\r\n  ```",
        "> first\r\n> second",
    ] {
        for positions in [false, true] {
            for source_lines in [false, true] {
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines);
                for mode in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve] {
                    let expected = {
                        let _standard_nested = StandardNestedReaderGuard::install();
                        parse_with_options_mode_and_index(source, &options, mode).0
                    };
                    let (actual, _, used) =
                        event_pipeline::parse_document_with_mode(source, &options, mode).unwrap();
                    assert!(used, "{source:?}");
                    assert_eq!(actual, expected, "{source:?}; {positions}; {source_lines}");
                }
            }
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn nested_event_cursor_keeps_unpositioned_authored_base_parity() {
    for body in [
        "::: note\n  : term\n:::",
        "::: note\n  > quoted\n:::",
        "- item\n%%%\ntext\n%%%",
    ] {
        let lines = body.lines().count();
        let mapped = MappedSource {
            source: body.to_string(),
            line_map: (1..=lines).map(Some).collect(),
            col_map: Vec::new(),
            reached: vec![true; lines],
            opaque_blank_lines: Vec::new(),
            discarded_outer_blank: false,
            outer_plus_after_blank: false,
            item_marker_at_start: false,
            marker_tail: None,
            in_quoted_body: false,
            sublists_carry_authored_base: true,
        };
        let expected = {
            let _standard_nested = StandardNestedReaderGuard::install();
            parse_mapped_source(&mapped, &Options::default())
        };
        let actual = parse_mapped_source(&mapped, &Options::default());
        assert_eq!(actual, expected, "{body:?}");
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn nested_event_cursor_fits_a_two_megabyte_stack_for_sixty_quotes() {
    let source = format!("{}text\n", "> ".repeat(60));
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let expected = {
                let _standard_nested = StandardNestedReaderGuard::install();
                parse_with_options(&source, &Options::default())
            };
            let (actual, _, used) = event_pipeline::parse_document_with_mode(
                &source,
                &Options::default(),
                ParseMode::Html,
            )
            .unwrap();
            assert!(used);
            assert_eq!(actual, expected);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn deeper_quote_chains_own_or_decline_on_a_small_stack() {
    let chain = format!("{}text\n", "> ".repeat(120));
    for (source, owned) in [
        (chain.clone(), true),
        (format!("\u{feff}{chain}"), true),
        (format!("prefix\r{chain}"), true),
        (format!("> head\n{chain}"), true),
        (format!("> head\nlazy\n{chain}"), true),
        // Item and definition bodies parse their quotes eagerly.
        (format!("- item\n  {chain}"), false),
        (format!(":: term\n: desc\n  {chain}"), false),
        // Colon bodies and footnotes keep their own readers.
        (format!("::: note\n{chain}:::\n"), false),
        (format!("[^n]: head\n    {chain}\nUse [^n].\n"), false),
    ] {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                let expected = {
                    let _standard_root = StandardEventRootGuard::install();
                    let _standard_nested = StandardNestedReaderGuard::install();
                    parse_with_options(&source, &Options::default())
                };
                let (actual, used) =
                    parse_experimental_event_blocks_with_options(&source, &Options::default());
                assert_eq!(used, owned, "{source:?}");
                assert_eq!(actual, expected, "{source:?}");
                let (base_actual, base_used) = parse_experimental_event_blocks(&source);
                assert_eq!(base_used, owned, "{source:?}");
                assert_eq!(base_actual, expected, "{source:?}");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(feature = "experimental-event-parser-options")]
mod deep_quote_worklist {
    use super::*;
    use crate::{BlockMatch, CarveExtension, InlineMatch, MatcherContext};

    const DEPTHS: [usize; 5] = [64, 120, 199, 200, 260];

    #[derive(Default)]
    pub(super) struct Log {
        pub(super) blocks: RefCell<Vec<String>>,
        pub(super) inlines: RefCell<Vec<(usize, String)>>,
    }

    impl CarveExtension for Log {
        fn name(&self) -> &'static str {
            "deep-quote-order"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            self.blocks.borrow_mut().push(lines.get(start)?.to_string());
            None
        }

        fn match_inline(
            &self,
            text: &str,
            pos: usize,
            _: &MatcherContext<'_>,
        ) -> Option<InlineMatch> {
            let prefix = text.get(pos..).unwrap_or("").chars().take(8).collect();
            self.inlines.borrow_mut().push((pos, prefix));
            None
        }
    }

    fn quote(depth: usize) -> String {
        "> ".repeat(depth)
    }

    /// A marker run with no content, which `strip_blockquote_prefix` reads
    /// to an empty body.
    fn bare(depth: usize) -> String {
        format!("{}>", "> ".repeat(depth - 1))
    }

    fn staircase(depth: usize) -> String {
        (1..=depth)
            .rev()
            .map(|level| format!("{}line\n{}\n", quote(level), bare(level)))
            .collect()
    }

    /// `levels` lines, each one level shallower than the last and the first
    /// `top - 1` deep, after a quote `depth` deep. As a caption, or a dangling
    /// attribute when the parse order is observed, each line keeps one quote
    /// eager, each inside the one before. With `top == levels` the eager
    /// quotes are the outermost ones, above the whole deep chain.
    fn eager_ladder(depth: usize, top: usize, levels: usize, line: &str) -> String {
        let mut source = format!("{}text\n", quote(depth));
        for step in 1..=levels {
            source.push_str(&format!("{}{line}\n", quote(top - step)));
        }
        source
    }

    /// `(label, source, owned_when_observed, owned_unobserved)` for eager
    /// ladders at the budget and one past it, at both ends of the chain.
    fn eager_ladders(depth: usize) -> Vec<(String, String, bool, bool)> {
        let budget = event_pipeline::DEEP_QUOTE_EAGER_BUDGET;
        let mut ladders = Vec::new();
        for (line, captioned) in [("^ caption", true), ("{.k}", false)] {
            for levels in [budget, budget + 1] {
                // "under direct": the eager quotes sit right below the
                // levels the direct quote helpers parse, the deepest stacking
                // of the two that admission allows.
                for (end, top) in [
                    ("inner", depth),
                    ("outer", levels),
                    (
                        "under direct",
                        event_pipeline::DIRECT_QUOTE_DEPTH_LIMIT + levels,
                    ),
                ] {
                    let within = levels <= budget;
                    ladders.push((
                        format!("{line} ladder; levels={levels}; {end}"),
                        eager_ladder(depth, top, levels, line),
                        within,
                        within || !captioned,
                    ));
                }
            }
        }
        ladders
    }

    /// `(label, source, owned_when_observed)`. Every shape is owned when
    /// nothing observes the parse order.
    fn shapes(depth: usize) -> Vec<(String, String, bool)> {
        let q = quote(depth);
        let fixed = [
            ("plain", format!("{q}text\n"), true),
            ("unterminated", format!("{q}text"), true),
            ("lazy", format!("{q}text\nlazy line\nmore lazy\n"), true),
            ("root caption", format!("{q}text\n\n^ caption\n"), true),
            (
                "inner caption",
                format!("{q}text\n{}^ inner\n", quote(depth - 1)),
                true,
            ),
            ("emphasis leaf", format!("{q}*text* and _more_\n"), true),
            ("list leaf", format!("{q}- one\n{q}- two\n"), true),
            ("fence leaf", format!("{q}```\n{q}code\n{q}```\n"), true),
            (
                "attributes",
                format!("{q}{{.k}}\n{q}text\n{}\n{q}{{.dangling}}\n", bare(depth)),
                true,
            ),
            ("heading tail", format!("{q}text\n\n# after\n"), true),
            (
                "siblings",
                format!("{q}deep\n{}\n{}tail\n", bare(2), quote(2)),
                true,
            ),
            // One shallower line per level keeps every level eager when the
            // parse order is observed, so that shape declines.
            ("staircase", staircase(depth), false),
        ];
        let fixed = fixed
            .into_iter()
            .map(|(label, source, owned)| (label.to_string(), source, owned));
        // A caption ladder past the budget declines even unobserved.
        let ladders = eager_ladders(depth)
            .into_iter()
            .filter(|ladder| ladder.3)
            .map(|(label, source, owned, _)| (label, source, owned));
        fixed.chain(ladders).collect()
    }

    /// Item and definition bodies that host the deep chain. Their quotes
    /// parse eagerly, outside the deferral and the eager budget, so admission
    /// declines them before any callback runs.
    fn item_hosted(depth: usize) -> Vec<(&'static str, String)> {
        let q = quote(depth);
        // `>   ` puts each deeper level at the content column of the item
        // above it, so the chain alternates quote, item, quote, item.
        let alternation = (0..depth)
            .map(|level| {
                let indent = ">   ".repeat(level);
                format!("{indent}> - a\n{indent}>\n")
            })
            .collect();
        vec![
            ("quote/list alternation", alternation),
            ("list hosts chain", format!("- item\n  {q}text\n")),
            ("ordered hosts chain", format!("> 1. item\n>    {q}text\n")),
            (
                "definition hosts chain",
                format!(":: term\n: desc\n  {q}text\n"),
            ),
        ]
    }

    /// With `collect`, the diagnostics collectors are installed, which makes
    /// the parse order observable just as an extension does.
    pub(super) fn check(
        label: &str,
        source: &str,
        options: &Options<'_>,
        mode: ParseMode,
        collect: bool,
        owned: bool,
    ) {
        let run = |parse: &dyn Fn() -> Option<Document>| {
            if collect {
                let ((document, unattached), events) =
                    collecting_lint_events(|| collecting_unattached_block_attrs(parse));
                (document, unattached, events)
            } else {
                (parse(), Vec::new(), Vec::new())
            }
        };
        let (expected, expected_unattached, expected_events) = run(&|| {
            let _standard_root = StandardEventRootGuard::install();
            let _standard_nested = StandardNestedReaderGuard::install();
            Some(parse_with_options_mode_and_index(source, options, mode).0)
        });
        let used = std::cell::Cell::new(false);
        let (actual, actual_unattached, actual_events) = run(&|| {
            let (document, _, admitted) =
                event_pipeline::parse_document_with_mode(source, options, mode)?;
            used.set(admitted);
            Some(document)
        });
        let Some(actual) = actual else {
            assert!(!owned, "{label}: declined");
            return;
        };
        assert!(owned, "{label}: expected a decline");
        assert!(used.get(), "{label}");
        assert_eq!(actual, expected.unwrap(), "{label}: ast");
        assert_eq!(
            actual_unattached, expected_unattached,
            "{label}: unattached attributes"
        );
        assert_eq!(actual_events, expected_events, "{label}: lint events");
    }

    #[test]
    fn deep_quote_shapes_match_standard_across_modes_and_options() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for depth in DEPTHS {
                    for (shape, source, owned_observed) in shapes(depth) {
                        for positions in [false, true] {
                            for source_lines in [false, true] {
                                let options = Options::default()
                                    .with_positions(positions)
                                    .with_source_lines(source_lines);
                                for mode in
                                    [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve]
                                {
                                    for collect in [false, true] {
                                        let label = format!(
                                            "depth={depth}; shape={shape}; positions={positions}; \
                                             source_lines={source_lines}; mode={}; \
                                             collect={collect}",
                                            mode as u8
                                        );
                                        let owned = !collect || owned_observed;
                                        check(&label, &source, &options, mode, collect, owned);
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn deep_quote_shapes_keep_extension_callback_order() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for depth in DEPTHS {
                    for (shape, source, owned) in shapes(depth) {
                        for positions in [false, true] {
                            for mode in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve] {
                                let label = format!(
                                    "depth={depth}; shape={shape}; positions={positions}; mode={}",
                                    mode as u8
                                );
                                let baseline = Log::default();
                                let candidate = Log::default();
                                let baseline_options = Options::default()
                                    .with_positions(positions)
                                    .with_extension(&baseline);
                                let candidate_options = Options::default()
                                    .with_positions(positions)
                                    .with_extension(&candidate);
                                let expected = {
                                    let _standard_root = StandardEventRootGuard::install();
                                    let _standard_nested = StandardNestedReaderGuard::install();
                                    parse_with_options_mode_and_index(
                                        &source,
                                        &baseline_options,
                                        mode,
                                    )
                                    .0
                                };
                                let event = event_pipeline::parse_document_with_mode(
                                    &source,
                                    &candidate_options,
                                    mode,
                                );
                                let Some((actual, _, used)) = event else {
                                    assert!(!owned, "{label}: declined");
                                    assert!(candidate.blocks.borrow().is_empty(), "{label}");
                                    assert!(candidate.inlines.borrow().is_empty(), "{label}");
                                    continue;
                                };
                                assert!(owned, "{label}: expected a decline");
                                assert!(used, "{label}");
                                assert_eq!(actual, expected, "{label}: ast");
                                assert_eq!(
                                    *candidate.blocks.borrow(),
                                    *baseline.blocks.borrow(),
                                    "{label}: block callbacks"
                                );
                                assert_eq!(
                                    *candidate.inlines.borrow(),
                                    *baseline.inlines.borrow(),
                                    "{label}: inline callbacks"
                                );
                            }
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn deep_quote_item_bodies_decline_before_callbacks() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for depth in DEPTHS {
                    let q = quote(depth);
                    // Lists that host no deeper line stay owned.
                    let near_misses = [
                        ("list leaf", format!("{q}- one\n{q}- two\n")),
                        ("list after chain", format!("{q}text\n> - after\n")),
                    ];
                    let declined = item_hosted(depth).into_iter().map(|(label, source)| {
                        assert!(
                            !event_pipeline::deep_quote_chain_owned(&source, &Options::default()),
                            "depth={depth}; shape={label}: admitted"
                        );
                        (label, source, false)
                    });
                    let owned = near_misses
                        .into_iter()
                        .map(|(label, source)| (label, source, true));
                    for (shape, source, owned) in declined.chain(owned) {
                        for positions in [false, true] {
                            let options = Options::default().with_positions(positions);
                            for mode in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve] {
                                for collect in [false, true] {
                                    let label = format!(
                                        "depth={depth}; shape={shape}; positions={positions}; \
                                         mode={}; collect={collect}",
                                        mode as u8
                                    );
                                    check(&label, &source, &options, mode, collect, owned);
                                }
                                let log = Log::default();
                                let observed = Options::default()
                                    .with_positions(positions)
                                    .with_extension(&log);
                                let event = event_pipeline::parse_document_with_mode(
                                    &source, &observed, mode,
                                );
                                assert_eq!(event.is_some(), owned, "depth={depth}; shape={shape}");
                                if !owned {
                                    assert!(log.blocks.borrow().is_empty(), "{shape}");
                                    assert!(log.inlines.borrow().is_empty(), "{shape}");
                                }
                            }
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// The event route must decline item-hosted chains on the release stack.
    /// Only the event route runs here: the standard reader is not safe on
    /// this stack at these depths, and admission is asserted first so a
    /// regression fails as an assertion rather than an overflow.
    #[test]
    fn deep_quote_item_bodies_decline_on_the_release_stack() {
        let stack = if cfg!(debug_assertions) {
            4 * 1024 * 1024
        } else {
            512 * 1024
        };
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(|| {
                for depth in [200, 260] {
                    for (shape, source) in item_hosted(depth) {
                        for positions in [false, true] {
                            for observed in [false, true] {
                                let label = format!(
                                    "depth={depth}; shape={shape}; positions={positions}; \
                                     observed={observed}"
                                );
                                let log = Log::default();
                                let mut options = Options::default().with_positions(positions);
                                if observed {
                                    options = options.with_extension(&log);
                                }
                                assert!(
                                    !event_pipeline::deep_quote_chain_owned(&source, &options),
                                    "{label}: admitted"
                                );
                                let event = event_pipeline::parse_document_with_mode(
                                    &source,
                                    &options,
                                    ParseMode::Html,
                                );
                                assert!(event.is_none(), "{label}: owned");
                                assert!(log.blocks.borrow().is_empty(), "{label}");
                                assert!(log.inlines.borrow().is_empty(), "{label}");
                            }
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// Each eager quote holds an event frame while its body and worklist run.
    /// Admission keeps that chain short, so no quoted body runs out of frames
    /// and drops the levels under it onto the recursive standard reader.
    #[test]
    fn deep_quote_eager_levels_keep_event_frames_on_the_release_stack() {
        let stack = if cfg!(debug_assertions) {
            4 * 1024 * 1024
        } else {
            512 * 1024
        };
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(|| {
                for depth in [200, 260] {
                    let mut cases = eager_ladders(depth);
                    cases.push(("staircase".into(), staircase(depth), false, true));
                    for (shape, source, owned_observed, owned_unobserved) in cases {
                        for positions in [false, true] {
                            for observer in ["none", "extension", "collectors"] {
                                let label = format!(
                                    "depth={depth}; shape={shape}; positions={positions}; \
                                     observer={observer}"
                                );
                                let log = Log::default();
                                let mut options = Options::default().with_positions(positions);
                                if observer == "extension" {
                                    options = options.with_extension(&log);
                                }
                                let parse = || {
                                    event_pipeline::parse_document_with_mode(
                                        &source,
                                        &options,
                                        ParseMode::Html,
                                    )
                                };
                                event_pipeline::PEAK_EVENT_FRAMES.with(|peak| peak.set(0));
                                let parsed = if observer == "collectors" {
                                    let ((parsed, _), _) = collecting_lint_events(|| {
                                        collecting_unattached_block_attrs(parse)
                                    });
                                    parsed
                                } else {
                                    parse()
                                };
                                let owned = if observer == "none" {
                                    owned_unobserved
                                } else {
                                    owned_observed
                                };
                                let Some((document, _, used)) = parsed else {
                                    assert!(!owned, "{label}: declined");
                                    assert!(log.blocks.borrow().is_empty(), "{label}");
                                    assert!(log.inlines.borrow().is_empty(), "{label}");
                                    continue;
                                };
                                assert!(owned, "{label}: expected a decline");
                                assert!(used, "{label}");
                                let peak = event_pipeline::PEAK_EVENT_FRAMES.with(Cell::get);
                                assert!(
                                    peak < event_pipeline::EVENT_FRAME_LIMIT,
                                    "{label}: {peak} event frames"
                                );
                                std::mem::forget(document);
                            }
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// The event route alone, on the stack a release embedder may give it.
    /// Debug frames are several times larger, so a debug run only checks the
    /// route on a stack no larger than the release floor times eight.
    #[test]
    fn deep_quote_event_route_fits_the_release_stack_floor() {
        let stack = if cfg!(debug_assertions) {
            4 * 1024 * 1024
        } else {
            512 * 1024
        };
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(|| {
                for depth in [199, 200, 260] {
                    for (shape, source, _) in shapes(depth) {
                        for positions in [false, true] {
                            let options = Options::default().with_positions(positions);
                            let (document, _, used) = event_pipeline::parse_document_with_mode(
                                &source,
                                &options,
                                ParseMode::Html,
                            )
                            .unwrap_or_else(|| panic!("depth={depth}; shape={shape}: declined"));
                            assert!(used, "depth={depth}; shape={shape}");
                            // Drop glue recurses once per level; this test is
                            // about the parser's stack, not teardown.
                            std::mem::forget(document);
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn event_owned_modes_preserve_extension_callbacks() {
    use crate::{BlockMatch, CarveExtension, InlineMatch, MatcherContext};
    event_pipeline::NESTED_EXTENSION_EVENT_PARSES.with(|count| count.set(0));

    #[derive(Default)]
    struct Hook {
        blocks: RefCell<Vec<String>>,
        inlines: RefCell<Vec<(usize, String)>>,
        after: Cell<usize>,
    }

    impl CarveExtension for Hook {
        fn name(&self) -> &'static str {
            "event-mode-parity"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            let line = *lines.get(start)?;
            self.blocks.borrow_mut().push(line.to_string());
            None
        }

        fn match_inline(
            &self,
            text: &str,
            pos: usize,
            _: &MatcherContext<'_>,
        ) -> Option<InlineMatch> {
            let prefix = text.get(pos..).unwrap_or("").chars().take(8).collect();
            self.inlines.borrow_mut().push((pos, prefix));
            None
        }

        fn after_parse(&self, doc: Document) -> Document {
            self.after.set(self.after.get() + 1);
            doc
        }
    }

    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    for entry in std::fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension() != Some(std::ffi::OsStr::new("crv")) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for mode in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve] {
            for positions in [false, true] {
                let baseline = Hook::default();
                let baseline_options = Options::default()
                    .with_positions(positions)
                    .with_extension(&baseline);
                let expected = {
                    let _standard_root = StandardEventRootGuard::install();
                    let _standard_nested = StandardNestedReaderGuard::install();
                    parse_with_options_mode_and_index(&source, &baseline_options, mode).0
                };
                let candidate = Hook::default();
                let candidate_options = Options::default()
                    .with_positions(positions)
                    .with_extension(&candidate);
                let (actual, _, used) =
                    event_pipeline::parse_document_with_mode(&source, &candidate_options, mode)
                        .unwrap_or_else(|| panic!("{} declined", path.display()));
                assert!(used, "{}", path.display());
                assert_eq!(actual, expected, "{}", path.display());
                assert_eq!(
                    *candidate.blocks.borrow(),
                    *baseline.blocks.borrow(),
                    "{}",
                    path.display()
                );
                assert_eq!(
                    *candidate.inlines.borrow(),
                    *baseline.inlines.borrow(),
                    "{}",
                    path.display()
                );
                assert_eq!(
                    candidate.after.get(),
                    baseline.after.get(),
                    "{}",
                    path.display()
                );
            }
        }
    }
    event_pipeline::NESTED_EXTENSION_EVENT_PARSES.with(|count| assert!(count.get() > 0));
}

#[cfg(feature = "experimental-event-parser-options")]
mod event_ladder_tail {
    use super::*;
    use crate::{BlockMatch, CarveExtension, InlineMatch, MatcherContext};

    /// Logs every callback and claims `@@@`, so a tail that reaches the
    /// extension at a different point shows up as a different log.
    #[derive(Default)]
    pub(super) struct LadderHook {
        blocks: RefCell<Vec<String>>,
        inlines: RefCell<Vec<(usize, String)>>,
        after: Cell<usize>,
    }

    impl CarveExtension for LadderHook {
        fn name(&self) -> &'static str {
            "event-ladder-tail"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            let line = *lines.get(start)?;
            self.blocks.borrow_mut().push(line.to_string());
            (line == "@@@").then(|| BlockMatch {
                node: BlockNode::Paragraph(Paragraph {
                    children: vec![InlineNode::text("custom".to_string())],
                    ..Default::default()
                }),
                lines_consumed: 1,
            })
        }

        fn match_inline(
            &self,
            text: &str,
            pos: usize,
            _: &MatcherContext<'_>,
        ) -> Option<InlineMatch> {
            let prefix = text.get(pos..).unwrap_or("").chars().take(8).collect();
            self.inlines.borrow_mut().push((pos, prefix));
            None
        }

        fn after_parse(&self, doc: Document) -> Document {
            self.after.set(self.after.get() + 1);
            doc
        }
    }

    pub(super) fn assert_same_callbacks(actual: &LadderHook, expected: &LadderHook, what: &str) {
        assert_eq!(*actual.blocks.borrow(), *expected.blocks.borrow(), "{what}");
        assert_eq!(
            *actual.inlines.borrow(),
            *expected.inlines.borrow(),
            "{what}"
        );
        assert_eq!(actual.after.get(), expected.after.get(), "{what}");
    }

    pub(super) fn hooked(hook: &LadderHook, positions: bool, source_lines: bool) -> Options<'_> {
        Options::default()
            .with_positions(positions)
            .with_source_lines(source_lines)
            .with_extension(hook)
    }

    fn ladder_tail_event_parses() -> usize {
        event_pipeline::LADDER_TAIL_EVENT_PARSES.with(Cell::get)
    }

    fn unpositioned_body(body: &str) -> MappedSource {
        let lines = body.lines().count();
        MappedSource {
            source: body.to_string(),
            line_map: (1..=lines).map(Some).collect(),
            col_map: Vec::new(),
            reached: vec![true; lines],
            opaque_blank_lines: Vec::new(),
            discarded_outer_blank: false,
            outer_plus_after_blank: false,
            item_marker_at_start: false,
            marker_tail: None,
            in_quoted_body: false,
            sublists_carry_authored_base: false,
        }
    }

    /// A nested EOF-closed ladder never offers its opener lines to an
    /// extension and resolves its whole tail before the caller resumes. The
    /// event cursor owns the tail now, and must keep both properties.
    #[test]
    fn nested_ladder_tail_is_event_owned_with_standard_callback_order() {
        for body in [
            "::: note\n::: box\ntext",
            "::: note\n@@@\ntext",
            "::: note\n{.c}\n@@@\npara",
            "::: note\n:: term\n: def",
            "::: box\n:: sub",
            "::: note\n::: box\n- a\n- ::: inner\n  ::: deeper\n  @@@\n  tail\n- b",
            "::: note\n# H\n> q\n```\ncode\n```\ntext",
            "::: note\n::: box\n- a\n\n  - b\n\n    c\nafter",
            "::: figure\n::: note\n![a](b)\n^ caption",
            "::: note\n\n\n",
        ] {
            let mapped = unpositioned_body(body);
            let baseline = LadderHook::default();
            let expected = {
                let _standard_nested = StandardNestedReaderGuard::install();
                let before = ladder_tail_event_parses();
                let blocks =
                    parse_mapped_source(&mapped, &Options::default().with_extension(&baseline));
                assert_eq!(ladder_tail_event_parses(), before, "{body:?}");
                blocks
            };
            let candidate = LadderHook::default();
            let before = ladder_tail_event_parses();
            let actual =
                parse_mapped_source(&mapped, &Options::default().with_extension(&candidate));
            let openers: Vec<&str> = body
                .lines()
                .take_while(|line| detect_container_open(line).is_some())
                .collect();
            if body
                .lines()
                .skip(openers.len())
                .any(|line| !is_blank_line(line))
            {
                assert!(
                    ladder_tail_event_parses() > before,
                    "{body:?} tail not event-owned"
                );
            }
            assert_eq!(actual, expected, "{body:?}");
            assert_same_callbacks(&candidate, &baseline, body);
            assert!(
                candidate
                    .blocks
                    .borrow()
                    .iter()
                    .all(|line| !openers.contains(&line.as_str())),
                "{body:?}: a ladder opener reached the extension"
            );
        }
    }

    /// A tail past the event frame limit, or deep on the native stack, uses
    /// the standard reader before any extension callback fires; a deep tail
    /// in a worklist body stays event owned. All keep the standard AST.
    #[test]
    fn ladder_tail_respects_frame_gate() {
        let limit = event_pipeline::EVENT_FRAME_LIMIT;
        for (body, depth, base, frames, event_owned) in [
            ("::: note\n@@@\ntext\n{.dangling}", 0, 0, 0, true),
            ("::: note\n{.c}\n@@@\ntext\n{.dangling}", 150, 0, 0, false),
            ("::: note\n{.c}\n@@@\ntext\n{.dangling}", 150, 150, 0, true),
            ("::: note\n{.c}\n@@@\ntext\n{.dangling}", 0, 0, limit, false),
        ] {
            let _depth = DepthScope::set(depth);
            let _base = WorklistBase::set(base, 0);
            let _frames = event_pipeline::hold_event_frames(frames);
            let mapped = unpositioned_body(body);
            let baseline = LadderHook::default();
            let expected = {
                let _standard_nested = StandardNestedReaderGuard::install();
                let options = Options::default().with_extension(&baseline);
                parse_mapped_source(&mapped, &options)
            };
            let candidate = LadderHook::default();
            let options = Options::default().with_extension(&candidate);
            let before = ladder_tail_event_parses();
            let actual = parse_mapped_source(&mapped, &options);
            assert_eq!(ladder_tail_event_parses() > before, event_owned, "{body:?}");
            assert_eq!(actual, expected, "{body:?}");
            assert_same_callbacks(&candidate, &baseline, body);
        }
        assert_eq!(WORKLIST_BASE.with(Cell::get), 0);
    }

    /// Each ladder tail opens a list whose item body is another ladder, so the
    /// eager tail parse recurses once per level.
    #[test]
    fn alternating_list_ladders_fit_a_two_megabyte_stack() {
        let source: String = (0..60)
            .map(|level| format!("{}- ::: n{level}\n", "  ".repeat(level)))
            .chain(std::iter::once(format!("{}@@@\n", "  ".repeat(60))))
            .collect();
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                let baseline = LadderHook::default();
                let expected = {
                    let _standard_root = StandardEventRootGuard::install();
                    let _standard_nested = StandardNestedReaderGuard::install();
                    let options = hooked(&baseline, false, false);
                    parse_with_options_mode_and_index(&source, &options, ParseMode::Html).0
                };
                let candidate = LadderHook::default();
                let options = hooked(&candidate, false, false);
                let before = ladder_tail_event_parses();
                let actual =
                    parse_with_options_mode_and_index(&source, &options, ParseMode::Html).0;
                assert!(ladder_tail_event_parses() > before, "tails not event-owned");
                assert_eq!(actual, expected);
                assert_same_callbacks(&candidate, &baseline, "alternating ladders");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// Corpus 620 and 692 regressed when the event reader took ladder bodies
    /// ahead of the ladder; every public mode must keep the standard order.
    #[test]
    fn ladder_documents_keep_callback_order_in_every_mode() {
        for source in [
            "- a\n  ::: box\n  :: sub\n: def\n",
            ">  ::: |\n :::\n",
            "- - ::: |\n  :::\n  para\ntail\n",
            "- ::: note\n  ::: box\n  @@@\n  text\n- tail\n",
            "> ::: note\n> @@@\n> text\n",
            "- ::: note\n  - ::: box\n    @@@\n  tail\n",
            "1. ::: note\n   :: t\n   : d\n\nafter\n",
            "- ::: note\n  ::: box\n  text",
        ] {
            for (label, mode) in [
                ("html", ParseMode::Html),
                ("facade", ParseMode::HtmlFacade),
                ("carve", ParseMode::Carve),
            ] {
                for positions in [false, true] {
                    for source_lines in [false, true] {
                        let what = format!("{source:?} {label} {positions} {source_lines}");
                        let baseline = LadderHook::default();
                        let expected = {
                            let _standard_root = StandardEventRootGuard::install();
                            let _standard_nested = StandardNestedReaderGuard::install();
                            let options = hooked(&baseline, positions, source_lines);
                            parse_with_options_mode_and_index(source, &options, mode).0
                        };
                        let candidate = LadderHook::default();
                        let options = hooked(&candidate, positions, source_lines);
                        let actual = parse_with_options_mode_and_index(source, &options, mode).0;
                        assert_eq!(actual, expected, "{what}");
                        assert_same_callbacks(&candidate, &baseline, &what);
                    }
                }
            }
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
mod event_colon_body {
    use super::event_ladder_tail::{assert_same_callbacks, hooked, LadderHook};
    use super::*;

    fn colon_body_event_parses() -> usize {
        event_pipeline::COLON_BODY_EVENT_PARSES.with(Cell::get)
    }

    type Observed = ((Document, Vec<Pos>), Vec<LintEvent>);

    fn observe(
        source: &str,
        mode: ParseMode,
        positions: bool,
        source_lines: bool,
        hook: &LadderHook,
    ) -> Observed {
        let options = hooked(hook, positions, source_lines);
        collecting_lint_events(|| {
            collecting_unattached_block_attrs(|| {
                parse_with_options_mode_and_index(source, &options, mode).0
            })
        })
    }

    /// Compare one source against the fully standard reader in every public
    /// mode, with the root on the event reader and on the standard one.
    /// `owned` says whether some colon body must reach the event cursor.
    fn assert_colon_parity(source: &str, owned: bool) {
        for (label, mode) in [
            ("html", ParseMode::Html),
            ("facade", ParseMode::HtmlFacade),
            ("carve", ParseMode::Carve),
        ] {
            for positions in [false, true] {
                for source_lines in [false, true] {
                    let baseline = LadderHook::default();
                    let expected = {
                        let _standard_root = StandardEventRootGuard::install();
                        let _standard_nested = StandardNestedReaderGuard::install();
                        let before = colon_body_event_parses();
                        let observed = observe(source, mode, positions, source_lines, &baseline);
                        assert_eq!(colon_body_event_parses(), before, "{source:?} baseline");
                        observed
                    };
                    for standard_root in [true, false] {
                        let what = format!(
                            "{source:?} {label} {positions} {source_lines} {standard_root}"
                        );
                        let candidate = LadderHook::default();
                        let before = colon_body_event_parses();
                        let actual = {
                            let _standard_root =
                                standard_root.then(StandardEventRootGuard::install);
                            observe(source, mode, positions, source_lines, &candidate)
                        };
                        assert_eq!(colon_body_event_parses() > before, owned, "{what} owner");
                        assert_eq!(actual.0 .0, expected.0 .0, "{what} ast");
                        assert_eq!(actual.0 .1, expected.0 .1, "{what} unattached attrs");
                        assert_eq!(actual.1, expected.1, "{what} lint events");
                        assert_same_callbacks(&candidate, &baseline, &what);
                    }
                }
            }
        }
    }

    #[test]
    fn closed_colon_bodies_are_event_owned_in_every_mode() {
        for source in [
            "::: note\npara\n:::\n",
            "::: note\n# H\n\n***\n\ntext\n:::\n",
            "::: note\n{.c #i}\npara\n\n{k=v}\n# H\n:::\nafter\n",
            "::: note\n{.a}\n%% c\n{.b}\npara\n:::\n",
            "::: note\npara\n\n{.dangling}\n:::\n",
            "::: note\n%%%\nhidden\n%%%\ntext\n:::\n",
            "::: note\n```rust \"Title\"\nfn main() {}\n```\n^ cap\n:::\n",
            "::: note\n{table}\n``` csv\na,b\n1,2\n```\n:::\n",
            "::: note\n| a | b |\n| 1 | 2 |\n:::\n",
            "::: note\n:: term\n: def\n\n:: other\n: more\n:::\n",
            "::: note\n> q\n> more\n\nafter\n:::\n",
            "::: note\n![a](a.png)\n^ caption\n\n![b](b.png)\n\ntext\n:::\n",
            "::: note\n$$`x^2`\n^ eq\n:::\n",
            "::: note\n::: |\nline one\n  line two\n:::\n::: \\\nhard\nbreaks\n:::\n::: >\nquoted\n:::\n:::\n",
            "::: warning Title *here*\nbody\n:::\n",
        ] {
            assert_colon_parity(source, true);
        }
    }

    #[test]
    fn nested_colon_and_list_bodies_keep_the_standard_ast() {
        for source in [
            "::: note\n::: box\ninner\n:::\nouter\n:::\n",
            "::: a\n::: b\n::: c\ndeep *text*\n:::\n:::\n:::\n",
            "::: note\n- a\n- b\n\n  c\n- d\n:::\n",
            "::: note\n- a\n  - b\n    - c\n\n      para\n1. one\n2. two\n:::\n",
            "::: note\n- [ ] task\n- [x] done\n:::\n",
            "::: note\n- a\n\n  ::: box\n  inner\n  :::\n- b\n:::\n",
            "- item\n\n  ::: note\n  - x\n  - y\n  :::\n",
            "- item\n\n  ::: note\n  > quoted\n  :: term\n  : def\n  :::\n",
            "- item\n\n  ::: note\n  text\n    > q\n    : def\n  :::\n",
            "- ::: note\n  text\n  :::\n- tail\n",
            "- ::: note\n    > q\n  :::\n- tail\n",
            "> ::: note\n> quoted body\n> :::\n",
            "::: figure\n![a](a.png)\n^ one\n\n![b](b.png)\n^ two\n:::\n^ group\n",
            "::: figure\n::: figure\n![a](a.png)\n:::\n:::\n",
            "[^n]: note\n\n    ::: box\n    body\n    :::\n\ntext[^n]\n",
            "::: note\n{.outer}\n::: box\n{.inner}\npara\n:::\n:::\n",
        ] {
            assert_colon_parity(source, true);
        }
    }

    /// `@@@` is claimed by the hook, so a body that offers a line in a
    /// different order, twice, or not at all shows up in the log.
    #[test]
    fn extension_callbacks_keep_order_through_event_colon_bodies() {
        for source in [
            "::: note\n@@@\ntext with *em*\n:::\n",
            "::: note\n{.c}\n@@@\n:::\n",
            "::: note\n@@@\n::: box\n@@@\nx\n:::\n@@@\n:::\n",
            "::: note\n- a\n- @@@\n\n  @@@\n:::\n",
            "- a\n\n  ::: note\n  @@@\n  > @@@\n  :::\n",
            "::: note\n> @@@\n\n:: @@@\n: @@@\n:::\n",
            "::: note\n@@@\n:::\n\n@@@\n\n::: box\n@@@\n:::\n",
        ] {
            assert_colon_parity(source, true);
        }
    }

    /// A `+` line reaches the extension hook through `parse_block` on both
    /// routes, so the callback log keeps the standard order.
    #[test]
    fn plus_lines_in_colon_bodies_keep_callback_order() {
        for source in [
            "::: note\n- a\n+\n  @@@\n:::\n",
            "::: note\n@@@ +\nmore\n:::\n",
            "::: note\n- a\n+\n@@@\n\n+\n@@@\n:::\n",
        ] {
            assert_colon_parity(source, true);
        }
    }

    /// Colon bodies ask `+` for its decision through the colon geometry, so
    /// accepted, refused and orphan markers match the standard reader.
    #[test]
    fn continuation_markers_in_colon_bodies_are_event_owned() {
        for source in [
            "::: note\n- a\n+\n```\ncode\n```\n:::\n",
            "::: note\npara\n\n+\n:::\n",
            "::: note\n+\npara\n:::\n",
            "::: note\n- a\n\n+\nb\n:::\n",
            "::: note\n- a\n +\nb\n:::\n",
            "::: note\n- a\n+\n- b\n:::\n",
            "::: note\n- a\n+\n:::\n",
            "::: note\n1. one\n+\n> q\n:::\n",
            "::: note\n:: t\n: d\n+\n  more\n:::\n",
            "::: note\n> a\n+\nb\n:::\n",
            "::: a\n::: b\n- x\n+\ny\n:::\n:::\n",
            "::: note\n- a\n+\n::: box\ninner\n:::\n:::\n",
            "- ::: note\n  - a\n  +\n  b\n  :::\n",
            "> ::: note\n> - a\n> +\n> b\n> :::\n",
            "::: note\n{.c}\n- a\n+\n{.d}\npara\n:::\n",
            "::: note\n- a\n+\n{.dangling}\n:::\n",
            "::: note\ntext +\nmore\n:::\n",
            "::: note\r\n- a\r\n+\r\nmore\r\n:::\r\n",
        ] {
            assert_colon_parity(source, true);
        }
    }

    /// Bodies deep in the worklist stay on the event cursor; past the global
    /// nesting cap both readers degrade the same way.
    #[test]
    fn colon_depth_caps_match_the_standard_reader() {
        let nest = |levels: usize| -> String {
            let mut out = String::new();
            for level in 0..levels {
                out.push_str(&format!("::: n{level}\n"));
            }
            out.push_str("@@@\ndeep\n");
            for _ in 0..levels {
                out.push_str(":::\n");
            }
            out
        };
        for levels in [event_pipeline::EVENT_FRAME_LIMIT + 2, MAX_NESTING_DEPTH + 5] {
            let source = nest(levels);
            std::thread::Builder::new()
                .stack_size(2 * 1024 * 1024)
                .spawn(move || assert_colon_parity(&source, true))
                .unwrap()
                .join()
                .unwrap();
        }
    }

    #[test]
    fn colon_bodies_at_the_event_frame_limit_decline() {
        let _frames = event_pipeline::hold_event_frames(event_pipeline::EVENT_FRAME_LIMIT);
        let baseline = LadderHook::default();
        let expected = {
            let _standard_root = StandardEventRootGuard::install();
            let _standard_nested = StandardNestedReaderGuard::install();
            observe(
                "::: note\n@@@\n:::\n",
                ParseMode::Html,
                false,
                false,
                &baseline,
            )
        };
        let candidate = LadderHook::default();
        let before = colon_body_event_parses();
        let actual = {
            let _standard_root = StandardEventRootGuard::install();
            observe(
                "::: note\n@@@\n:::\n",
                ParseMode::Html,
                false,
                false,
                &candidate,
            )
        };
        assert_eq!(colon_body_event_parses(), before);
        assert_eq!(actual.0 .0, expected.0 .0);
        assert_same_callbacks(&candidate, &baseline, "depth limit");
    }
}

/// List, colon, definition and quote mixes whose depth rides the worklist.
/// Their bodies stay on the event cursor through `MAX_NESTING_DEPTH` while
/// the native event frames stay within `EVENT_FRAME_LIMIT`.
#[cfg(feature = "experimental-event-parser-options")]
mod deep_worklist_owner {
    use super::deep_quote_worklist::{check, Log};
    use super::*;

    /// Deep standard parses dominate the runtime, so only this depth runs
    /// every shape through the full matrix.
    const SHALLOW_DEPTH: usize = 8;

    /// Strength-2 covering rows over `(mode, positions, source_lines,
    /// collect)`. At `MAX_NESTING_DEPTH` shape `s` takes row `s`, so the six
    /// rows run there once each.
    const PAIRWISE: [(ParseMode, bool, bool, bool); 6] = [
        (ParseMode::Html, false, false, false),
        (ParseMode::Html, true, true, true),
        (ParseMode::HtmlFacade, false, true, false),
        (ParseMode::HtmlFacade, true, false, true),
        (ParseMode::Carve, false, false, true),
        (ParseMode::Carve, true, true, false),
    ];

    /// `(depth, shape, PAIRWISE row)`: one standard comparison for each
    /// remaining depth.
    const DEEP_CELLS: [(usize, &str, usize); 6] = [
        (64, "list siblings", 0),
        (120, "quoted list", 2),
        (199, "list", 4),
        (260, "colon/list", 1),
        (199, "definition chain", 0),
        (260, "definition chain", 2),
    ];

    /// `(depth, shape, CALLBACK_ROWS row)` for the deep callback comparisons.
    /// The definition chain crosses onto the standard reader at this depth.
    const DEEP_CALLBACK_CELLS: [(usize, &str, usize); 2] =
        [(199, "colon/list", 1), (199, "definition chain", 2)];

    /// `(mode, positions)` rows for the callback test, indexed like `PAIRWISE`.
    const CALLBACK_ROWS: [(ParseMode, bool); 6] = [
        (ParseMode::Html, false),
        (ParseMode::HtmlFacade, true),
        (ParseMode::Carve, false),
        (ParseMode::Html, true),
        (ParseMode::HtmlFacade, false),
        (ParseMode::Carve, true),
    ];

    fn full_matrix() -> Vec<(ParseMode, bool, bool, bool)> {
        let mut rows = Vec::new();
        for mode in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve] {
            for positions in [false, true] {
                for source_lines in [false, true] {
                    for collect in [false, true] {
                        rows.push((mode, positions, source_lines, collect));
                    }
                }
            }
        }
        rows
    }

    fn indent(level: usize) -> String {
        "  ".repeat(level)
    }

    /// A paragraph, a quote, a definition list and a figure group at `lead`.
    fn leaf(lead: &str) -> String {
        format!(
            "{lead}deep *leaf*\n\n{lead}> quoted\n\n{lead}:: term\n{lead}:  desc\n\n\
             {lead}::: figure\n{lead}![a](b)\n{lead}^ caption\n{lead}:::\n"
        )
    }

    /// Inside a container, so no root direct reader takes the whole shape.
    fn wrapped(body: String) -> String {
        format!("::: wrap\n{body}:::\n")
    }

    /// `(label, source)`; every shape but the last nests on the worklist.
    fn shapes(depth: usize) -> Vec<(&'static str, String)> {
        let list: String = (0..depth)
            .map(|level| format!("{}- l{level}\n", indent(level)))
            .chain([leaf(&indent(depth))])
            .collect();
        let colon = (0..depth)
            .map(|level| format!("::: n{level}\n"))
            .chain([leaf("")])
            .chain((0..depth).map(|_| ":::\n".to_string()))
            .collect();
        // Even levels open a container inside the item above them.
        let alternating = (0..depth)
            .map(|level| {
                let lead = indent(level / 2);
                if level % 2 == 0 {
                    format!("{lead}::: n{level}\n")
                } else {
                    format!("{lead}- l{level}\n")
                }
            })
            .chain([leaf(&indent(depth / 2))])
            .collect();
        let siblings = (0..depth)
            .map(|level| {
                let lead = indent(level);
                format!(
                    "{lead}- l{level}\n\n{lead}  > q{level}\n\n{lead}  :: t{level}\n\
                     {lead}  :  d{level}\n\n"
                )
            })
            .collect();
        let quoted = list.lines().map(|line| format!("> {line}\n")).collect();
        // Unobserved, description bodies ride the worklist. Observed, they
        // parse eagerly, so the chain holds a frame per level until the limit
        // and reads the rest on the standard reader.
        let definitions = (0..depth)
            .map(|level| {
                let lead = indent(level);
                format!("{lead}:: t{level}\n{lead}: d{level}\n\n")
            })
            .chain([leaf(&indent(depth))])
            .collect();
        vec![
            ("list", wrapped(list)),
            ("colon", colon),
            ("colon/list", wrapped(alternating)),
            ("list siblings", wrapped(siblings)),
            ("quoted list", wrapped(quoted)),
            ("definition chain", definitions),
        ]
    }

    fn worklist_shapes(depth: usize) -> impl Iterator<Item = (&'static str, String)> {
        shapes(depth)
            .into_iter()
            .filter(|(shape, _)| *shape != "definition chain")
    }

    fn shape(depth: usize, name: &str) -> String {
        shapes(depth)
            .into_iter()
            .find(|(shape, _)| *shape == name)
            .unwrap_or_else(|| panic!("unknown shape {name}"))
            .1
    }

    fn check_row(
        depth: usize,
        shape: &str,
        source: &str,
        (mode, positions, source_lines, collect): (ParseMode, bool, bool, bool),
    ) {
        let options = Options::default()
            .with_positions(positions)
            .with_source_lines(source_lines);
        let label = format!(
            "depth={depth}; shape={shape}; positions={positions}; \
             source_lines={source_lines}; mode={}; collect={collect}",
            mode as u8
        );
        check(&label, source, &options, mode, collect, true);
    }

    #[test]
    fn shallow_worklist_shapes_match_standard_across_the_full_matrix() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for (shape, source) in shapes(SHALLOW_DEPTH) {
                    for row in full_matrix() {
                        check_row(SHALLOW_DEPTH, shape, &source, row);
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn multiple_items_and_descriptions_keep_their_worklist_slots() {
        let source =
            ":: first\n: alpha\n: beta\n  - nested\n:: second\n: gamma\n: delta\n  > quote\n";
        for row in full_matrix() {
            check_row(2, "multiple definitions", source, row);
        }
    }

    /// Every shape at `MAX_NESTING_DEPTH` on its own `PAIRWISE` row, then
    /// `DEEP_CELLS` for the other deep depths.
    #[test]
    fn deep_worklist_shapes_match_standard_on_a_covering_set() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for (s, (shape, source)) in shapes(MAX_NESTING_DEPTH).into_iter().enumerate() {
                    check_row(MAX_NESTING_DEPTH, shape, &source, PAIRWISE[s]);
                }
                for (depth, name, row) in DEEP_CELLS {
                    check_row(depth, name, &shape(depth, name), PAIRWISE[row]);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// The old logical gate never admitted a body at depth 8 or deeper.
    #[test]
    fn deep_worklist_bodies_reach_the_event_cursor() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for depth in [64, 120, 199, 200, 260] {
                    let floor = depth.min(MAX_NESTING_DEPTH - 8) / 2;
                    for (shape, source) in worklist_shapes(depth) {
                        event_pipeline::PEAK_EVENT_DEPTH.with(|peak| peak.set(0));
                        let (document, _, used) = event_pipeline::parse_document_with_mode(
                            &source,
                            &Options::default(),
                            ParseMode::Html,
                        )
                        .unwrap_or_else(|| panic!("depth={depth}; shape={shape}: declined"));
                        assert!(used, "depth={depth}; shape={shape}");
                        let peak = event_pipeline::PEAK_EVENT_DEPTH.with(Cell::get);
                        assert!(peak >= floor, "depth={depth}; shape={shape}: peak {peak}");
                        std::mem::forget(document);
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    fn check_callbacks(depth: usize, shape: &str, source: &str, row: usize) {
        let (mode, positions) = CALLBACK_ROWS[row];
        let label = format!(
            "depth={depth}; shape={shape}; positions={positions}; mode={}",
            mode as u8
        );
        let baseline = Log::default();
        let candidate = Log::default();
        let expected = {
            let _standard_root = StandardEventRootGuard::install();
            let _standard_nested = StandardNestedReaderGuard::install();
            let options = Options::default()
                .with_positions(positions)
                .with_extension(&baseline);
            parse_with_options_mode_and_index(source, &options, mode).0
        };
        let options = Options::default()
            .with_positions(positions)
            .with_extension(&candidate);
        let (actual, _, used) = event_pipeline::parse_document_with_mode(source, &options, mode)
            .unwrap_or_else(|| panic!("{label}: declined"));
        assert!(used, "{label}");
        assert_eq!(actual, expected, "{label}: ast");
        assert_eq!(
            *candidate.blocks.borrow(),
            *baseline.blocks.borrow(),
            "{label}: block callbacks"
        );
        assert_eq!(
            *candidate.inlines.borrow(),
            *baseline.inlines.borrow(),
            "{label}: inline callbacks"
        );
    }

    /// Every shape on every `CALLBACK_ROWS` row at `SHALLOW_DEPTH`, then
    /// `DEEP_CALLBACK_CELLS`.
    #[test]
    fn deep_worklist_shapes_keep_extension_callback_order() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for (shape, source) in shapes(SHALLOW_DEPTH) {
                    for row in 0..CALLBACK_ROWS.len() {
                        check_callbacks(SHALLOW_DEPTH, shape, &source, row);
                    }
                }
                for (depth, name, row) in DEEP_CALLBACK_CELLS {
                    check_callbacks(depth, name, &shape(depth, name), row);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// Only the event route runs here, as in the deep quote release tests.
    /// The definition chain is left out: observed, past the frame limit it
    /// recurses on the standard reader, which this stack does not hold at
    /// these depths. `unobserved_definition_chains_fit_the_release_stack_floor`
    /// runs it unobserved.
    /// Each shape runs observed and unobserved at every depth; positions
    /// alternate so each of those pairs sees both values across the depths.
    /// Debug builds skip it; the `stack-floor-release` CI job runs it.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "release stack floor; run with --release")]
    fn deep_worklist_shapes_fit_the_release_stack_floor() {
        let stack = if cfg!(debug_assertions) {
            4 * 1024 * 1024
        } else {
            512 * 1024
        };
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(|| {
                for (d, depth) in [199, 200, 260].into_iter().enumerate() {
                    for (s, (shape, source)) in worklist_shapes(depth).enumerate() {
                        for observed in [false, true] {
                            let positions = (d + s + usize::from(observed)) % 2 == 1;
                            let label = format!(
                                "depth={depth}; shape={shape}; positions={positions}; \
                                 observed={observed}"
                            );
                            let log = Log::default();
                            let mut options = Options::default().with_positions(positions);
                            if observed {
                                options = options.with_extension(&log);
                            }
                            let (document, _, used) = event_pipeline::parse_document_with_mode(
                                &source,
                                &options,
                                ParseMode::Html,
                            )
                            .unwrap_or_else(|| panic!("{label}: declined"));
                            assert!(used, "{label}");
                            std::mem::forget(document);
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    const WORKLIST_DEPTH: usize = 150;

    /// `levels` definition entries from `lead`, each in the description body
    /// of the one before.
    fn definition_chain(lead: &str, levels: usize) -> String {
        (0..levels)
            .map(|level| {
                let lead = format!("{lead}{}", indent(level));
                format!("{lead}:: t{level}\n{lead}: d{level}\n\n")
            })
            .collect()
    }

    /// An eager definition chain over stack the frame count does not see.
    struct StackedChain {
        label: String,
        source: String,
        /// Lowest logical depth some event frame must reach.
        min_depth: usize,
        /// The chain outlasts the budget, so it must take event frames up to
        /// the last admitted level.
        fills: bool,
        direct_quotes: usize,
    }

    /// Chains whose stack under the eager part is bounded: the direct quote
    /// helper levels, or a worklist body restored deep.
    fn stacked_eager_chains() -> Vec<StackedChain> {
        let limit = event_pipeline::EVENT_FRAME_LIMIT;
        let direct = event_pipeline::DIRECT_QUOTE_DEPTH_LIMIT - 1;
        let prefix = "> ".repeat(direct);
        let quoted = definition_chain("", limit)
            .lines()
            .map(|line| {
                if line.is_empty() {
                    format!("{}\n", prefix.trim_end())
                } else {
                    format!("{prefix}{line}\n")
                }
            })
            .collect();
        let list: String = (0..WORKLIST_DEPTH)
            .map(|level| format!("{}- l{level}\n", indent(level)))
            .collect();
        let worklist = wrapped(format!(
            "{list}\n{}",
            definition_chain(&indent(WORKLIST_DEPTH), limit)
        ));
        vec![
            // The direct levels hold native frames, so the chain on top of
            // them reads on the standard reader.
            StackedChain {
                label: "direct quotes over definitions".to_string(),
                source: quoted,
                min_depth: 0,
                fills: false,
                direct_quotes: direct,
            },
            StackedChain {
                label: "worklist body over definitions".to_string(),
                source: worklist,
                min_depth: WORKLIST_DEPTH,
                fills: true,
                direct_quotes: 0,
            },
        ]
    }

    /// Root chains that go on past the budget on the standard reader, whose
    /// own recursion sets the stack these need.
    fn root_definition_chains() -> Vec<StackedChain> {
        [MAX_NESTING_DEPTH - 1, MAX_NESTING_DEPTH]
            .into_iter()
            .map(|depth| StackedChain {
                label: format!("root definitions; depth={depth}"),
                source: shape(depth, "definition chain"),
                min_depth: 0,
                fills: true,
                direct_quotes: 0,
            })
            .collect()
    }

    #[test]
    fn stacked_eager_chains_match_standard() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for chain in stacked_eager_chains()
                    .into_iter()
                    .chain(root_definition_chains())
                {
                    // Collecting keeps the bodies eager; without it they
                    // ride the worklist.
                    for collect in [false, true] {
                        let label = format!("{}; collect={collect}", chain.label);
                        check(
                            &label,
                            &chain.source,
                            &Options::default(),
                            ParseMode::Html,
                            collect,
                            true,
                        );
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// The event route alone on a `stack`-byte thread. No unquoted body takes
    /// an event frame `EVENT_FRAME_LIMIT` or more levels above the worklist
    /// base, so an eager chain spends its frames near the top of the stack.
    /// The collector makes the parse order observed, which keeps the
    /// description bodies eager.
    fn check_frame_budget(stack: usize, chains: Vec<StackedChain>) {
        let limit = event_pipeline::EVENT_FRAME_LIMIT;
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(move || {
                for chain in chains {
                    let label = &chain.label;
                    event_pipeline::PEAK_EVENT_FRAMES.with(|peak| peak.set(0));
                    event_pipeline::PEAK_EVENT_DEPTH.with(|peak| peak.set(0));
                    event_pipeline::PEAK_UNQUOTED_STACKED_DEPTH.with(|peak| peak.set(0));
                    event_pipeline::DIRECT_QUOTE_PARSES.with(|count| count.set(0));
                    let (document, _, used) = collecting_unattached_block_attrs(|| {
                        event_pipeline::parse_document_with_mode(
                            &chain.source,
                            &Options::default(),
                            ParseMode::Html,
                        )
                    })
                    .0
                    .unwrap_or_else(|| panic!("{label}: declined"));
                    assert!(used, "{label}");
                    let frames = event_pipeline::PEAK_EVENT_FRAMES.with(Cell::get);
                    assert!(frames <= limit, "{label}: {frames} frames");
                    let stacked = event_pipeline::PEAK_UNQUOTED_STACKED_DEPTH.with(Cell::get);
                    assert!(stacked < limit, "{label}: stacked {stacked}");
                    if chain.fills {
                        assert!(frames > 0, "{label}: no event frame");
                        assert_eq!(stacked, limit - 1, "{label}: stacked");
                    }
                    let depth = event_pipeline::PEAK_EVENT_DEPTH.with(Cell::get);
                    assert!(depth >= chain.min_depth, "{label}: event depth {depth}");
                    let quotes = event_pipeline::DIRECT_QUOTE_PARSES.with(Cell::get);
                    assert_eq!(quotes, chain.direct_quotes, "{label}: direct quotes");
                    assert_eq!(WORKLIST_BASE.with(Cell::get), 0, "{label}: base leak");
                    std::mem::forget(document);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// As in `deep_worklist_shapes_fit_the_release_stack_floor`.
    #[test]
    fn stacked_eager_chains_fit_the_release_stack_floor() {
        let stack = if cfg!(debug_assertions) {
            4 * 1024 * 1024
        } else {
            512 * 1024
        };
        check_frame_budget(stack, stacked_eager_chains());
    }

    /// A root chain at the cap recurses on the standard reader past the
    /// budget, so 512 KiB is not a floor the event route can owe it; the
    /// standard reader's own 2 MiB release budget is (`MAX_NESTING_DEPTH`).
    #[test]
    fn root_definition_chains_keep_the_frame_budget() {
        let stack = if cfg!(debug_assertions) {
            4 * 1024 * 1024
        } else {
            2 * 1024 * 1024
        };
        check_frame_budget(stack, root_definition_chains());
    }

    /// Unobserved, each description body waits on the worklist, so the chain
    /// stays on the event cursor without an event frame per level.
    fn check_unobserved_definition_chain(depth: usize, positions: bool) {
        let label = format!("depth={depth}; positions={positions}");
        event_pipeline::PEAK_EVENT_FRAMES.with(|peak| peak.set(0));
        event_pipeline::PEAK_EVENT_DEPTH.with(|peak| peak.set(0));
        let (document, _, used) = event_pipeline::parse_document_with_mode(
            &shape(depth, "definition chain"),
            &Options::default().with_positions(positions),
            ParseMode::Html,
        )
        .unwrap_or_else(|| panic!("{label}: declined"));
        assert!(used, "{label}");
        let frames = event_pipeline::PEAK_EVENT_FRAMES.with(Cell::get);
        assert!(
            frames < event_pipeline::EVENT_FRAME_LIMIT,
            "{label}: {frames} frames"
        );
        let floor = depth.min(MAX_NESTING_DEPTH - 8) / 2;
        let peak = event_pipeline::PEAK_EVENT_DEPTH.with(Cell::get);
        assert!(peak >= floor, "{label}: event depth {peak}");
        assert_eq!(WORKLIST_BASE.with(Cell::get), 0, "{label}: base leak");
        std::mem::forget(document);
    }

    #[test]
    fn unobserved_definition_chains_reach_the_event_cursor() {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                for (d, depth) in [SHALLOW_DEPTH, 64, 120, 199, 200, 260]
                    .into_iter()
                    .enumerate()
                {
                    check_unobserved_definition_chain(depth, d % 2 == 1);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// As in `deep_worklist_shapes_fit_the_release_stack_floor`.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "release stack floor; run with --release")]
    fn unobserved_definition_chains_fit_the_release_stack_floor() {
        let stack = if cfg!(debug_assertions) {
            4 * 1024 * 1024
        } else {
            512 * 1024
        };
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(|| {
                for depth in [199, 200, 260] {
                    for positions in [false, true] {
                        check_unobserved_definition_chain(depth, positions);
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn event_owned_facade_renders_with_a_derived_index() {
    for source in [
        "# One\n\nSee </#one>.\n",
        "# One\n\n# One\n\nSee </#one-2>.\n",
        "[^n]: note\n\n# Notes\n\nSee </#notes> and note[^n].\n",
        "```\nbody",
    ] {
        let options = Options::default();
        let (baseline_doc, baseline_index) =
            parse_with_options_mode_and_index(source, &options, ParseMode::HtmlFacade);
        let (event_doc, event_index, _) =
            event_pipeline::parse_document_with_mode(source, &options, ParseMode::HtmlFacade)
                .unwrap();
        assert_eq!(event_doc, baseline_doc, "{source:?}");
        let event_index = event_index.expect("facade index");
        let baseline = crate::render::render_html_owned_with_index(
            baseline_doc,
            &options,
            baseline_index,
            source.contains("[@") || source.contains(":::"),
            source.contains("[^") || source.contains("^["),
        );
        let event = crate::render::render_html_owned_with_index(
            event_doc,
            &options,
            event_index,
            source.contains("[@") || source.contains(":::"),
            source.contains("[^") || source.contains("^["),
        );
        assert_eq!(event, baseline, "{source:?}");
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn event_owned_facade_corpus_render_index_matches() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    for entry in std::fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension() != Some(std::ffi::OsStr::new("crv")) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for positions in [false, true] {
            for source_lines in [false, true] {
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines);
                let (baseline_doc, baseline_index) =
                    parse_with_options_mode_and_index(&source, &options, ParseMode::HtmlFacade);
                let (event_doc, event_index, _) = event_pipeline::parse_document_with_mode(
                    &source,
                    &options,
                    ParseMode::HtmlFacade,
                )
                .unwrap();
                assert_eq!(event_doc, baseline_doc, "{}", path.display());
                let baseline = crate::render::render_html_owned_with_index(
                    baseline_doc,
                    &options,
                    baseline_index,
                    source.contains("[@") || source.contains(":::"),
                    source.contains("[^") || source.contains("^["),
                );
                let event = crate::render::render_html_owned_with_index(
                    event_doc,
                    &options,
                    event_index.expect("facade index"),
                    source.contains("[@") || source.contains(":::"),
                    source.contains("[^") || source.contains("^["),
                );
                assert_eq!(event, baseline, "{}", path.display());
            }
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn inline_explicit_id_and_heading_slug_keep_mode_parity() {
    let options = Options::default();
    for source in [
        "# One\n\n`x`{#one}\n\nSee </#one>.\n",
        "# One\n\n[x](y){#one}\n\nSee </#one>.\n",
        "# one\n\n`x`{#one}\n\nSee </#one>.\n",
        "# one\n\n[x](y){#one}\n\nSee </#one>.\n",
        "# One\n\n{#one}\nplain\n\nSee </#one>.\n",
        "# One\n\n::: note\n# Two\n:::\n\n# Two\n\nSee </#two-1>.\n",
        "# One\n\n> # Two\n\n# Two\n\nSee </#two-1>.\n",
        "# One\n\n- # Two\n\n# Two\n\nSee </#two-1>.\n",
        "# One\n\n| a |\n|---|\n| # Two |\n\n# Two\n\nSee </#two-1>.\n",
    ] {
        for mode in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve] {
            let (expected, baseline_index) =
                parse_with_options_mode_and_index(source, &options, mode);
            let (actual, event_index, used) =
                event_pipeline::parse_document_with_mode(source, &options, mode).unwrap();
            assert!(used);
            assert_eq!(actual, expected);
            if matches!(mode, ParseMode::HtmlFacade) {
                let baseline = crate::render::render_html_owned_with_index(
                    expected,
                    &options,
                    baseline_index,
                    false,
                    false,
                );
                let event = crate::render::render_html_owned_with_index(
                    actual,
                    &options,
                    event_index.unwrap(),
                    false,
                    false,
                );
                assert_eq!(event, baseline);
            }
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn event_owned_facade_extension_index_matches() {
    use crate::CarveExtension;

    struct AddHeading;
    impl CarveExtension for AddHeading {
        fn name(&self) -> &'static str {
            "add-heading-index-test"
        }

        fn after_parse(&self, mut doc: Document) -> Document {
            doc.children.push(BlockNode::Heading(Heading {
                attrs: None,
                level: 1,
                children: vec![InlineNode::text("Added".to_string())],
                pos: None,
            }));
            doc
        }
    }

    let source = "See </#added>.\n";
    let extension = AddHeading;
    for positions in [false, true] {
        for source_lines in [false, true] {
            let options = Options::default()
                .with_extension(&extension)
                .with_positions(positions)
                .with_source_lines(source_lines);
            let (baseline_doc, baseline_index) =
                parse_with_options_mode_and_index(source, &options, ParseMode::HtmlFacade);
            let (event_doc, event_index, used) =
                event_pipeline::parse_document_with_mode(source, &options, ParseMode::HtmlFacade)
                    .unwrap();
            assert!(used);
            assert_eq!(event_doc, baseline_doc);
            let baseline = crate::render::render_html_owned_with_index(
                baseline_doc,
                &options,
                baseline_index,
                false,
                false,
            );
            let event = crate::render::render_html_owned_with_index(
                event_doc,
                &options,
                event_index.expect("facade index"),
                false,
                false,
            );
            assert_eq!(event, baseline);
        }
    }
}

#[cfg(test)]
mod footnote_comment_fence_work {
    use super::FOOTNOTE_FENCE_CLOSER_LOOKUPS;

    #[test]
    fn repeated_note_comments_use_bounded_index_lookups() {
        for count in [32, 64, 128] {
            let mut source = String::new();
            for index in 0..count {
                source.push_str(&format!(
                    "[^n{index}]: note\n%% comment\n```\ncode\n```\n\n"
                ));
            }
            FOOTNOTE_FENCE_CLOSER_LOOKUPS.with(|work| work.set(0));
            let html = crate::to_html(&source);
            assert_eq!(html.matches("<pre><code>").count(), count);
            let lookups = FOOTNOTE_FENCE_CLOSER_LOOKUPS.with(|work| work.get());
            // Rule A (carve#3885): the comment closes the note, so no note
            // retains the fence and none looks for its closer.
            assert_eq!(lookups, 0, "{count}: {lookups} lookups");
        }
    }
}

#[cfg(test)]
mod container_comment_dedent_steps {
    //! COUNTED guard on the container-comment dedent walk
    //! (markup-carve/carve-rs#1047).
    //!
    //! Counted rather than timed, for the reason the module below it records at
    //! length: a call count is a property of the algorithm and reproduces
    //! byte-identically across runs and loads, while this repo's timing tests
    //! had to be serialized against each other to stop them flaking.
    //!
    //! The subject is the shape the first version of the fix had. One container
    //! can hold many comment openers, and answering "does this one close inside
    //! its container" by walking forward from each meant walking the whole
    //! container once per opener - O(m^3) work for an O(m^2) document, which is
    //! exactly the class `comment_fence_close_index` exists to avoid for
    //! column-0 openers.

    use super::CONTAINER_DEDENT_STEPS;

    /// Dedent-walk steps over one full parse-and-render of `src`, on its own
    /// thread so the thread-local tally cannot pick up another test's.
    fn steps_for(src: String) -> u64 {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                CONTAINER_DEDENT_STEPS.with(|c| c.set(0));
                let _ = crate::parse(&src);
                CONTAINER_DEDENT_STEPS.with(|c| c.get())
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// `m` comment openers of distinct widths at one item's content column,
    /// above `m * m` filler lines, with every matching closer only past the
    /// dedent. Every opener therefore passes the document-wide "is there a
    /// closer of this width later" test and none of them closes inside the
    /// item, which is the worst case for the container bound.
    ///
    /// The work is the line count: a walk that visits the container once has to
    /// grow with it, and one that visits it per opener cannot stay flat.
    fn openers_over_one_container(m: usize) -> (String, u64) {
        let mut out = String::from("- x\n");
        for i in 0..m {
            out.push_str("  ");
            out.push_str(&"%".repeat(3 + i));
            out.push_str(" o\n");
        }
        for _ in 0..m * m {
            out.push_str("  filler line here\n");
        }
        for i in 0..m {
            out.push_str(&"%".repeat(3 + i));
            out.push('\n');
        }
        out.push_str("\n[r]: /u\n\n[r][]\n");
        let lines = out.lines().count() as u64;
        (out, lines)
    }

    /// The walk must cost steps in proportion to the DOCUMENT, not to openers
    /// times container length.
    ///
    /// The lower bound keeps the counter live. Per-line cost must stay flat as
    /// the opener count grows, catching per-opener rescanning.
    #[test]
    fn many_openers_over_one_container_walk_it_once_apiece() {
        let (small_src, small_lines) = openers_over_one_container(60);
        let (large_src, large_lines) = openers_over_one_container(120);
        let small_steps = steps_for(small_src);
        let large_steps = steps_for(large_src);

        assert!(
            small_steps > 0 && large_steps > 0,
            "the dedent-step counter is dead: {small_steps} and {large_steps} steps",
        );
        assert!(
            large_steps <= 8 * large_lines,
            "{large_steps} steps over {large_lines} lines ({:.1} each): \
             the container is being re-walked per opener",
            large_steps as f64 / large_lines as f64,
        );
        assert!(
            large_steps * small_lines * 10 <= small_steps * large_lines * 11,
            "the per-line dedent cost climbs with size ({:.1} at {small_lines} lines, \
             {:.1} at {large_lines}): the container is being re-walked per opener",
            small_steps as f64 / small_lines as f64,
            large_steps as f64 / large_lines as f64,
        );
    }

    /// The same worst case one prefix over: `m` quoted openers of distinct
    /// widths in ONE quote, above `m * m` quoted filler lines, with every
    /// matching closer only past the blank that ends the quote.
    ///
    /// The quote bound (markup-carve/carve#1341) is a second forward walk, and a
    /// new walk with no counted guard is a walk nobody would notice going
    /// quadratic. Every opener passes the "is there a closer of this width and
    /// depth later" test and none of them closes inside its quote, which is the
    /// shape that makes a per-opener walk visible.
    fn quoted_openers_over_one_quote(m: usize) -> (String, u64) {
        let mut out = String::new();
        for i in 0..m {
            out.push_str("> ");
            out.push_str(&"%".repeat(3 + i));
            out.push_str(" o\n");
        }
        for _ in 0..m * m {
            out.push_str("> filler line here\n");
        }
        out.push('\n');
        for i in 0..m {
            out.push_str("> ");
            out.push_str(&"%".repeat(3 + i));
            out.push('\n');
        }
        out.push_str("\n[r]: /u\n\n[r][]\n");
        let lines = out.lines().count() as u64;
        (out, lines)
    }

    /// The three claims `many_openers_over_one_container_walk_it_once_apiece`
    /// makes, for the blank-line bound: a floor so a dead counter cannot pass,
    /// a ceiling, and the flat per-line shape that a per-opener walk cannot hold.
    #[test]
    fn many_quoted_openers_over_one_quote_walk_it_once_apiece() {
        let (small_src, small_lines) = quoted_openers_over_one_quote(60);
        let (large_src, large_lines) = quoted_openers_over_one_quote(120);
        let small_steps = steps_for(small_src);
        let large_steps = steps_for(large_src);

        #[cfg(feature = "experimental-event-parser")]
        if small_steps == 0 && large_steps == 0 {
            // The event route can own both inputs without the counted reader.
            assert!(crate::parse_experimental_event_blocks(&quoted_openers_over_one_quote(60).0).1);
            assert!(
                crate::parse_experimental_event_blocks(&quoted_openers_over_one_quote(120).0).1
            );
            return;
        }

        assert!(
            small_steps > 0 && large_steps > 0,
            "the blank-walk counter is dead: {small_steps} and {large_steps} steps",
        );
        assert!(
            large_steps <= 8 * large_lines,
            "{large_steps} steps over {large_lines} lines ({:.1} each): \
             the quote is being re-walked per opener",
            large_steps as f64 / large_lines as f64,
        );
        assert!(
            large_steps * small_lines * 10 <= small_steps * large_lines * 11,
            "the per-line blank-walk cost climbs with size ({:.1} at {small_lines} lines, \
             {:.1} at {large_lines}): the quote is being re-walked per opener",
            small_steps as f64 / small_lines as f64,
            large_steps as f64 / large_lines as f64,
        );
    }
}

#[cfg(test)]
mod quote_prefix_calls {
    //! COUNTED guards on the blockquote prefix (markup-carve/carve-rs#731).
    //!
    //! They live here rather than in `tests/` because the counter they read is
    //! `#[cfg(test)]`, so a release build carries none of it. `cargo test` runs
    //! this binary, which is what CI runs.
    //!
    //! Counted, not timed, deliberately. This repo already records why a clock
    //! cannot express these bounds: `tests/perf_regressions.rs` had to
    //! serialize its 34 timing tests against each other because they flaked
    //! competing for cores, and carve-js's `writer-deep-list-perf.test.ts`
    //! carries "No ratio guard here on purpose... would also fail on the
    //! healthy build". A call count is a property of the algorithm, not of the
    //! machine: every figure quoted below reproduces byte-identically across
    //! runs and loads.

    use super::{
        detect_fence_open, fence_closer_suffix_maxima, is_fence_close, leading_ws,
        strip_container_prefixes_keep_indent, strip_prepass_blockquote_prefix, FenceOpen,
        QUOTE_PREFIX_CALLS,
    };
    #[test]
    fn repeated_unclosed_quoted_fences_have_linear_work() {
        for definitions in [false, true] {
            let mut measurements = Vec::new();
            for n in [128, 256, 512, 1024] {
                let tail = if definitions { "\n[r]: /target\n" } else { "" };
                let source = format!("> ::: |\n> verse\n{}> :::\n{tail}", "> ```x\n".repeat(n));
                let calls = calls_for(source);
                println!("quoted_fences definitions={definitions} n={n} prefix_calls={calls}");
                measurements.push(calls);
            }
            assert!(measurements[3] <= measurements[0] * 12,
                "eight times the input must use at most twelve times the prefix work: {measurements:?}");
        }
    }

    #[test]
    fn prepass_closer_index_preserves_columns_and_run_widths() {
        let lines = [
            "payload",
            "> ```",
            "  ~~~~",
            " > `````",
            "> - ```",
            "- ~~~",
            "``` info",
            "> > ~~~~~",
            "```` tail",
            "  ```\t",
        ];
        let closers = fence_closer_suffix_maxima(&lines);
        for index in 0..lines.len() {
            for quoted in [false, true] {
                for content_col in 0..=6 {
                    for fence_char in *b"`~" {
                        for fence_len in 3..=6 {
                            let open = FenceOpen {
                                fence_char,
                                fence_len,
                                content_col,
                                quoted,
                                ..detect_fence_open("```").unwrap()
                            };
                            let suffix = if fence_char == b'`' {
                                closers.maxima.0[index + 1]
                            } else {
                                closers.maxima.1[index + 1]
                            };
                            let expected = suffix >= fence_len
                                && lines[index + 1..].iter().any(|&line| {
                                    let line = if quoted {
                                        strip_prepass_blockquote_prefix(line).unwrap_or(line)
                                    } else {
                                        line
                                    };
                                    let kept = strip_container_prefixes_keep_indent(line);
                                    let candidate = if leading_ws(&kept) >= content_col {
                                        &kept[content_col..]
                                    } else {
                                        &kept
                                    };
                                    is_fence_close(candidate, open)
                                });
                            assert_eq!(closers.closes_after(index, open), expected,
                                "index={index}, quoted={quoted}, column={content_col}, character={fence_char}, width={fence_len}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lazy_quote_fence_admission_has_linear_prefix_work() {
        for shape in 0..4 {
            let measurements: Vec<_> = [128, 256, 512, 1024]
                .into_iter()
                .map(|n| {
                    let source = match shape {
                        0 => format!("> p\n{}> ```\n> [r]: /target\n", "lazy\n".repeat(n)),
                        1 => "> p\nlazy\n> ```\n> [r]: /target\n\n".repeat(n),
                        2 => format!(
                            "> ```a\n{}> ```\nlazy\n> ~~~\n> [r]: /target\n",
                            "> ```x\n".repeat(n)
                        ),
                        _ => format!("{}> ```\n", "> ```a\nlazy\n".repeat(n)),
                    };
                    calls_for(source)
                })
                .collect();
            assert!(
                measurements[3] <= measurements[0] * 12,
                "lazy quote admission repeats prefix work: {measurements:?}"
            );
        }
    }

    /// Strips counted over one full parse-and-render of `src`.
    ///
    /// On its own thread for two reasons: the counter is thread-local, so a
    /// fresh thread cannot pick up another test's tally, and a 200-deep quote
    /// recurses past the default 2 MiB test stack in a debug build.
    fn calls_for(src: String) -> u64 {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                QUOTE_PREFIX_CALLS.with(|c| c.set(0));
                let _ = crate::parse(&src);
                QUOTE_PREFIX_CALLS.with(|c| c.get())
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// One measurement: the work the document contains, and the strips it cost.
    struct Measured {
        work: u64,
        calls: u64,
    }

    impl Measured {
        fn per_unit(&self) -> f64 {
            self.calls as f64 / self.work as f64
        }
    }

    /// Assert that a document's strip count stays PROPORTIONAL to the work it
    /// contains, across a doubling of that work.
    ///
    /// Three separate claims, each of which has been shown to fail on its own:
    ///
    /// 1. A floor. Every quote marker has to be stripped at least once to be
    ///    recognized, so a count below the work is not a faster parser - it is a
    ///    counter that stopped counting. Without this the rest would pass on a
    ///    dead counter reporting zero.
    /// 2. A ceiling, generous enough to survive honest constant-factor drift.
    /// 3. The shape, which is the load-bearing half: a cost proportional to the
    ///    work holds its per-unit rate flat or falling as the document grows,
    ///    while anything superlinear in the work must make it climb. This one
    ///    is a statement about the curve rather than its constant, so it still
    ///    fires when a ceiling has been raised past the point of usefulness.
    ///
    /// The comparison is cross-multiplied integers so no float rounding enters
    /// the verdict; a tenth is allowed for drift. Floats appear in the failure
    /// messages only.
    fn assert_proportional(what: &str, small: Measured, large: Measured, ceiling: u64) {
        assert!(
            small.calls >= small.work && large.calls >= large.work,
            "{what}: the strip counter is not counting - {} strips for {} units of work, \
             {} for {}",
            small.calls,
            small.work,
            large.calls,
            large.work,
        );
        assert!(
            large.calls <= ceiling * large.work,
            "{what}: {} strips for {} units of work ({:.1} each, ceiling {ceiling}) - \
             the quote markers are being re-walked",
            large.calls,
            large.work,
            large.per_unit(),
        );
        assert!(
            large.calls * small.work * 10 <= small.calls * large.work * 11,
            "{what}: the per-unit strip cost climbs with size ({:.1} at the smaller size, \
             {:.1} at the larger): the quote markers are being re-walked",
            small.per_unit(),
            large.per_unit(),
        );
    }

    /// A blockquote depth ladder: line `i` carries `i` quote markers, so the
    /// document holds `depth * (depth + 1) / 2` markers in total - which is the
    /// work, and is exactly what carve-js spends on it.
    fn ladder(depth: usize) -> (String, u64) {
        ladder_of(depth, "a")
    }

    /// The same ladder with a chosen line body, so a guard can say what the
    /// quoted text is made of.
    fn ladder_of(depth: usize, body: &str) -> (String, u64) {
        let mut out = String::new();
        for i in 1..=depth {
            for _ in 0..i {
                out.push_str("> ");
            }
            out.push_str(body);
            out.push('\n');
        }
        (out, (depth * (depth + 1) / 2) as u64)
    }

    /// One deep quoted paragraph followed by `depth` lazy continuation lines,
    /// each of which has to ask whether that paragraph is still open. The work
    /// is one pass over every line at every level.
    fn lazy_tail(depth: usize) -> (String, u64) {
        let mut out = "> ".repeat(depth);
        out.push_str("a\n");
        for _ in 0..depth {
            out.push_str("lazy\n");
        }
        (out, ((depth + 1) * depth) as u64)
    }

    /// A ladder must not cost strips per nesting level per line.
    ///
    /// Before the fix the quote's paragraph-open state was decided eagerly on
    /// every quoted line, and deciding it walked the line down to its innermost
    /// content - a walk each enclosing level repeated over the same markers:
    ///
    /// | depth | markers | strips, eager | strips, deferred |
    /// | ----- | ------- | ------------- | ---------------- |
    /// | 25    | 325     | 6,495         | 3,245            |
    /// | 50    | 1,275   | 35,495        | 12,120           |
    /// | 100   | 5,050   | 223,495       | 46,745           |
    /// | 200   | 20,100  | 1,556,994     | 183,494          |
    ///
    /// carve-rs still spends about 9 strips per marker where carve-js spends 1.
    /// That residue is the prefix re-scan every implementation shares
    /// (markup-carve/carve#752), it is flat in depth, and it is not this
    /// guard's subject.
    #[test]
    fn a_depth_ladder_costs_strips_in_proportion_to_its_markers() {
        let (small_src, small_work) = ladder(100);
        let (large_src, large_work) = ladder(200);
        assert_proportional(
            "depth ladder",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// Ordinary quoted PROSE holding a colon must stay on the deferred path.
    ///
    /// The ladder above is made of `a`, so it says nothing about the pre-test
    /// that decides when §12's absorption flag has to be resolved
    /// (markup-carve/carve-rs#738). That pre-test has to be conservative, and
    /// the conservative reading is a COLON RUN: widening it to any `:` puts the
    /// cubic walk back for every `Note:`, `12:30` and `https://` in a quoted
    /// document - 1,556,994 strips at depth 200 rather than 183,494 - while the
    /// colon-free ladder above stays green and sees none of it.
    ///
    /// A line that really does carry `:::` is not covered, and cannot be:
    /// deciding the absorption on such a line is what #738 asks for, and that
    /// answer is only reachable by walking to the innermost content.
    #[test]
    fn quoted_prose_holding_a_colon_still_defers() {
        let (small_src, small_work) = ladder_of(100, "Note: at 12:30, see https://example.com");
        let (large_src, large_work) = ladder_of(200, "Note: at 12:30, see https://example.com");
        assert_proportional(
            "a depth ladder of colon-bearing prose",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// A depth ladder ending in one lazy line must not replay the body at every
    /// level: with no fence in it there is nothing to replay for.
    #[test]
    fn a_depth_ladder_ending_in_a_lazy_line_costs_strips_in_proportion_to_its_markers() {
        let with_lazy = |depth: usize| {
            let (mut src, work) = ladder(depth);
            src.push_str("lazy\n");
            (src, work)
        };
        let (small_src, small_work) = with_lazy(100);
        let (large_src, large_work) = with_lazy(200);
        assert_proportional(
            "depth ladder ending in a lazy line",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// A run of lazy continuation lines must resolve the open paragraph once,
    /// not once per line.
    ///
    /// The ladder above never reads the paragraph-open state at all, so it
    /// cannot see this: deferring the walk without caching its answer passes
    /// the ladder guard and the whole corpus byte for byte, while costing
    /// 4,143,610 strips here against 103,910 - and climbing, 28 to 53 to 103
    /// per unit across depths 50, 100 and 200, where the cached answer falls,
    /// 2.86 to 2.68 to 2.58.
    #[test]
    fn lazy_continuation_under_a_deep_quote_resolves_the_paragraph_once() {
        let (small_src, small_work) = lazy_tail(100);
        let (large_src, large_work) = lazy_tail(200);
        assert_proportional(
            "lazy continuation under a deep quote",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            8,
        );
    }
}

#[cfg(test)]
mod footnote_body_floor_unit {
    use super::{footnote_body_floor, indent_columns, leading_ws};

    #[test]
    fn a_tab_makes_the_two_spellings_disagree() {
        // THE PROOF. One tab indents the definition line. As characters it is
        // 1, so the old spelling asked continuations to reach column 3; as
        // columns it is 4, so the floor is 6. A continuation at column 4 or 5
        // would have been accepted under the old spelling and must not be.
        let def = "\t[^a]: note";
        assert_eq!(leading_ws(def), 1, "the character count");
        assert_eq!(indent_columns(def), 4, "the column count");
        assert_ne!(
            leading_ws(def) + 2,
            footnote_body_floor(def),
            "the two spellings must differ on a tabbed definition line, or this \
             test is pinning nothing"
        );
        assert_eq!(footnote_body_floor(def), 6);
    }

    #[test]
    fn a_mixed_run_disagrees_too() {
        // A space then a tab: the tab advances to the next 4-stop from column
        // 1, so the indentation is still 4 columns while being 2 characters.
        // Included because a fix that special-cased a LEADING tab would pass
        // the case above and fail here.
        let def = " \t[^a]: note";
        assert_eq!(leading_ws(def), 2);
        assert_eq!(indent_columns(def), 4);
        assert_eq!(footnote_body_floor(def), 6);
    }

    #[test]
    fn control_space_only_indentation_is_unchanged() {
        // CONTROL, and the reason the bug was latent: for space-only
        // indentation the two counts are equal, so every reachable input today
        // gets the same answer either way. This is what must NOT move.
        for def in ["[^a]: note", "  [^a]: note", "      [^a]: note"] {
            assert_eq!(leading_ws(def), indent_columns(def), "{def}");
            assert_eq!(footnote_body_floor(def), leading_ws(def) + 2, "{def}");
        }
    }

    #[test]
    fn control_the_floor_is_the_indentation_plus_two() {
        // CONTROL for the other half of the expression. §16 asks for ">= 2"
        // relative to the definition line; a mutation changing the addend
        // passes every unit assertion above that only compares the two
        // spellings, so the constant is pinned on its own.
        assert_eq!(footnote_body_floor("[^a]: x"), 2);
        assert_eq!(footnote_body_floor("    [^a]: x"), 6);
    }
}

#[cfg(test)]
mod promote_block_images_is_iterative {
    use super::promote_block_images;
    use crate::ast::{BlockNode, Div};

    /// Nest `depth` divs, innermost first, and return the outer level.
    fn nested_divs(depth: usize) -> Vec<BlockNode> {
        let mut children: Vec<BlockNode> = Vec::new();
        for _ in 0..depth {
            children = vec![BlockNode::Div(Div {
                attrs: None,
                label: None,
                children,
                pos: None,
            })];
        }
        children
    }

    /// Nothing pinned that this pass descends into a BLOCK QUOTE.
    ///
    /// Found by mutation while turning the pass into a worklist: drop the
    /// `BlockNode::BlockQuote` arm and all 155 corpus tests still pass, as do
    /// `reference_image_fmt`, `unresolved_reference_shape`,
    /// `caption_inside_a_list_item`, `non_html_parity` and
    /// `position_spans_match_source`. The behavior does change - the quote
    /// publishes `<p><img ...></p>` instead of a bare block image - so the
    /// descent set was live and simply unpinned. Every other arm has a corpus
    /// document behind it; this one did not.
    #[test]
    fn a_resolved_reference_image_promotes_inside_a_block_quote() {
        let html = crate::to_html("> ![a][i]\n\n[i]: p.png\n");
        assert!(
            !html.contains("<p>"),
            "the sole image in the quote must promote to a block image, got: {html}"
        );
        assert!(
            html.contains("<img src=\"p.png\" alt=\"a\">"),
            "got: {html}"
        );
    }

    /// THE PROOF that the pass costs no native stack per level.
    ///
    /// The thread's stack is 256 KiB and the tree is 4000 levels deep. The
    /// recursive spelling this pass had until markup-carve/carve-wasm#44 spent
    /// about 1.4 KiB of stack per level, so it needed roughly 5.5 MiB here and
    /// aborted the process; the worklist spelling needs a constant frame and a
    /// heap vector. Revert `promote_block_images` to recursion and this test
    /// takes the whole run down with it, which is what makes it load-bearing.
    ///
    /// 4000 levels is far past `MAX_NESTING_DEPTH`, and deliberately so. The
    /// pass runs only over parser-produced trees, so 200 is the real ceiling
    /// and 200 recursive levels would still fit a test thread - the depth here
    /// is chosen so the difference between the two spellings cannot be read as
    /// measurement noise.
    ///
    /// The tree is leaked rather than dropped. Teardown is compiler-generated
    /// recursive drop glue and would overflow on its own, which would prove
    /// nothing about this pass.
    #[test]
    fn a_four_thousand_level_tree_survives_a_small_stack() {
        let worker = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let mut blocks = nested_divs(4000);
                promote_block_images(&mut blocks, false, true);
                promote_block_images(&mut blocks, true, false);
                std::mem::forget(blocks);
            })
            .expect("spawn");
        worker.join().expect("the pass must not overflow the stack");
    }
}

#[cfg(test)]
mod collector_scope_tests {
    use super::*;

    #[test]
    fn an_unwinding_nested_collector_restores_its_parent() {
        let (_, outer) = collecting_unattached_block_attrs(|| {
            note_unattached_block_attrs(Some(Pos {
                start_line: 1,
                ..Pos::default()
            }));
            let result = std::panic::catch_unwind(|| {
                collecting_unattached_block_attrs(|| {
                    note_unattached_block_attrs(Some(Pos {
                        start_line: 2,
                        ..Pos::default()
                    }));
                    panic!("extension failed");
                });
            });
            assert!(result.is_err());
            note_unattached_block_attrs(Some(Pos {
                start_line: 3,
                ..Pos::default()
            }));
        });
        assert_eq!(
            outer.iter().map(|pos| pos.start_line).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert!(UNATTACHED_BLOCK_ATTRS.with(|slot| slot.borrow().is_none()));
    }
}

#[cfg(test)]
mod comment_fence_work {
    use super::COMMENT_FENCE_BYTES;

    fn measure(n: usize) -> (u64, u64) {
        let mut source = String::new();
        for width in 3..n + 3 {
            source.push_str(&"%".repeat(width));
            source.push_str(" x\n");
        }
        COMMENT_FENCE_BYTES.with(|count| count.set(0));
        assert_eq!(crate::to_html(&source), "");
        let scanned = COMMENT_FENCE_BYTES.with(|count| count.get());
        (source.len() as u64, scanned)
    }

    #[test]
    fn unterminated_comment_fence_openers_scan_proportional_bytes() {
        // Widths grow with the opener count, so input bytes grow quadratically.
        // A suffix rescan per opener adds another factor; count both opener
        // and closer scans to detect it without depending on machine load.
        let (small_bytes, small_scanned) = measure(64);
        let (large_bytes, large_scanned) = measure(128);
        assert!(small_scanned >= small_bytes && large_scanned >= large_bytes,
            "the counter must observe the input: {small_scanned}/{small_bytes}, {large_scanned}/{large_bytes}");
        assert!(
            large_scanned <= 16 * large_bytes,
            "comment fence scans visited {large_scanned} bytes for {large_bytes} input bytes"
        );
        assert!(large_scanned * small_bytes * 10 <= small_scanned * large_bytes * 11,
            "comment fence work per byte grew: {small_scanned}/{small_bytes}, {large_scanned}/{large_bytes}");
    }
}

#[cfg(test)]
mod nested_list_content_scans {
    #[test]
    fn carried_boundaries_match_fresh_scans() {
        use super::*;
        for marker in ["- ", "1. ", "-{.x} ", "- [x] ", "\t- "] {
            for tail in [
                "end\n",
                "α *bold*\n   # heading\n",
                "``` =html\n<pre>x</pre>\n```\n",
            ] {
                let source = format!("{}{tail}", marker.repeat(24));
                for positions in [false, true] {
                    let options = Options {
                        positions,
                        source_lines: positions,
                        ..Options::default()
                    };
                    REUSE_MARKER_TAIL.with(|flag| flag.set(false));
                    let expected = parse_with_options(&source, &options);
                    REUSE_MARKER_TAIL.with(|flag| flag.set(true));
                    assert_eq!(
                        parse_with_options(&source, &options),
                        expected,
                        "{marker:?} {tail:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn descendants_reuse_the_marker_tail_boundary() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                for depth in [32, 64, 128, 192] {
                    super::INNERMOST_CONTENT_SCANS.with(|count| count.set(0));
                    super::INNERMOST_MARKER_STEPS.with(|count| count.set(0));
                    let source = format!("{}end\n", "- ".repeat(depth));
                    let html = crate::to_html(&source);
                    assert_eq!(html.matches("<ul>").count(), depth);
                    assert_eq!(super::INNERMOST_CONTENT_SCANS.with(|count| count.get()), 0);
                    assert!(super::INNERMOST_MARKER_STEPS.with(|count| count.get()) <= depth * 2);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(test)]
mod reference_definition_original_positions {
    use super::*;

    #[test]
    fn definition_spans_select_original_codepoints() {
        for newline in ["\n", "\r\n", "\r"] {
            for prefix in ["", "\u{feff}", "😀 prose", "\u{feff}😀 prose"] {
                let definition = "[a]: /first";
                let source = if prefix.is_empty() || prefix == "\u{feff}" {
                    format!("{prefix}{definition}")
                } else {
                    format!("{prefix}{newline}{newline}{definition}")
                };
                let options = Options {
                    positions: true,
                    ..Options::default()
                };
                let doc = parse_with_options(&source, &options);
                let node = doc
                    .children
                    .iter()
                    .find_map(|node| match node {
                        BlockNode::LinkReferenceDefinition(def) => Some(def),
                        _ => None,
                    })
                    .expect("definition");
                let pos = node.pos.as_ref().expect("position");
                let selected: String = source
                    .chars()
                    .skip(pos.start_offset)
                    .take(pos.end_offset - pos.start_offset)
                    .collect();
                assert_eq!(selected, definition, "{source:?}");
                assert_eq!(pos.start_column, if prefix == "\u{feff}" { 2 } else { 1 });
                assert_eq!(
                    pos.end_column - pos.start_column,
                    definition.chars().count()
                );
            }
        }
    }
}

#[cfg(test)]
mod reference_definition_provenance {
    use super::*;

    #[test]
    fn hoisted_definitions_keep_the_authored_line() {
        for (source, expected) in [
            ("---\ntitle: x\n---\n\n[r]: /u", "[r]: /u"),
            ("> [r]: /u", "> [r]: /u"),
            ("- [r]: /u", "- [r]: /u"),
            ("[^a]: note\n  [r]: /u\n\nsee[^a] [t][r]", "  [r]: /u"),
            (
                "[^a]: note\n  [^b]: nested\n    [r]: /u\n\nsee[^a]",
                "    [r]: /u",
            ),
            ("😀\0 prose\n\n[r]: /u", "[r]: /u"),
        ] {
            for newline in ["\n", "\r\n", "\r"] {
                let source = format!("\u{feff}{}", source.replace('\n', newline));
                let doc = parse_with_options(&source, &Options::default().with_positions(true));
                let pos = doc
                    .children
                    .iter()
                    .find_map(|node| match node {
                        BlockNode::LinkReferenceDefinition(def) => def.pos.as_ref(),
                        _ => None,
                    })
                    .expect("definition position");
                let selected: String = source
                    .chars()
                    .skip(pos.start_offset)
                    .take(pos.end_offset - pos.start_offset)
                    .collect();
                assert_eq!(selected, expected, "{source:?}");
            }
        }
    }

    #[test]
    fn footnote_definition_columns_include_the_bom() {
        let options = Options::default().with_positions(true);
        let doc = parse_with_options("\u{feff}[^n]: {empty}", &options);
        let pos = doc.footnote_def_pos.get("n").expect("footnote position");
        assert_eq!(pos.start_column, 2);
        assert_eq!(pos.start_offset, 1);
    }
}

#[cfg(test)]
mod container_open_readings {
    //! COUNTED guards on a RECOVERED container's nesting cost
    //! (markup-carve/carve#2693, implemented here by carve-rs#2260).
    //!
    //! They live here rather than in `tests/` for the two reasons
    //! `quote_prefix_calls` gives: the counters are `#[cfg(test)]`, and a count
    //! is a property of the algorithm rather than of the machine. The second
    //! reason binds harder here than usual, because the shape these guards name
    //! does not appear in `tests/perf_regressions.rs` at all and that file is
    //! OFF the pull-request path - so a timed spelling of this would have let a
    //! regression merge before anything measured it.
    //!
    //! Every colon opener the timing suite builds - `::: note`, `::: d`,
    //! `::: index`, and the widening `::: d` ladder - reads as VALID metadata.
    //! Nothing there puts an invalid opener on the nesting path, which is the
    //! blind spot carve-php merged a cubic regression through.

    use super::{CONTAINER_OPEN_READINGS, CONTAINER_RECOVERY_ACCEPTS, CONTAINER_RECOVERY_CHARS};

    struct Counted {
        readings: u64,
        recovery_chars: u64,
        recoveries: u64,
        nested: usize,
    }

    /// Counts over one full parse-and-render of `src`, on its own thread: the
    /// counters are thread-local, and a deep ladder recurses past the default
    /// test stack in a debug build.
    fn count(src: String) -> Counted {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                CONTAINER_OPEN_READINGS.with(|c| c.set(0));
                CONTAINER_RECOVERY_CHARS.with(|c| c.set(0));
                CONTAINER_RECOVERY_ACCEPTS.with(|c| c.set(0));
                let html = crate::to_html(&src);
                Counted {
                    readings: CONTAINER_OPEN_READINGS.with(|c| c.get()),
                    recovery_chars: CONTAINER_RECOVERY_CHARS.with(|c| c.get()),
                    recoveries: CONTAINER_RECOVERY_ACCEPTS.with(|c| c.get()),
                    nested: html.matches("<aside").count() + html.matches("<div").count(),
                }
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// `depth` nested colon containers and their closers, every line the same
    /// length at every depth.
    ///
    /// Widths alternate 3/4 rather than widening outward, so a per-byte reading
    /// measures the nesting and not the fence growing with the ladder. The
    /// metadata decides validity and nothing else: `note Bare title!` is an
    /// unquoted title, which §3 refuses and §12 recovers, and
    /// `noteaaaaaaaaaaaa` is a plain kind of the same byte length.
    fn ladder(depth: usize, metadata: &str) -> String {
        let widths: Vec<usize> = (0..depth).map(|i| 3 + i % 2).collect();
        let mut out = String::new();
        for width in &widths {
            out.push_str(&":".repeat(*width));
            out.push(' ');
            out.push_str(metadata);
            out.push('\n');
        }
        out.push_str("body\n");
        for width in widths.iter().rev() {
            out.push_str(&":".repeat(*width));
            out.push('\n');
        }
        out
    }

    fn recovered(depth: usize) -> String {
        ladder(depth, "note Bare title!")
    }

    fn valid(depth: usize) -> String {
        ladder(depth, "noteaaaaaaaaaaaa")
    }

    /// The ladder has to reach the depth it claims, in both spellings, or every
    /// count below is a count of something else.
    #[test]
    fn both_ladders_nest_to_their_depth() {
        for depth in [100usize, 200] {
            assert_eq!(recovered(depth).len(), valid(depth).len());
            assert_eq!(count(recovered(depth)).nested, depth);
            assert_eq!(count(valid(depth)).nested, depth);
        }
    }

    /// RECOVERY MUST COST WHAT A VALID OPENER COSTS, to the reading.
    ///
    /// This is the assertion that pins the ruling rather than the parser: the
    /// two ladders nest alike and differ only in whether §3 accepts the
    /// metadata, so any work the recovery path adds per level shows up here as
    /// an inequality. It is what carve-php#2817 would have failed - its
    /// recovery branch called two full-run helpers once per level, which no
    /// valid opener paid for.
    #[test]
    fn a_recovered_opener_costs_no_more_readings_than_a_valid_one() {
        for depth in [100usize, 200] {
            let rec = count(recovered(depth));
            let val = count(valid(depth));
            assert!(
                rec.readings > 0 && val.readings > 0,
                "the opener-reading counter is dead at depth {depth}",
            );
            assert_eq!(
                rec.readings, val.readings,
                "depth {depth}: a recovered container cost {} opener readings against {} for a \
                 valid one - the recovery path is doing work per level",
                rec.readings, val.readings,
            );
        }
    }

    /// Only the recovered ladder RECOVERS a container, so the guard above is
    /// measuring the construct it is named for and not two spellings of the
    /// same parse.
    ///
    /// The valid ladder still ENTERS the recovery branch - its `body` line is
    /// no opener by either reading - so the claim is on what the branch
    /// accepts, not on whether it ran.
    #[test]
    fn only_the_recovered_ladder_recovers_a_container() {
        let rec = count(recovered(200));
        let val = count(valid(200));
        assert!(
            rec.recoveries >= 200,
            "the recovered ladder recovered {} containers at depth 200",
            rec.recoveries,
        );
        assert_eq!(
            val.recoveries, 0,
            "the valid ladder recovered {} containers - its openers are supposed to parse",
            val.recoveries,
        );
    }

    /// A recovered ladder, every opener carrying a metadata tail of `tail`
    /// characters after its kind.
    ///
    /// The tail is what separates a BOUNDED PREFIX from a full-line walk. The
    /// ladder's own lines are the same length at every depth, so varying the
    /// tail varies only how far past the kind there is to read - and a gate
    /// that answers from the prefix cannot see it at all.
    fn tailed(depth: usize, tail: usize) -> String {
        ladder(depth, &format!("note {}", "x".repeat(tail)))
    }

    /// A RECOVERY READING MUST ANSWER FROM A BOUNDED PREFIX.
    ///
    /// The three scans the branch makes are a colon run, the space run after
    /// it, and the kind. Each is a prefix, so the characters per reading hold
    /// flat however much metadata follows. A gate that walked the whole run to
    /// answer a bounded question - carve-php#2817's two helpers, which read a
    /// full indentation run to compare three columns - makes this climb with
    /// the run it walks.
    ///
    /// Quadrupling the tail is the axis rather than deepening the ladder,
    /// because carve-rs nests containers FLUSH LEFT: a body line that starts
    /// with a space opens nothing, so an opener on the nesting path has no
    /// indentation run to re-walk and depth alone cannot express this claim.
    #[test]
    fn a_recovery_reading_reads_a_bounded_prefix() {
        let small = count(tailed(100, 32));
        let large = count(tailed(100, 128));
        let small_per = small.recovery_chars as f64 / small.readings as f64;
        let large_per = large.recovery_chars as f64 / large.readings as f64;
        assert!(
            small.recovery_chars > 0 && large.recovery_chars > 0,
            "the recovery-character counter is dead: {} and {}",
            small.recovery_chars,
            large.recovery_chars,
        );
        assert!(
            large.recovery_chars * small.readings * 10
                <= small.recovery_chars * large.readings * 11,
            "the recovery characters per reading grew with the metadata tail \
             ({small_per:.2} at 32 characters, {large_per:.2} at 128): a recovery gate is \
             walking the run instead of reading its prefix",
        );
    }

    /// Shared colon boundaries keep opener readings proportional to input lines.
    #[test]
    fn opener_readings_stay_proportional_to_the_nesting_walk() {
        let work = |depth: u64| depth;
        let small = count(recovered(100));
        let large = count(recovered(200));
        let small_work = work(100);
        let large_work = work(200);
        assert!(
            small.readings >= 100 && large.readings >= 200,
            "the opener-reading counter is not counting: {} readings at depth 100, {} at 200",
            small.readings,
            large.readings,
        );
        assert!(
            large.readings <= 5 * large_work,
            "{} opener readings for {large_work} units of nesting work ({:.2} each) - \
             an opener is being read more than five times per level",
            large.readings,
            large.readings as f64 / large_work as f64,
        );
        assert!(
            large.readings * small_work * 10 <= small.readings * large_work * 11,
            "the per-unit opener-reading cost climbs with depth ({:.3} at depth 100, {:.3} at \
             200): a recovered container is being re-read per level",
            small.readings as f64 / small_work as f64,
            large.readings as f64 / large_work as f64,
        );
    }
}

#[cfg(test)]
mod ascii_heading_id_tests {
    use super::{
        de_typography, sanitize_id_source, slugify_parse, AsciiHeadingIds, HeadingIdOptions,
    };

    #[test]
    fn ascii_heading_ids_keep_separator_number_and_case_rules_in_every_mode() {
        let ascii: String = (0..=127).map(char::from).collect();
        assert_eq!(sanitize_id_source(&ascii), ascii);
        assert_eq!(de_typography(&ascii), ascii);
        for ascii in [
            AsciiHeadingIds::Off,
            AsciiHeadingIds::Fold,
            AsciiHeadingIds::Strict,
        ] {
            for lowercase in [false, true] {
                let options = HeadingIdOptions { ascii, lowercase };
                for (source, expected) in [
                    ("  ASCII Title... ", "ASCII-Title"),
                    ("A\0B\tC\nD", "A-B-C-D"),
                    ("123 & punctuation", "s-123-punctuation"),
                    ("", "s"),
                    ("---", "s"),
                    ("A Z", "A-Z"),
                ] {
                    let expected = if lowercase {
                        expected.to_ascii_lowercase()
                    } else {
                        expected.into()
                    };
                    assert_eq!(
                        slugify_parse(source, options),
                        expected,
                        "{source:?}: {options:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn paired_quote_glyphs_reverse_before_slugging() {
        let options = HeadingIdOptions::default();
        for source in ["q ‘2 active’", "q ‚2 active‛"] {
            assert_eq!(slugify_parse(source, options), "q-2-active");
        }
        for source in [
            "q “2 active”",
            "q „2 active“",
            "q ‟2 active‟",
            "q «2 active»",
        ] {
            assert_eq!(slugify_parse(source, options), "q-2-active");
        }
    }
}

#[cfg(test)]
mod wrapped_attrs_work_tests {
    use super::*;

    #[test]
    fn unclosed_attribute_lines_do_not_repeatedly_copy_the_paragraph_tail() {
        for prefix in ["", "> ", "- "] {
            for suffix in ["", "}\n", "\n}\n", "\n{#valid\n.class}\ntext\n"] {
                let source = format!("{prefix}{{abcd\n").repeat(8000) + suffix;
                ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(0));
                let doc = parse(&source);
                assert!(!doc.children.is_empty());
                let work = ATTRS_LOOKAHEAD_BYTES.with(Cell::get);
                assert!(
                    work <= source.len() * 3,
                    "copied {work} bytes for {} source bytes",
                    source.len()
                );
            }
        }
    }

    #[test]
    fn container_collectors_stop_before_the_next_invalid_fragment() {
        for source in [
            "> {abcd\n".repeat(8000) + "> }\n",
            String::from("- x\n") + &"  {abcd\n".repeat(8000) + "  }\n",
        ] {
            ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(0));
            let doc = parse(&source);
            assert!(!doc.children.is_empty());
            let work = ATTRS_LOOKAHEAD_BYTES.with(Cell::get);
            assert!(
                work <= source.len() * 8,
                "read {work} bytes for {} source bytes",
                source.len()
            );
        }
    }

    #[test]
    fn a_valid_three_line_block_attaches_after_a_rejected_opener_in_each_container() {
        for source in [
            "{abcd\n{#valid\n  .class\nrole=note}\ntext\n",
            "> {abcd\n> {#valid\n>   .class\n> role=note}\n> text\n",
            "- {abcd\n  {#valid\n    .class\n  role=note}\n  text\n",
        ] {
            let html = crate::to_html(source);
            assert!(
                html.contains("id=\"valid\" class=\"class\" role=\"note\""),
                "{html}"
            );
            assert!(html.contains("{abcd"), "{html}");
        }
    }

    #[test]
    fn a_later_wrapped_attribute_block_keeps_its_closer() {
        let doc = parse("{abcd\n\n{#valid\n.class}\ntext\n");
        assert_eq!(
            plain_inlines_parse(match &doc.children[0] {
                BlockNode::Paragraph(p) => &p.children,
                _ => panic!("unclosed opener must remain a paragraph"),
            }),
            "{abcd"
        );
        let BlockNode::Paragraph(p) = &doc.children[1] else {
            panic!("expected attributed paragraph")
        };
        let attrs = p.attrs.as_ref().expect("wrapped block survives");
        assert_eq!(attrs.id.as_deref(), Some("valid"));
        assert_eq!(attrs.classes, ["class"]);
    }
}

#[cfg(test)]
mod shared_colon_work_tests {
    use super::*;

    #[test]
    fn rejected_fence_carry_scans_only_new_chunks() {
        for positions in [false, true] {
            for retained in [false, true] {
                for count in [64, 256, 1024] {
                    let source = format!(
                        "- a\n    ~~~\n{}",
                        format!(
                            "  ````\n  code\n  ````\n{}after\n",
                            if retained { "  " } else { "" }
                        )
                        .repeat(count)
                    );
                    source_map::MAPPED_SOURCE_SCAN_BYTES.with(|work| work.set(0));
                    CODE_INDEX_LINES.with(|work| work.set(0));
                    let html = crate::to_html_with_options(
                        &source,
                        &Options {
                            positions,
                            ..Options::default()
                        },
                    );
                    // Rule A (carve#3885): an indented fence after `after` opens at
                    // the document level, so every repetition is a code block. With
                    // `after` retained in the item, the item's unclosed `~~~` holds
                    // every repetition (carve#4018).
                    let blocks = if retained { 1 } else { count };
                    assert_eq!(html.matches("<pre>").count(), blocks);
                    assert_eq!(html.matches("after").count(), count);
                    assert!(
                        source_map::MAPPED_SOURCE_SCAN_BYTES.with(Cell::get) <= source.len() * 64,
                        "count={count}, positions={positions}, scan={}, bytes={}",
                        source_map::MAPPED_SOURCE_SCAN_BYTES.with(Cell::get),
                        source.len()
                    );
                    assert!(CODE_INDEX_LINES.with(Cell::get) <= source.lines().count() * 64);
                }
            }
        }
    }

    #[test]
    fn quoted_anonymous_body_comments_share_their_index() {
        for level in 0..=2 {
            for count in [64, 512, 4096] {
                let prefix = "> ".repeat(level);
                let mut body = LineBuffer::default();
                for _ in 0..count {
                    for line in ["- :::", "%%%", "comment", "%%%", "  :::"] {
                        body.push_at(format!("{prefix}{line}"), None, None);
                    }
                }
                let mut comments = QuotedBodyComments::default();
                COMMENT_INDEX_LINES.with(|work| work.set(0));
                for from in (0..body.lines.len()).step_by(5) {
                    assert!(quoted_colon_body_follows(
                        &body.lines,
                        from,
                        level,
                        2,
                        &mut comments,
                    ));
                }
                assert!(COMMENT_INDEX_LINES.with(Cell::get) <= body.lines.len());
            }
        }
    }

    #[test]
    fn quoted_body_comment_indices_stop_at_their_quote_scope() {
        let mut body = LineBuffer::default();
        for _ in 0..4096 {
            body.push_at("> prelude".to_owned(), None, None);
        }
        let mut groups = Vec::new();
        for level in 2..=32 {
            let prefix = "> ".repeat(level);
            groups.push((level, body.lines.len()));
            body.push_at(format!("{prefix}%%%"), None, None);
            body.push_at(format!("{prefix}%%%"), None, None);
            body.push_at("departure".to_owned(), None, None);
        }
        COMMENT_INDEX_LINES.with(|work| work.set(0));
        for (level, from) in groups {
            assert!(QuotedBodyComments::default().closes_after(&body.lines, from, level, 3));
        }
        assert!(COMMENT_INDEX_LINES.with(Cell::get) <= 32 * 2);
    }

    #[test]
    fn quoted_body_comment_index_reenters_and_can_rewind() {
        let mut body = LineBuffer::default();
        for line in ["> %%%", "> %%%", "departure", "> %%%%", "> %%%%"] {
            body.push_at(line.to_owned(), None, None);
        }
        let mut comments = QuotedBodyComments::default();
        assert!(comments.closes_after(&body.lines, 0, 1, 3));
        assert!(comments.closes_after(&body.lines, 3, 1, 4));
        assert!(!comments.closes_after(&body.lines, 3, 1, 3));
        assert!(comments.closes_after(&body.lines, 0, 1, 3));
    }

    #[test]
    fn literal_code_index_reenters_a_later_quote_scope() {
        let mut body = LineBuffer::default();
        for line in [
            ">   ``` info",
            ">   ```",
            "departure",
            ">   ~~~ info",
            ">   ~~~",
        ] {
            body.push_at(line.to_owned(), None, None);
        }
        let mut lookup = LiteralCodeLookup::default();
        assert!(literal_code_closes(
            &body.lines,
            0,
            1,
            2,
            2,
            detect_fence_open("``` info").unwrap(),
            &mut lookup
        ));
        assert!(literal_code_closes(
            &body.lines,
            3,
            1,
            2,
            2,
            detect_fence_open("~~~ info").unwrap(),
            &mut lookup
        ));
        assert!(!literal_code_closes(
            &body.lines,
            3,
            1,
            2,
            2,
            detect_fence_open("``` info").unwrap(),
            &mut lookup
        ));
    }

    #[test]
    fn literal_code_indices_do_not_scan_unrelated_quote_scopes() {
        let mut body = LineBuffer::default();
        for _ in 0..4096 {
            body.push_at("> prelude".to_owned(), None, None);
        }
        let mut groups = Vec::new();
        for level in 2..=32 {
            let prefix = "> ".repeat(level);
            groups.push((level, body.lines.len()));
            body.push_at(format!("{prefix}  ``` info"), None, None);
            body.push_at(format!("{prefix}  ```"), None, None);
            body.push_at("departure".to_owned(), None, None);
        }
        let mut lookup = LiteralCodeLookup::default();
        let open = detect_fence_open("``` info").unwrap();
        LITERAL_CODE_LINES.with(|work| work.set(0));
        for (level, from) in groups {
            assert!(literal_code_closes(
                &body.lines,
                from,
                level,
                2,
                2,
                open,
                &mut lookup
            ));
        }
        assert!(LITERAL_CODE_LINES.with(Cell::get) <= 32 * 3);
    }

    #[test]
    fn literal_code_columns_store_one_record_per_run() {
        let mut column = LiteralCodeColumn::default();
        for index in 0..4096 {
            column.push(index * 2, 1_000_000 - index);
        }
        assert_eq!(column.positions.len(), 4096);
        assert!(column.maxima.len() <= column.positions.len() * 4);
        assert_eq!(column.next(0, 999_999), Some(2));
        assert_eq!(column.next(2, 999_999), None);
        assert_eq!(column.next(8190, 1), None);
    }

    #[test]
    fn literal_code_scope_misses_share_the_departure_bound() {
        for count in [64, 512, 4096] {
            let mut body = LineBuffer::default();
            for _ in 0..count {
                body.push_at("  ``` info".to_string(), None, None);
            }
            body.push_at("- sibling".to_string(), None, None);
            body.push_at("  ```".to_string(), None, None);
            let mut lookup = LiteralCodeLookup::default();
            let open = detect_fence_open("``` info").unwrap();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for from in 0..count {
                assert!(!literal_code_closes(
                    &body.lines,
                    from,
                    0,
                    2,
                    2,
                    open,
                    &mut lookup
                ));
            }
            assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len() * 2);
        }
    }

    #[test]
    fn literal_code_misses_refute_varied_unclosed_runs() {
        for level in 0..=2 {
            for count in [64, 512, 4096] {
                let prefix = "> ".repeat(level);
                let mut body = LineBuffer::default();
                for index in 0..count {
                    body.push_at(
                        format!("{prefix}    {} info", "`".repeat(3 + index % 8)),
                        None,
                        None,
                    );
                }
                body.push_at(format!("{prefix}   ``````````"), None, None);
                let mut misses = LiteralCodeLookup::default();
                LITERAL_CODE_LINES.with(|work| work.set(0));
                for index in 0..count {
                    let open =
                        detect_fence_open(&format!("{} info", "`".repeat(3 + index % 8))).unwrap();
                    assert!(!literal_code_closes(
                        &body.lines,
                        index,
                        level,
                        4,
                        4,
                        open,
                        &mut misses
                    ));
                }
                assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len());
            }
        }
    }

    #[test]
    fn literal_code_descending_bare_runs_share_the_index() {
        for count in [64, 512] {
            let mut body = LineBuffer::default();
            for index in 0..count {
                body.push_at(format!("    {}", "`".repeat(count - index + 3)), None, None);
            }
            let mut misses = LiteralCodeLookup::default();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for index in 0..count {
                let open = detect_fence_open(&"`".repeat(count - index + 3)).unwrap();
                assert!(!literal_code_closes(
                    &body.lines,
                    index,
                    0,
                    4,
                    4,
                    open,
                    &mut misses
                ));
            }
            assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len());
        }
    }

    #[test]
    fn literal_code_cached_index_answers_smaller_widths() {
        let mut body = LineBuffer::default();
        for line in [
            "    ```` info",
            "    ``` info",
            "    ```",
            "- sibling",
            "    ```",
            "    ```",
        ] {
            body.push_at(line.to_owned(), None, None);
        }
        let mut misses = LiteralCodeLookup::default();
        assert!(!literal_code_closes(
            &body.lines,
            0,
            0,
            4,
            4,
            detect_fence_open("```` info").unwrap(),
            &mut misses
        ));
        assert!(literal_code_closes(
            &body.lines,
            1,
            0,
            4,
            4,
            detect_fence_open("``` info").unwrap(),
            &mut misses
        ));
        assert!(literal_code_closes(
            &body.lines,
            4,
            0,
            4,
            4,
            detect_fence_open("```").unwrap(),
            &mut misses
        ));
    }

    #[test]
    fn literal_code_varied_columns_and_growing_buffers_are_indexed_once() {
        for count in [64, 512] {
            let mut body = LineBuffer::default();
            for index in 0..count {
                body.push_at(format!("{}``` info", " ".repeat(index + 3)), None, None);
            }
            body.push_at(" ```".to_owned(), None, None);
            let mut lookup = LiteralCodeLookup::default();
            let open = detect_fence_open("``` info").unwrap();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for index in 0..count {
                assert!(!literal_code_closes(
                    &body.lines,
                    index,
                    0,
                    2,
                    index + 3,
                    open,
                    &mut lookup
                ));
            }
            assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len());

            let mut growing = LineBuffer::default();
            let mut lookup = LiteralCodeLookup::default();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for index in 0..count {
                growing.push_at("    ``` info".to_owned(), None, None);
                assert!(!literal_code_closes(
                    &growing.lines,
                    index,
                    0,
                    4,
                    4,
                    open,
                    &mut lookup
                ));
            }
            assert_eq!(LITERAL_CODE_LINES.with(Cell::get), 0);
        }
    }

    #[test]
    fn literal_code_closes_at_any_owned_payload_column() {
        let open = detect_fence_open("``` info").unwrap();
        for (closer, expected) in [
            ("  ```", true),
            ("    ```", true),
            ("   ```", true),
            (" ```", false),
        ] {
            let mut body = LineBuffer::default();
            for line in ["    ``` info", closer] {
                body.push_at(line.to_owned(), None, None);
            }
            assert_eq!(
                literal_code_closes(
                    &body.lines,
                    0,
                    0,
                    2,
                    4,
                    open,
                    &mut LiteralCodeLookup::default()
                ),
                expected
            );
        }
    }

    #[test]
    fn repeated_attachment_probes_share_code_and_comment_indices() {
        for count in [64, 512, 4096] {
            let unit = [
                "::: |", "```", "code", "```", "%%%", "comment", "%%%", ":::", "+",
            ];
            let lines: Vec<_> = (0..count).flat_map(|_| unit).collect();
            let mut code = None;
            let mut comments = None;
            CODE_INDEX_LINES.with(|work| work.set(0));
            COMMENT_INDEX_LINES.with(|work| work.set(0));
            for start in (0..lines.len()).step_by(unit.len()) {
                assert_eq!(
                    attached_block_end(
                        CursorLines::Slice(&lines),
                        start,
                        &mut comments,
                        &mut code,
                        &mut |line, _| line == "+"
                    ),
                    start + unit.len() - 1
                );
            }
            assert_eq!(CODE_INDEX_LINES.with(Cell::get), lines.len());
            assert_eq!(COMMENT_INDEX_LINES.with(Cell::get), lines.len());
        }
    }

    #[test]
    fn repeated_verse_comment_probes_share_their_index() {
        for count in [64, 512, 4096] {
            let source = "::: |\n%%%\nx\n%%%\n:::\n".repeat(count);
            let lines = source.split('\n').count();
            COMMENT_INDEX_LINES.with(|work| work.set(0));
            assert_eq!(colon_fences_left_open(&source), 0);
            assert!(COMMENT_INDEX_LINES.with(Cell::get) <= lines);
            let refs: Vec<_> = source.lines().collect();
            COMMENT_INDEX_LINES.with(|work| work.set(0));
            assert_eq!(
                attached_block_end(
                    CursorLines::Slice(&refs),
                    0,
                    &mut None,
                    &mut None,
                    &mut |_, _| false
                ),
                refs.len()
            );
            assert!(COMMENT_INDEX_LINES.with(Cell::get) <= lines);
        }
    }

    #[test]
    fn nested_bodies_copy_the_payload_once_with_and_without_positions() {
        for positions in [false, true] {
            for depth in [20, 80, 150] {
                let mut source: String = (0..depth)
                    .map(|i| ["::: box\n", "::: >\n", "::: \\\n"][i % 3])
                    .collect();
                source.push_str(&"payload ".repeat(8000));
                source.push('\n');
                source.push_str(&":::\n".repeat(depth));
                COLON_COPIED_BYTES.with(|count| count.set(0));
                let doc =
                    parse_with_options(&source, &Options::default().with_positions(positions));
                let copied = COLON_COPIED_BYTES.with(Cell::get);
                assert!(
                    copied <= source.len() * 2,
                    "{depth} levels copied {copied} bytes for {} source bytes",
                    source.len()
                );
                assert!(crate::render_html(&doc)
                    .unwrap()
                    .contains("payload payload"));
            }
        }
    }
    #[test]
    fn nested_colon_line_references_are_materialized_only_where_needed() {
        for positions in [false, true] {
            for depth in [24, 96, 192] {
                for prefix in [
                    "::: box\ntext\n\n",
                    "::: box\n- item\n\n",
                    "::: box\n```text\ncode\n```\n\n",
                    "::: box\n%%%\ncomment\n%%%\n\n",
                    "::: box\n::: |\nverse\n:::\n\n",
                    "::: box\n> text\n> ```text\n> code\n> ```\n\n",
                    "::: box\n- item\n+\n# attached\n\n",
                    "::: box\n- item\n+\n```text\ncode\n```\n\n",
                    "::: box\n- item\n+\n%%%\ncomment\n%%%\n\n",
                    "::: box\nterm\n:  +\n# attached\n\n",
                ] {
                    for closed in [true, false] {
                        for payload in [
                            "payload\n".repeat(500),
                            format!("``` =html\n<pre>\n{}</pre>\n```\n", "payload\n".repeat(500)),
                        ] {
                            let source = format!(
                                "{}{}{}",
                                prefix.repeat(depth),
                                payload,
                                if closed {
                                    ":::\n".repeat(depth)
                                } else {
                                    String::new()
                                }
                            );
                            COLON_REFERENCE_ENTRIES.with(|count| count.set(0));
                            COLON_CLOSER_LINES.with(|count| count.set(0));
                            let document = parse_with_options(
                                &source,
                                &Options::default().with_positions(positions),
                            );
                            let entries = COLON_REFERENCE_ENTRIES.with(Cell::get);
                            assert!(
                                COLON_CLOSER_LINES.with(Cell::get) <= source.lines().count() * 2
                            );
                            assert!(
                                entries <= source.lines().count() * 2,
                                "{depth} levels, prefix {prefix:?}, closed {closed}: \
                                 materialized {entries} references"
                            );
                            assert!(crate::render_html(&document).unwrap().contains("payload"));
                            if depth == 24 {
                                COLON_CACHE_ENABLED.with(|flag| flag.set(false));
                                let reference = parse_with_options(
                                    &source,
                                    &Options::default().with_positions(positions),
                                );
                                COLON_CACHE_ENABLED.with(|flag| flag.set(true));
                                assert_eq!(
                                    crate::ast_json::to_json(&document),
                                    crate::ast_json::to_json(&reference)
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cached_boundaries_match_fresh_collection_on_adversarial_bodies() {
        let mut cases = vec![
            "::: outer\n::: inner\npayload\n".to_owned(),
            "::: outer\n::: inner\n```text\n:::\n```\n:::\n:::\n".to_owned(),
            "::: outer\n::: inner\n%%%\n:::\n%%%\n:::\n:::\n".to_owned(),
            "::: outer\n::: |\n::: inner\nverse\n:::\n:::\n:::\n".to_owned(),
            "::: outer\n::: >\n::: \\\nfirst\nsecond\n:::\n:::\n:::\n".to_owned(),
            "::: outer\n::: figure\n![map](map.svg)\n:::\n^ caption\n:::\n".to_owned(),
            "::: outer\n:::: inner\n:::\npayload\n::::\n:::\n".to_owned(),
            "::: outer\n- item\n+\n  ```text\n  x\n  ```\n:::\n".to_owned(),
            "::: box\n- item\n+\n::: note\n  ```\n:::\n  ```\n:::\n:::\n".to_owned(),
        ];
        cases.push(format!(
            "{}payload\n{}",
            "::: box\n".repeat(205),
            ":::\n".repeat(205)
        ));
        cases.push("::: outer\n- item\n  ::: inner\n  payload\n  :::\n:::\n".to_owned());
        for (positions, source_lines) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            for source in &cases {
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines);
                COLON_CACHE_ENABLED.with(|flag| flag.set(false));
                let reference = parse_with_options(source, &options);
                COLON_CACHE_ENABLED.with(|flag| flag.set(true));
                let cached = parse_with_options(source, &options);
                assert_eq!(
                    crate::ast_json::to_json(&reference),
                    crate::ast_json::to_json(&cached),
                    "{source}"
                );
                assert_eq!(
                    crate::render_html(&reference).unwrap(),
                    crate::render_html(&cached).unwrap(),
                    "{source}"
                );
            }
        }
    }
}

#[cfg(test)]
mod nested_list_indent_work {
    //! COUNTED guard on the nested-list indentation walk (carve-rs#2301).
    //!
    //! Counts, not durations: a count is a property of the algorithm and
    //! reproduces on any machine under any load, while a duration describes the
    //! machine that took it.

    use super::{BODY_LINES_COLLECTED, INDENT_BYTES_WALKED, MAX_NESTING_DEPTH};

    /// A `depth`-level ladder of `- x`, then `siblings` items at the deepest level.
    fn ladder(depth: usize, siblings: usize) -> String {
        let mut out = String::new();
        for level in 0..depth {
            out.push_str(&"  ".repeat(level));
            out.push_str("- x\n");
        }
        for _ in 0..siblings {
            out.push_str(&"  ".repeat(depth));
            out.push_str("- y\n");
        }
        out
    }

    /// `(indent bytes walked, body lines collected)` over one parse of `src`, on
    /// its own deep-stacked thread so the tallies cannot pick up another test's.
    fn work_for(src: String) -> (u64, u64) {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                INDENT_BYTES_WALKED.with(|c| c.set(0));
                BODY_LINES_COLLECTED.with(|c| c.set(0));
                let _ = crate::parse(&src);
                (
                    INDENT_BYTES_WALKED.with(|c| c.get()),
                    BODY_LINES_COLLECTED.with(|c| c.get()),
                )
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// Work charged to ONE added sibling line at `depth`, isolating the ladder.
    fn per_sibling(depth: usize, siblings: usize) -> (f64, f64) {
        let (bare_bytes, bare_lines) = work_for(ladder(depth, 0));
        let (full_bytes, full_lines) = work_for(ladder(depth, siblings));
        (
            (full_bytes as f64 - bare_bytes as f64) / siblings as f64,
            (full_lines as f64 - bare_lines as f64) / siblings as f64,
        )
    }

    /// The indent walking a sibling line costs is bounded by the square of its
    /// depth, with a ceiling the pre-fix parser could not meet: it charged
    /// about 11.1 bytes per depth squared, and this allows 6.
    #[test]
    fn a_deep_sibling_line_walks_a_bounded_multiple_of_its_depth_squared() {
        for depth in [50usize, 100, 200] {
            let (bytes, _) = per_sibling(depth, 100);
            let ratio = bytes / (depth * depth) as f64;
            assert!(
                ratio <= 6.0,
                "depth {depth}: {bytes:.0} indent bytes per sibling line is {ratio:.2} per \
                 depth squared, over the ceiling of 6"
            );
        }
    }

    /// The re-collection count per line is the enclosing-item count and no
    /// more, so nothing above it multiplies the work again.
    #[test]
    fn a_sibling_line_is_collected_once_per_enclosing_item() {
        for depth in [50usize, 100, 200] {
            let (_, collections) = per_sibling(depth, 100);
            let cap = depth.min(MAX_NESTING_DEPTH) as f64;
            assert!(
                collections <= cap,
                "depth {depth}: {collections:.1} collections per sibling line, over {cap}"
            );
        }
    }

    /// At a FIXED depth the cost is linear in the line count: the work charged
    /// to one sibling line does not depend on how many siblings there are.
    #[test]
    fn at_a_fixed_depth_the_cost_per_line_is_flat() {
        let (hundred, _) = per_sibling(150, 100);
        let (three_hundred, _) = per_sibling(150, 300);
        if hundred == 0.0 && three_hundred == 0.0 {
            // The experimental event route can parse both controls without
            // entering the instrumented general list reader.
            return;
        }
        let ratio = three_hundred / hundred;
        assert!(
            (0.95..=1.05).contains(&ratio),
            "per-line indent work moved by {ratio:.3}x between 100 and 300 sibling lines \
             ({hundred:.0} against {three_hundred:.0} bytes)"
        );
    }

    #[test]
    #[ignore = "a measurement, not a guard"]
    fn measure() {
        for depth in [25usize, 50, 100, 150, 199, 200, 201, 250] {
            let (bytes, collections) = per_sibling(depth, 100);
            println!(
                "depth {depth}: {bytes:.0} indent bytes ({:.2} per depth squared), \
                 {collections:.1} collections per sibling line",
                bytes / (depth * depth) as f64,
            );
        }
    }
}

#[cfg(test)]
mod nested_list_ladder_work {
    //! COUNTED guard on a list ladder deeper than the nesting cap.
    //!
    //! This replaces `perf_regressions::deeply_nested_list_parse_is_bounded`,
    //! which asserted a 10-second wall clock. An absolute duration describes
    //! the machine that chose it: the same test passed on an idle box and
    //! failed at load 42 (12.5 s), so it could not tell a regression from a
    //! busy runner, and its own doc comment recorded the cap being widened from
    //! 2 s to 10 s for exactly that reason. The intent is kept and the
    //! stopwatch is dropped, following `nested_list_indent_work` (#2324).
    //!
    //! The intent was that the nesting cap keeps a deep ladder's
    //! collect-and-reparse from blowing up per level. That is a relationship
    //! and it is stated as one here: past the cap, one more level costs one
    //! more collection per enclosing item and no more, which is the cap, and
    //! the indent bytes that level's own line costs stay a bounded multiple of
    //! its length times the cap. An uncapped per-level rescan fails both.
    //!
    //! ONE parse per depth, all relationships from the same three tallies: a
    //! 300-level ladder is the expensive part, and asserting each relationship
    //! in its own test would parse it three times over.

    use super::{BODY_LINES_COLLECTED, INDENT_BYTES_WALKED, MAX_NESTING_DEPTH};

    /// A `depth`-level ladder of `- x`, each level one column deeper.
    fn ladder(depth: usize) -> String {
        let mut out = String::new();
        for level in 0..depth {
            out.push_str(&"  ".repeat(level));
            out.push_str("- x\n");
        }
        out
    }

    /// `(indent bytes walked, body lines collected)` over one parse, on its own
    /// deep-stacked thread so the tallies cannot pick up another test's.
    fn work_for(depth: usize) -> (f64, f64) {
        let src = ladder(depth);
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                INDENT_BYTES_WALKED.with(|c| c.set(0));
                BODY_LINES_COLLECTED.with(|c| c.set(0));
                let doc = crate::parse(&src);
                assert!(
                    matches!(doc.children.first(), Some(crate::BlockNode::List(_))),
                    "the ladder must actually parse into a list, or the tallies below \
                     are measuring a parse that did nothing"
                );
                (
                    INDENT_BYTES_WALKED.with(|c| c.get()) as f64,
                    BODY_LINES_COLLECTED.with(|c| c.get()) as f64,
                )
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// Indent bytes a level's own line may cost: its length grows with its
    /// depth and it is re-read once per enclosing item, which the cap bounds.
    ///
    /// CALIBRATED AGAINST THE REGRESSION IT GUARDS, not against headroom alone.
    /// Undoing #2324's reuse (the `_at` helpers re-measuring the indent they
    /// were handed) takes the two pairs below from 2.25 and 2.58 to 6.16 and
    /// 7.06, so this sits between them: 55% over the measurement and 35% under
    /// the regression. A looser ceiling of 9 was tried first and could not see
    /// that regression at all, which is the thing a guard has to do.
    const BYTES_PER_LEVEL_LENGTH_PER_ENCLOSING_ITEM: f64 = 4.0;

    #[test]
    fn a_ladder_past_the_cap_pays_the_cap_per_added_level_and_no_more() {
        let cap = MAX_NESTING_DEPTH as f64;
        let at_the_cap = work_for(MAX_NESTING_DEPTH);
        let past = work_for(MAX_NESTING_DEPTH + 50);
        let further = work_for(MAX_NESTING_DEPTH + 100);

        for (name, shallow, deep, levels) in [
            ("at the cap to 50 past it", at_the_cap, past, 50.0),
            ("50 past the cap to 100 past it", past, further, 50.0),
        ] {
            // ONE MORE COLLECTION PER ENCLOSING ITEM. The recursion stops at the
            // cap, so a level added past it is re-collected by the items above
            // it and by nothing more. Below the cap this count is the depth
            // itself and grows; past it, it is flat at the cap. An uncapped
            // per-level rescan keeps growing here.
            let per_level_collections = (deep.1 - shallow.1) / levels;
            assert!(
                per_level_collections <= cap,
                "{name}: {per_level_collections:.1} collections per added level, over the \
                 cap of {cap}"
            );
            // AND ITS INDENT IS WALKED A BOUNDED NUMBER OF TIMES. The line's own
            // length is what grows with depth; the number of walks over it is
            // what a regression multiplies (#2324).
            let per_level_bytes = (deep.0 - shallow.0) / levels;
            let line_length = levels.mul_add(0.5, MAX_NESTING_DEPTH as f64) * 2.0;
            let ratio = per_level_bytes / (line_length * cap);
            assert!(
                ratio <= BYTES_PER_LEVEL_LENGTH_PER_ENCLOSING_ITEM,
                "{name}: {per_level_bytes:.0} indent bytes per added level is {ratio:.2} per \
                 line byte per enclosing item, over the ceiling of \
                 {BYTES_PER_LEVEL_LENGTH_PER_ENCLOSING_ITEM}"
            );
        }

        // AND THE TOTAL IS LINEAR IN THE LADDER'S LINES, with the cap as the
        // constant. The quadratic an uncapped rescan produces passes the
        // marginal checks above only while the sample is narrow; this one it
        // cannot pass at all.
        let lines = (MAX_NESTING_DEPTH + 100) as f64;
        assert!(
            further.1 <= lines * cap,
            "a {lines}-level ladder collected {:.0} body lines, over the linear bound of {:.0}",
            further.1,
            lines * cap
        );
    }

    #[test]
    #[ignore = "a measurement, not a guard"]
    fn measure() {
        let cap = MAX_NESTING_DEPTH as f64;
        let mut previous: Option<(usize, (f64, f64))> = None;
        for depth in [50usize, 100, 150, 200, 250, 300] {
            let work = work_for(depth);
            if let Some((before, earlier)) = previous {
                let levels = (depth - before) as f64;
                let per_level_bytes = (work.0 - earlier.0) / levels;
                let line_length = (before + depth) as f64;
                println!(
                    "{before:4} to {depth:4}: {per_level_bytes:10.0} indent bytes per level \
                     ({:.2} per line byte per enclosing item), {:.1} collections per level",
                    per_level_bytes / (line_length * cap),
                    (work.1 - earlier.1) / levels,
                );
            }
            previous = Some((depth, work));
        }
    }
}

#[cfg(test)]
mod single_line_open_paragraph_probes {
    use super::*;

    #[test]
    fn deep_single_line_lists_do_not_reparse_their_prefix_chains() {
        for depth in [8, 16, 32] {
            for terminal in [
                "a",
                "*a*",
                "`a`",
                "é",
                "[a](url)",
                "# h",
                "",
                "%% c",
                "{.x}",
                "::: note",
                "[a]: url",
                "[^a]: body",
                "   ",
            ] {
                PARAGRAPH_BODY_PROBES.with(|count| count.set(0));
                let source = format!("{}{terminal}\n", "- > ".repeat(depth));
                let html = crate::to_html(&source);
                assert_eq!(html.matches("<ul>").count(), depth);
                assert_eq!(html.matches("<blockquote>").count(), depth);
                assert_eq!(
                    PARAGRAPH_BODY_PROBES.with(Cell::get),
                    0,
                    "{depth}: {terminal}"
                );
            }
        }
    }

    #[test]
    fn prefix_chains_match_block_verdicts() {
        let options = Options::default();
        for terminal in [
            "a",
            "a *b*",
            "a `b`",
            "a {+b+}",
            "a [b](url)",
            "*a*",
            "`a`",
            "é",
            "^ caption",
            "<tag>",
            "[a](url)",
            "[^a]",
            "[^a]: body",
            "# h",
            "---",
            "```",
            "::: note",
            "%% c",
            "| a |",
            "{.x}",
            "",
            "[a]: url",
            "   ",
            "%%% note",
            ":: term",
        ] {
            for prefix in ["", "- ", "> ", "- > - > ", "> - > - "] {
                let body = format!("{prefix}{terminal}");
                let blocks = probe_blocks(&body, &options);
                let expected = block_ends_with_open_paragraph(
                    blocks.last(),
                    colon_fences_left_open(&body),
                    false,
                );
                assert_eq!(
                    body_ends_with_open_paragraph(&format!("{prefix}{terminal}"), &options),
                    expected,
                    "{prefix}{terminal}"
                );
            }
        }
    }
}

#[cfg(test)]
mod frame_probe_tests {
    use super::*;

    #[test]
    fn definition_free_probe_reduction_retains_existing_normalization() {
        let options = Options::default();
        for source in [
            "",
            "p",
            "p\n- :: t\n  ```\n  x",
            "\u{feff}p",
            "p\r\nq",
            "p\r",
            "p\n",
            "p\n\n",
            " p \t\n q",
            "p [text]",
            "- [x] task",
            "[r]: /u\np",
            "p\rinside",
            "[r]: <b>{=html}",
            "[r]:\t/u",
            "> - [r]: /u",
        ] {
            assert_eq!(
                reduce_frame_probe_links(source.to_owned(), &options),
                extract_link_defs(source, &options).0,
                "{source:?}"
            );
        }
    }
}

#[cfg(test)]
mod definition_gate_tests {
    use super::*;

    #[test]
    fn global_gate_preserves_source_maps_and_guard_budget() {
        let extension = Extension;
        for positions in [false, true] {
            for extended in [false, true] {
                let mut options = Options::default().with_positions(positions);
                if extended {
                    options.extensions.push(&extension);
                }
                for source in [
                    "",
                    "p",
                    "p\n",
                    "p\n\n",
                    "p\r\nq",
                    "\u{feff}p",
                    " p \t\n q",
                    "p [text]: literal",
                    "[r]: /u",
                    "[r]:\t/u\n\n[r][]",
                    "[r]:/u\n\n[r][]",
                    "[r]:\n  /u\n\n[r][]",
                    "\u{feff}[r]: /u\n\n[r][]",
                    "[r]: /u \"title\" {.c}",
                    "[r]: /u junk",
                    "[@r]: /u",
                    "[]: /u",
                    "- [r]: /u",
                    "> - [r]: /u",
                    "  > - : [r]: /u",
                    ":: t\n: [r]: /u",
                    ":: t\n: - [r]: /u",
                    "- a\n  > [r]: /u",
                    "- a\n  [r]: /u",
                    "- a\n   [r]: /u",
                    "- a\n\t[r]: /u",
                    "> - a\n>   [r]: /u",
                    "- - x\n   [r]: /u",
                    "1. a\n   [r]: /u",
                    "-{.c} [r]: /u",
                    ". [r]: /u",
                    "- [x]  - [r]: /u",
                    "- [x] [r]: /u",
                    "- [x] p [r]: /u",
                    "p\n- :: t\n  ```\n  x",
                    "para\n  - :: t\n    ```",
                    "```\n[r]: /u\n```",
                    "> ```\n> [r]: /u\n> ```",
                    "%%%\n[r]: /u\n%%%",
                    "::: |\n[r]: /u\n:::",
                    "- x\n  %%% o\n  filler\n%%%\n\n[r][]\n",
                ] {
                    let mut fast_budget = 1_000_000;
                    let mut full_budget = fast_budget;
                    let fast =
                        extract_link_defs_with_guard(source, &options, Some(&mut fast_budget));
                    let full =
                        extract_link_defs_with_guard_full(source, &options, Some(&mut full_budget));
                    assert!(
                        full.1.is_empty() || source_may_contain_link_definition(source),
                        "{source:?}"
                    );
                    assert_eq!(fast.0, full.0, "{source:?}");
                    let projection = |map: &BTreeMap<String, LinkDef>| {
                        map.iter()
                            .map(|(label, def)| {
                                (
                                    label.clone(),
                                    def.raw_label.clone(),
                                    def.href.clone(),
                                    def.title.clone(),
                                    def.line,
                                    format!("{:?}", def.attrs),
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(projection(&fast.1), projection(&full.1), "{source:?}");
                    assert_eq!(fast_budget, full_budget, "{source:?}");
                }
            }
        }
    }

    #[test]
    fn definition_free_gate_preserves_source_without_scanning_container_bounds() {
        let source = "- x\n  %%% o\n  filler\n%%%\n\n[r][]\n";
        std::thread::spawn(move || {
            let options = Options::default();
            CONTAINER_DEDENT_STEPS.with(|counter| counter.set(0));
            let fast = extract_link_defs(source, &options);
            assert_eq!(CONTAINER_DEDENT_STEPS.with(Cell::get), 0);
            assert!(fast.1.is_empty());
            let full = extract_link_defs_with_guard_full(source, &options, None);
            assert!(CONTAINER_DEDENT_STEPS.with(Cell::get) > 0);
            assert_eq!(fast.0, full.0);
            assert!(full.1.is_empty());
        })
        .join()
        .unwrap();
    }

    struct CountingExtension(Cell<usize>);
    impl crate::extension::CarveExtension for CountingExtension {
        fn name(&self) -> &'static str {
            "definition-gate-callback-control"
        }
        fn match_block(
            &self,
            _lines: &[&str],
            _start: usize,
            _context: &crate::extension::MatcherContext<'_>,
        ) -> Option<crate::extension::BlockMatch> {
            self.0.set(self.0.get() + 1);
            None
        }
    }

    #[test]
    fn definition_gate_retains_extension_matcher_calls() {
        for (source, has_matcher_work) in [
            ("p [text]: literal\n\n::: |\nx\n:::\n", true),
            ("p\n\n::: |\nx\n:::\n", false),
        ] {
            for positions in [false, true] {
                let extension = CountingExtension(Cell::new(0));
                let mut options = Options::default().with_positions(positions);
                options.extensions.push(&extension);
                let full = extract_link_defs_with_guard_full(source, &options, None);
                let full_calls = extension.0.replace(0);
                assert_eq!(full_calls > 0, has_matcher_work);
                let probe = reduce_frame_probe_links(source.to_owned(), &options);
                assert_eq!(probe, full.0);
                assert_eq!(extension.0.replace(0), full_calls);
                let fast = extract_link_defs(source, &options);
                assert_eq!(fast.0, full.0);
                assert_eq!(extension.0.get(), full_calls);
            }
        }
    }

    struct Extension;
    impl crate::extension::CarveExtension for Extension {
        fn name(&self) -> &'static str {
            "global-definition-gate-control"
        }
    }

    #[test]
    fn canonical_prefix_gate_preserves_probe_reduction() {
        let options = Options::default();
        for source in [
            "p [text]: literal",
            "p [text]: literal\n- :: t\n  ```\n  x",
            "[r]: /u",
            " [r]: /u",
            "- [r]: /u",
            "> - [r]: /u",
            "  > - : [r]: /u",
            ">  >   - [r]: /u",
            ":: t\n: [r]: /u",
            ":: t\n: - [r]: /u",
            "- a\n  > [r]: /u",
            "- a\n  > - [r]: /u",
            "[r]: /u \"title\" {.c}",
            "[r]: /u junk",
            "[@r]: /u",
            "[]: /u",
            "\u{feff}[r]: /u",
            "p\r\n[r]: /u",
            "p\r",
            "",
            "p\n",
            "p\n\n",
            "[r]: /u\n",
            "\t[r]: /u",
            "- [x] [r]: /u",
            "- [x] p [r]: /u",
        ] {
            assert_eq!(
                reduce_frame_probe_links(source.to_owned(), &options),
                extract_link_defs(source, &options).0,
                "{source:?}"
            );
        }
        assert!(!source_may_contain_link_definition("p [text]: literal"));
        assert!(source_may_contain_link_definition("  > - : [r]: /u"));
    }
}

#[cfg(test)]
mod quoted_prepass_work_tests {
    use super::*;

    #[test]
    fn literal_angle_text_keeps_root_columns_and_needs_no_quote_memo() {
        let lines = [
            ":: t",
            "",
            "- a",
            "- - b",
            "",
            "    [r]: /x",
            "",
            "x > [y][r]",
        ];
        let maxima = fence_closer_suffix_maxima(&lines);
        let ownership = term_folded_lines(&lines, &maxima, &Options::default());
        assert!(ownership.tracks_quote_context);
        let mut columns = ContentColumns::new();
        let mut previous_non_blank = "";
        for (index, line) in lines.iter().enumerate() {
            assert!(!prepass_quote_line_is_folded(&ownership, index, line));
            columns.observe(line, false);
            let prefix = strip_container_prefixes_at(
                line,
                &columns,
                opens_definition_entry(previous_non_blank),
            );
            remember_prepass_paragraph_line(&ownership, index, line, &prefix, &columns);
            assert!(
                ownership.paragraph_quotes.borrow().is_empty(),
                "line {index}"
            );
            if !is_blank_line(line) {
                previous_non_blank = line;
            }
            if index == 3 {
                assert_eq!(columns.current().cols, [2, 4]);
            }
        }
    }
}

#[cfg(test)]
mod marker_note_comment_bounds {
    use super::*;

    #[test]
    fn note_comment_queries_reuse_the_content_boundary() {
        for depth in 0..=2 {
            for count in [128, 512, 2048] {
                let prefix = "> ".repeat(depth);
                let owned = vec![format!("{prefix}    %%%"); count];
                let lines: Vec<_> = owned.iter().map(String::as_str).collect();
                let mut index = ContainerCommentClosers::build(&lines);
                CONTAINER_DEDENT_STEPS.with(|counter| counter.set(0));
                for at in 0..count - 1 {
                    assert!(index.closes_in_note(&lines, at, depth, 4, 3));
                }
                assert!(CONTAINER_DEDENT_STEPS.with(Cell::get) <= count as u64);
            }
        }
    }

    #[test]
    fn a_note_comment_cannot_close_after_its_content_scope() {
        for depth in 0..=2 {
            let prefix = "> ".repeat(depth);
            for middle in [format!("{prefix}- sibling"), "outside".to_string()] {
                let owned = [
                    format!("{prefix}    %%%"),
                    middle,
                    format!("{prefix}    %%%"),
                ];
                let lines: Vec<_> = owned.iter().map(String::as_str).collect();
                let mut index = ContainerCommentClosers::build(&lines);
                assert!(!index.closes_in_note(&lines, 0, depth, 4, 3));
            }
            let owned = [
                format!("{prefix}    %%%"),
                format!("{prefix}    body"),
                format!("{prefix}    %%%"),
            ];
            let lines: Vec<_> = owned.iter().map(String::as_str).collect();
            assert!(ContainerCommentClosers::build(&lines).closes_in_note(&lines, 0, depth, 4, 3));
        }
    }

    #[test]
    fn unmatched_comment_widths_do_not_scan_note_suffixes() {
        let owned: Vec<_> = (3..259)
            .map(|width| format!("    {}", "%".repeat(width)))
            .collect();
        let lines: Vec<_> = owned.iter().map(String::as_str).collect();
        let mut index = ContainerCommentClosers::build(&lines);
        CONTAINER_DEDENT_STEPS.with(|counter| counter.set(0));
        for (at, width) in (3..259).enumerate() {
            assert!(!index.closes_in_note(&lines, at, 0, 4, width));
        }
        assert_eq!(CONTAINER_DEDENT_STEPS.with(Cell::get), 0);
    }
}

#[cfg(test)]
mod term_marker_probe_work_tests {
    use super::*;

    fn probed_bytes(source: &str) -> usize {
        let lines: Vec<&str> = source.lines().collect();
        let maxima = fence_closer_suffix_maxima(&lines);
        TERM_MARKER_PROBE_BYTES.with(|bytes| bytes.set(0));
        drop(term_folded_lines(&lines, &maxima, &Options::default()));
        TERM_MARKER_PROBE_BYTES.with(std::cell::Cell::get)
    }

    // carve-rs#2690: every marker probe here used to parse the whole document
    // above it, until the probe budget ran out.
    #[test]
    fn a_marker_probe_parses_the_block_holding_its_marker() {
        let unit = "A paragraph that only adds bytes above the next marker, see[^n].\n\n\
                    - ::: |\n  - ```\n- :: term\n    ```\n";
        for separator in ["\n", "\n\n", "\n\n\n"] {
            let source = vec![unit; 200].join(separator);
            let probed = probed_bytes(&source);
            assert!(
                probed > 0,
                "no marker probe ran with separator {separator:?}"
            );
            assert!(
                probed * 16 < source.len(),
                "marker probes parsed {probed} bytes of a {}-byte document with separator \
                 {separator:?}",
                source.len()
            );
        }
    }
}

#[cfg(test)]
mod ladder_body_reparses {
    //! COUNTED guard on a list ladder whose items hold quotes or containers.
    //!
    //! Each item asked whether its body ends in an open paragraph by re-parsing
    //! the body, and a sub-list inside that body asked the same of its own body
    //! inside the probe, so every level parsed what was below it twice: 30
    //! levels took 0.5 s and 40 levels 44 s. The answer is now probed only when
    //! a later line needs it.
    use super::*;

    /// `depth` levels alternating a list item with a quote (`> q` and a blank
    /// quote line) or with a `::: note` container.
    fn ladder(shape: &str, depth: usize) -> String {
        let mut out = String::new();
        let mut indent = String::new();
        let mut closers = Vec::new();
        for i in 0..depth {
            if i % 2 == 1 && shape == "quote" {
                out += &format!("{indent}> q{i}\n{indent}>\n");
                indent += "> ";
            } else if i % 2 == 1 {
                let fence = ":".repeat(3 + (depth - i).div_ceil(2));
                out += &format!("{indent}{fence} note\n");
                closers.push(format!("{indent}{fence}"));
            } else {
                out += &format!("{indent}- item {i}\n");
                indent += "  ";
            }
        }
        while let Some(closer) = closers.pop() {
            out += &closer;
            out.push('\n');
        }
        out
    }

    fn parses(source: &str) -> usize {
        BLOCK_SEQUENCE_PARSES.with(|count| count.set(0));
        let html = crate::to_html(source);
        assert!(html.contains("item"), "{html}");
        BLOCK_SEQUENCE_PARSES.with(Cell::get)
    }

    /// Doubling the depth may cost a few times the parses, since each level
    /// re-reads what is below it; doubling them per level is what this catches.
    #[test]
    fn doubling_a_ladder_keeps_its_body_parses_polynomial() {
        for shape in ["quote", "container"] {
            for tail in ["", "lazy tail\n"] {
                let short = parses(&(ladder(shape, 16) + tail));
                let long = parses(&(ladder(shape, 32) + tail));
                // The event route may admit the shallow ladder and leave no
                // standard body parses, then decline the deeper ladder.
                let route_transition_allowance =
                    if cfg!(feature = "experimental-event-parser-options") {
                        16
                    } else {
                        0
                    };
                assert!(
                    long <= 5 * short + route_transition_allowance,
                    "{shape} {tail:?}: {short} body parses at 16 levels, {long} at 32"
                );
            }
        }
    }
}

#[test]
fn verse_spacing_does_not_enter_a_link_destination() {
    let literal = crate::to_html("::: |\n[a](/u\t\"t\")\n");
    assert!(!literal.contains("<a "), "{literal}");
    assert!(literal.contains("[a](/u"), "{literal}");
    assert!(literal.contains("&nbsp;"), "{literal}");

    let linked = crate::to_html("::: |\n[a](/u \"t\")\n");
    assert!(linked.contains("<a href=\"/u\""), "{linked}");
}

#[cfg(feature = "experimental-event-parser-options")]
mod event_root_rule_a {
    use super::*;
    use crate::{BlockMatch, CarveExtension, InlineMatch, MatcherContext};

    #[derive(Default)]
    struct Hook {
        blocks: RefCell<Vec<String>>,
        inlines: RefCell<Vec<(usize, String)>>,
        after: Cell<usize>,
    }

    impl CarveExtension for Hook {
        fn name(&self) -> &'static str {
            "event-root-rule-a"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            self.blocks.borrow_mut().push(lines.get(start)?.to_string());
            None
        }

        fn match_inline(
            &self,
            text: &str,
            pos: usize,
            _: &MatcherContext<'_>,
        ) -> Option<InlineMatch> {
            let prefix = text.get(pos..).unwrap_or("").chars().take(8).collect();
            self.inlines.borrow_mut().push((pos, prefix));
            None
        }

        fn after_parse(&self, doc: Document) -> Document {
            self.after.set(self.after.get() + 1);
            doc
        }
    }

    /// Past-column openers under each host, at one to four columns past the
    /// host's content column and through a tab.
    fn walked_sources() -> Vec<String> {
        let openers = [
            "> q\n",
            "- x\n",
            "```\ncode\n```\n",
            "::: note\nx\n:::\n",
            ":: t\n:  d\n",
            "%%%\nc\n%%%\n",
            "a\n+\nb\n",
        ];
        // (header, continuation prefix, closer)
        let hosts = [
            ("- item\n", "  ", ""),
            ("> quote\n", "> ", ""),
            (":: term\n:  desc\n", "   ", ""),
            ("::: note\n", "", ":::\n"),
        ];
        let mut sources = Vec::new();
        for opener in openers {
            for (header, prefix, closer) in hosts {
                for extra in [" ", "  ", "   ", "    ", "\t"] {
                    for blank in ["", "\n"] {
                        let mut source = header.to_string();
                        if !blank.is_empty() {
                            source.push_str(prefix.trim_end());
                            source.push('\n');
                        }
                        for line in opener.lines() {
                            source.push_str(&format!("{prefix}{extra}{line}\n"));
                        }
                        source.push_str(closer);
                        source.push_str("\ntail\n");
                        sources.push(source);
                    }
                }
            }
        }
        sources
    }

    fn assert_matches_standard(source: &str) -> bool {
        let mut used_any = false;
        for (mode_index, mode) in [ParseMode::Html, ParseMode::HtmlFacade, ParseMode::Carve]
            .into_iter()
            .enumerate()
        {
            for positions in [false, true] {
                for source_lines in [false, true] {
                    let plain = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines);
                    let expected = {
                        let _standard_root = StandardEventRootGuard::install();
                        let _standard_nested = StandardNestedReaderGuard::install();
                        parse_with_options_mode_and_index(source, &plain, mode).0
                    };
                    let label = format!(
                        "mode={mode_index} positions={positions} source_lines={source_lines} \
                         source={source:?}"
                    );
                    // The nested footnote body may still decline when positioned.
                    let Some((actual, _, used)) =
                        event_pipeline::parse_document_with_mode(source, &plain, mode)
                    else {
                        assert!(
                            positions && source.contains("[^n]: head"),
                            "declined: {label}"
                        );
                        continue;
                    };
                    assert_eq!(actual, expected, "{label}");
                    assert!(used, "not event-owned: {label}");
                    used_any |= used;

                    let baseline = Hook::default();
                    let baseline_options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&baseline);
                    let expected = {
                        let _standard_root = StandardEventRootGuard::install();
                        let _standard_nested = StandardNestedReaderGuard::install();
                        parse_with_options_mode_and_index(source, &baseline_options, mode).0
                    };
                    let candidate = Hook::default();
                    let candidate_options = Options::default()
                        .with_positions(positions)
                        .with_source_lines(source_lines)
                        .with_extension(&candidate);
                    let (actual, _, _) =
                        event_pipeline::parse_document_with_mode(source, &candidate_options, mode)
                            .unwrap_or_else(|| panic!("declined with extension: {label}"));
                    assert_eq!(actual, expected, "extension: {label}");
                    assert_eq!(
                        *candidate.blocks.borrow(),
                        *baseline.blocks.borrow(),
                        "{label}"
                    );
                    assert_eq!(
                        *candidate.inlines.borrow(),
                        *baseline.inlines.borrow(),
                        "{label}"
                    );
                    assert_eq!(candidate.after.get(), 1, "{label}");
                    assert_eq!(baseline.after.get(), 1, "{label}");
                }
            }
        }
        used_any
    }

    #[test]
    fn walked_documents_match_standard_root() {
        let mut walked = 0;
        let mut used = 0;
        for source in walked_sources() {
            if !walk_moves_lines(&normalize_source(&source)) {
                continue;
            }
            walked += 1;
            if assert_matches_standard(&source) {
                used += 1;
            }
        }
        assert!(walked > 0, "no generated source moved under rule A");
        assert_eq!(used, walked, "some walked shapes were not event-owned");
    }

    #[test]
    fn walked_frontmatter_footnotes_bom_and_crlf_match_standard_root() {
        let rows = [
            "---\ntitle: t\n---\n- item\n\n     - child\n".to_string(),
            "- item\n\n     - child [^n]\n\n[^n]: note\n".to_string(),
            "[^n]: head\n\n    - item\n\n         - child\n\nUse [^n].\n".to_string(),
            "- item\n\n     - child\n\n[r]: /target\n\n[link][r]\n".to_string(),
            "\u{feff}- item\n\n     - child\n".to_string(),
            "- item\r\n\r\n     - child\r\n".to_string(),
            "\u{feff}- item\r\n\r\n     > quote\r\n".to_string(),
        ];
        for source in rows {
            assert!(
                walk_moves_lines(&normalize_source(&source)),
                "row does not exercise rule A: {source:?}"
            );
            assert!(
                assert_matches_standard(&source),
                "not event-owned: {source:?}"
            );
        }
    }

    #[test]
    fn walked_child_span_keeps_its_authored_column() {
        let source = "- item\n\n     - child\n";
        assert!(walk_moves_lines(source));
        let options = Options::default().with_positions(true);
        let (document, _, used) =
            event_pipeline::parse_document_with_mode(source, &options, ParseMode::Html)
                .expect("event root admits a walked document");
        assert!(used);
        let BlockNode::List(outer) = &document.children[0] else {
            panic!("{:?}", document.children);
        };
        let inner = outer.items[0]
            .children
            .iter()
            .find_map(|block| match block {
                BlockNode::List(list) => Some(list),
                _ => None,
            })
            .expect("the moved marker opens a sublist");
        let pos = inner.items[0].pos.as_ref().expect("positioned item");
        assert_eq!((pos.start_line, pos.start_column), (3, 6));
        assert_eq!(pos.start_offset, source.find("- child").unwrap());
    }
}
