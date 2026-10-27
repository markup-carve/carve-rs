use carve::lint_carve;

#[test]
fn quoted_plus_in_a_list_item_has_no_false_refusal() {
    for source in ["- a\n  > q\n  > > +\n  > > x\n", "- a\n  >\n  > +\n  > x\n"] {
        let warnings = lint_carve(source);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.rule != "continuation-marker-refused"),
            "{source:?}: {warnings:?}"
        );
        if source.contains("> > +") {
            let html = carve::to_html(source);
            assert!(
                html.contains("<blockquote><p>+\nx</p></blockquote>"),
                "{html}"
            );
        }
    }
}

#[test]
fn first_block_refusals_name_the_quote_and_footnote() {
    for (source, host) in [("[^a]: +\n", "footnote"), ("> +\n", "block quote")] {
        let warning = lint_carve(source)
            .into_iter()
            .find(|warning| warning.rule == "continuation-marker-refused")
            .unwrap();
        assert!(
            warning.message.contains("remains text in the"),
            "{warning:?}"
        );
        assert!(warning.message.contains(host), "{warning:?}");
    }
}

#[test]
fn source_rules_report_the_shared_triggers() {
    for (rule, source) in [
        ("bidi-control-in-source", "a\u{202e}b\n"),
        ("heading-trailing-attribute", "# Title {#id}\n"),
        (
            "fence-opener-fallback",
            "``` php extra bad info\ncode\n```\n",
        ),
        ("raw-block-syntax", "```raw html\nx\n```\n"),
        ("blockquote-marker-without-space", ">quoted\n"),
        ("blockquote-marker-without-space", ">+\n"),
        ("block-marker-as-text", "  ::: note\n"),
        ("fence-delimiter-indentation", "  ```\n  x\n  ```\n"),
        ("list-item-body-detached", "1. item\n\n  # heading\n"),
        (
            "list-item-block-overindented",
            "-{.x1} item\n\n       # heading\n",
        ),
        ("empty-include-path", "{{ #section }}\n"),
        (
            "carve-version-unsupported",
            "---\ncarve-version: 99.0\n---\n\nx\n",
        ),
        ("unclosed-container-fence", "::: note\nbody\n"),
        ("colon-fence-length-mismatch", ":::: note\nbody\n:::\n"),
        (
            "figure-group-empty",
            "::: figure\njust a paragraph\n:::\n^ Figure #: G\n",
        ),
        (
            "figure-group-single-panel",
            "::: figure\n![a](a.png)\n^ (a) A\n:::\n^ Figure #: G\n",
        ),
        ("fence-title-syntax", "::: note Some Title\nbody\n:::\n"),
        (
            "footnotes-placement-in-container",
            "Intro[^a].\n\n> ::: footnotes\n> :::\n\n[^a]: only note\n",
        ),
        ("table-marker-run-padding", "|{#x}< content |\n"),
    ] {
        assert!(
            lint_carve(source).iter().any(|w| w.rule == rule),
            "{rule}: {source:?}"
        );
    }
}

#[test]
fn literal_examples_and_closed_containers_do_not_warn() {
    for source in [
        "```\n{{ #x }}\n>quoted\n  ::: note\n```\n",
        "%%%\n{{ #x }}\n>quoted\n%%%\n",
        "`{{ #x }}`\n",
        "\\{{ #x }}\n",
        "::: note\nbody\n:::\n",
        "> ::: note\n> body\n> :::\n",
        "- item\n\n  # Heading\n",
        "---\ncarve-version: 0.1.0\n---\n\nx\n",
    ] {
        assert!(
            lint_carve(source).is_empty(),
            "{source:?}: {:?}",
            lint_carve(source)
        );
    }
}

#[test]
fn include_warning_spans_use_original_unicode_bytes() {
    let source = "😀\r\n\r\n> {{ #missing }}\r\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "empty-include-path")
        .unwrap();
    assert_eq!(warning.line, 3);
    assert_eq!(warning.column, 3);
    assert_eq!(&source[warning.start..warning.end], "{{ #missing }}");
}

#[test]
fn fence_diagnostics_respect_the_owner_and_closer_column() {
    for source in [
        "::: note\n::: tip\nx\n:::\n",
        "::: note\nx\n\n    :::\n",
        "a. ::: note\n   x\n",
    ] {
        assert!(
            lint_carve(source)
                .iter()
                .any(|w| w.rule == "unclosed-container-fence"),
            "{source:?}"
        );
    }
    assert!(lint_carve(":::: note\n:::\ninner\n:::\n::::\n").is_empty());
}

#[test]
fn cell_marker_advice_requires_a_real_unpadded_cell_prefix() {
    for source in [
        "| a `|{#x}<` b |\n",
        "| a \\|{#x}< b |\n",
        "| {#x}< b |\n",
        "> | a | {#x}< b |\n",
    ] {
        assert!(
            !lint_carve(source)
                .iter()
                .any(|w| w.rule == "table-marker-run-padding"),
            "{source:?}"
        );
    }
    assert!(lint_carve("> |{#x}< b |\n")
        .iter()
        .any(|w| w.rule == "table-marker-run-padding"));
}

#[test]
fn quoted_markup_gets_the_specific_warning_and_marker_span() {
    assert!(lint_carve("> ```raw html\n> x\n> ```\n")
        .iter()
        .any(|w| w.rule == "raw-block-syntax"));
    assert!(lint_carve("> # T {#id}\n")
        .iter()
        .any(|w| w.rule == "heading-trailing-attribute"));
    let source = "  {.cls} text\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "block-marker-as-text")
        .unwrap();
    assert_eq!(&source[warning.start..warning.end], "{.");
}

#[test]
fn speculative_list_parses_do_not_publish_attribute_warnings() {
    let source = "- {.a\n  .b}\ntail\n";
    let warnings: Vec<_> = lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "unattached-block-attribute")
        .collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].line, warnings[0].column), (1, 3));
    assert_eq!(&source[warnings[0].start..warnings[0].end], "{.a\n  .b}");
}

#[test]
fn invalid_alignment_marker_pairs_are_not_unpadded_runs() {
    assert!(!lint_carve("|? lone |v? reversed |\n")
        .iter()
        .any(|w| w.rule == "table-marker-run-padding"));
    assert!(lint_carve("|>value |\n")
        .iter()
        .any(|w| w.rule == "table-marker-run-padding"));
}

#[test]
fn invalid_title_separator_diagnoses_dropped_metadata() {
    let warnings = lint_carve("::: note \t\"Title\"\nx\n:::\n");
    assert!(warnings.iter().any(|w| w.rule == "fence-title-syntax"));
    assert!(!warnings.iter().any(|w| w.rule == "block-marker-as-text"));
}

#[test]
fn definition_markers_do_not_hide_later_list_warnings() {
    let warnings = lint_carve("- intro\n\n  :: term\n  :  definition\n   > quote\n");
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        (warnings[0].rule, warnings[0].line),
        ("list-item-block-overindented", 5)
    );
}

#[test]
fn fence_tracking_ends_with_the_owning_item() {
    for fence in ["```", "~~~"] {
        let warnings: Vec<_> = lint_carve(&format!(
            "- a\n  - b\n\n    {fence}\n    p\n {fence}\n\n    tail\n"
        ))
        .into_iter()
        .filter(|warning| warning.rule != "unclosed-code-fence")
        .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            (warnings[0].rule, warnings[0].line),
            ("list-item-body-detached", 6)
        );
    }
}

#[test]
fn migration_habits_use_codepoint_columns_and_skip_native_constructs() {
    let warnings = lint_carve("+ bullet\n\né😀 **bold** ~~gone~~ ^word^\n");
    for (rule, line, column) in [
        ("djot-plus-bullet", 1, 1),
        ("markdown-strong-double-star", 3, 4),
        ("markdown-strikethrough-double-tilde", 3, 13),
        ("djot-superscript-caret", 3, 22),
    ] {
        assert!(
            warnings
                .iter()
                .any(|w| (w.rule, w.line, w.column) == (rule, line, column)),
            "{warnings:?}"
        );
    }
    for source in [
        "`**code** ~~code~~ ^code^`\n",
        "{^sup^}\n",
        "\\^escaped^\n",
        "| a | b |\n+ c | d |\n",
        "[link](/^path^)\n",
    ] {
        assert!(
            lint_carve(source).is_empty(),
            "{source}: {:?}",
            lint_carve(source)
        );
    }
}

#[test]
fn a_comment_closer_at_the_host_boundary_hides_definitions() {
    for source in [
        "- item\n  %%%\n  [r]: /url\n%%%\n\n[use][r]\n",
        "- item\n  %%%\n  [r]: /url\n  %%%\n\n[use][r]\n",
    ] {
        assert!(!carve::to_html(source).contains("href="));
        assert!(lint_carve(source)
            .iter()
            .any(|w| w.rule == "unresolved-reference-link"));
    }
    assert!(carve::to_html("- item\n  %%%\n  [r]: /url\n\n[use][r]\n").contains("href="));
}

#[test]
fn same_line_nested_markers_keep_their_own_columns() {
    assert!(lint_carve("- > - x\n    [r]: /url\n\nSee [r][].\n").is_empty());
    let warnings = lint_carve("- - x\n   [r]: /url\n\nSee [r][].\n");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].rule, "list-item-body-detached");
}

#[test]
fn habit_warnings_cross_inline_nodes_and_soft_breaks() {
    for (source, rule) in [
        ("a **b `c` d** e", "markdown-strong-double-star"),
        ("a ~~b *c* d~~ e", "markdown-strikethrough-double-tilde"),
        ("~~->~~ arrow", "markdown-strikethrough-double-tilde"),
        ("x^a \"b\"^", "djot-superscript-caret"),
        ("a ~~b\nc~~ d", "markdown-strikethrough-double-tilde"),
        ("a ^b\nc^ d", "djot-superscript-caret"),
    ] {
        assert!(
            lint_carve(source).iter().any(|w| w.rule == rule),
            "{source}"
        );
    }
    for source in ["a ~~b\n\nc~~ d", "a ^b\n\nc^ d", "{~~b~} x {~~c~}"] {
        assert!(
            !lint_carve(source)
                .iter()
                .any(|w| w.rule.starts_with("markdown-") || w.rule == "djot-superscript-caret"),
            "{source}"
        );
    }
}

#[test]
fn rejected_habit_candidates_do_not_hide_later_spans() {
    for (source, line, column) in [
        ("x 2^10 in para\n\nand x^2^ later", 3, 6),
        ("a ~~b\n\nc~~d~~ e", 3, 2),
        ("a `~~b`~~c~~ d", 1, 8),
    ] {
        let warnings = lint_carve(source);
        assert!(
            warnings
                .iter()
                .any(|w| w.line == line && w.column == column),
            "{source}: {warnings:?}"
        );
    }
}

#[test]
fn overindented_blocks_report_only_their_openers() {
    for (source, expected) in [
        ("- a\n  > q\n   | c |\n   |---|\n   | 1 |\n", vec![3]),
        ("- a\n   | c |\n   |---|\n   | 1 |\n", vec![2]),
        ("- a\n   > q\n   > r\n", vec![2]),
        ("- a\n   > q\n   lazy\n   > r\n", vec![2]),
        ("- a\n    > q\n   > r\n", vec![2]),
        ("> - a\n>    | x |\n>    | y |\n", vec![2]),
        ("- a\n  - b\n     | x |\n     | y |\n", vec![3]),
        ("- a\n   | x |\n   > q\n   | y |\n", vec![2, 3, 4]),
        ("- a\n   > q\n   >\n   > r\n", vec![2]),
        ("- a\n   > q\n\n   > r\n", vec![2, 4]),
        ("- a\n   > ```\n   > code\n   > ```\n   > r\n", vec![2]),
        ("- a\n   > q\nlazy\n   > > r\n", vec![2]),
        ("- a\n   | x |\n    | y |\n", vec![2]),
        ("- a\n  | x |\n   | y |\n", vec![]),
        ("- | x |\n   | y |\n", vec![]),
        ("- a\n  > q\n   > r\n", vec![]),
        ("- a\n  >\n   > r\n", vec![]),
        ("- a\n  > ```\n  > c\n   > ```\n", vec![]),
        ("> - a\n>    > q\n>    > r\n", vec![2]),
        ("> - a\n>\n>    > q\n", vec![3]),
        ("- > q\n   > r\n", vec![]),
        ("- a\n   # a\n   # b\n", vec![2, 3]),
        ("- a\n   ---\n   ---\n", vec![2, 3]),
        ("- a\n   | a |\n\n   | b |\n", vec![2, 4]),
    ] {
        let lines: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "list-item-block-overindented")
            .map(|w| w.line)
            .collect();
        assert_eq!(lines, expected, "{source}");
    }
}

#[test]
fn a_fence_below_a_comment_in_a_marker_line_note_is_not_a_list_warning() {
    for source in [
        "- [^n]: text\n\n    %% c\n    ```\n    code\n    ```\nafter [^n]\n",
        "- [^n]: text\n    %% c\n    ```\n    code\n    ```\nafter [^n]\n",
        "- [^n]: text\n    more\n\n    %% c\n    ~~~\n    code\nafter [^n]\n",
        "- [^n]: text\n    more\n    %% c\n    ~~~\n    code\nafter [^n]\n",
        "- [^n]: text\n\n    %% c\n        ```\n        code\n        ```\nafter [^n]\n",
    ] {
        let warnings: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|warning| warning.rule == "list-item-block-overindented")
            .collect();
        assert!(warnings.is_empty(), "{source:?}: {warnings:?}");
    }
    let source =
        "- [^n]: text\n\n    - child\n\n      %% c\n        ```\n        code\n        ```\nafter [^n]\n";
    assert!(
        lint_carve(source)
            .iter()
            .any(|warning| warning.rule == "list-item-block-overindented"),
        "{source:?}"
    );
}

#[test]
fn canonical_empty_bodies_and_quote_prefixes_do_not_warn() {
    for source in [
        "See[^f]\n\n[^f]: {empty}\n",
        ":: t\n: {empty}\n\nflush\n",
        "- > ```\n  > x\n\n  > y\n  > ```\n",
    ] {
        let warnings: Vec<_> = carve::lint_carve(source)
            .into_iter()
            .filter(|warning| warning.rule != "unclosed-code-fence")
            .collect();
        assert!(warnings.is_empty(), "{source:?}: {warnings:?}");
    }
    for source in [
        "> :  a\n>\n>    ```\n>    x\nflush\n",
        "> - a\n>\n>     ~~~\n>     x\nz\n>     b\n>     ~~~\n",
    ] {
        assert!(!carve::lint_carve(source)
            .iter()
            .any(|warning| warning.rule == "fence-delimiter-indentation"));
    }
    assert!(carve::lint_carve("{empty}\n")
        .iter()
        .any(|warning| warning.rule == "unattached-block-attribute"));
}

#[test]
fn repeated_unicode_warnings_keep_codepoint_columns() {
    let source = "é **one** **two**\n漢 **three** **four**\na\u{202e}é\u{2066}b";
    let warnings = lint_carve(source);
    let strong: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule == "markdown-strong-double-star")
        .map(|w| (w.line, w.column))
        .collect();
    assert_eq!(strong, [(1, 3), (1, 11), (2, 3), (2, 13)]);
    let bidi: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule == "bidi-control-in-source")
        .map(|w| (w.line, w.column))
        .collect();
    assert_eq!(bidi, [(3, 2), (3, 4)]);
}

#[test]
fn markdown_star_habit_reaches_parsed_emphasis_markers() {
    for (source, line, column) in [
        ("/**/ /**/", 1, 2),
        ("/**/\n/**/", 1, 2),
        ("> /**/ /**/", 1, 4),
        ("- /**/ /**/", 1, 4),
        ("::: note\n/**/ /**/\n:::", 2, 2),
    ] {
        let warnings: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|warning| warning.rule == "markdown-strong-double-star")
            .collect();
        assert!(
            warnings
                .iter()
                .any(|warning| (warning.line, warning.column) == (line, column)),
            "{source:?}: {warnings:?}"
        );
    }
    for source in ["`/**/ /**/`", "```txt\n/**/ /**/\n```"] {
        assert!(
            !lint_carve(source)
                .iter()
                .any(|warning| warning.rule == "markdown-strong-double-star"),
            "{source:?}"
        );
    }
}

#[test]
fn region_marker_rendering_warning() {
    for source in [
        "#region demo",
        "#endregion",
        "// #region demo",
        "-- #region demo",
        "/* #region demo */",
        "<!-- #region demo -->",
        "%% #Region demo",
        "%% #ENDREGION",
    ] {
        let warnings: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "region-marker-renders-as-text")
            .collect();
        assert_eq!(warnings.len(), 1, "{source:?}: {warnings:?}");
        assert!(warnings[0].message.contains("%% #region name"));
    }
    for source in [
        "%% #region demo\nBody.\n%% #endregion demo",
        "```txt\n#region demo\n```",
        "%%%\n#region demo\n%%%",
        "```txt\n%% #Region demo\n```",
        "%%%\n%% #Region demo\n%%%",
    ] {
        assert!(
            !lint_carve(source)
                .iter()
                .any(|w| w.rule == "region-marker-renders-as-text"),
            "{source:?}"
        );
    }
}

#[test]
fn an_attached_item_first_block_right_has_no_refusal_warning() {
    let warnings = lint_carve("- +\n    x\n");
    assert!(
        warnings
            .iter()
            .all(|warning| warning.rule != "continuation-marker-refused"),
        "{warnings:?}"
    );
}
