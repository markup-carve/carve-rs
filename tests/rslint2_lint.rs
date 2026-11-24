//! C2-T-005: lint includes parse-channel diagnostics from the same document.

use carve::lint::{LintOptions, lint};

fn first_position(input: &str, code: &str) -> (u32, u32) {
    let mut options = LintOptions::default();
    options.rules.insert(code.to_string(), "on".to_string());
    let diagnostics = lint(input, &options);
    let finding = diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("missing {code} in {input:?}"));
    let pos = finding.pos.expect("source position");
    (pos.start_line, pos.start_column)
}

#[test]
fn cc_477_lint_reports_an_unclosed_fence_from_parse() {
    let diagnostics = lint("a **b** c\n\n```\nx\n", &LintOptions::default());
    let finding = diagnostics
        .iter()
        .find(|d| d.code == "unclosed-code-fence")
        .expect("parse finding in lint output");
    assert_eq!(finding.channel, "parse");
    let pos = finding.pos.unwrap();
    assert_eq!((pos.start_line, pos.start_column), (3, 1));
}

#[test]
fn cc_1118_lint_reports_fence_title_parse_diagnostic() {
    let diagnostics = lint("```  {.c}\nx\n```\n", &LintOptions::default());
    assert!(diagnostics.iter().any(|d| d.code == "fence-title-syntax"));
}

#[test]
fn cc_920_and_921_version_lint_selects_only_unsupported_values() {
    let unsupported = lint("---\ncarve-version: 9.9\n---\n", &LintOptions::default());
    let finding = unsupported
        .iter()
        .find(|d| d.code == "carve-version-unsupported")
        .expect("unsupported version");
    let pos = finding.pos.unwrap();
    assert_eq!(
        (pos.start_line, pos.start_column, pos.end_column),
        (2, 16, 19)
    );
    let supported = lint("---\ncarve: 0.1\n---\n", &LintOptions::default());
    assert!(
        !supported
            .iter()
            .any(|d| d.code == "carve-version-unsupported")
    );
}

#[test]
fn cc_984_and_985_frontmatter_shape_respects_format_and_blank_first_line() {
    let blank = lint("---\n\na: b\n---\n", &LintOptions::default());
    assert!(
        blank
            .iter()
            .any(|d| d.code == "frontmatter-shaped-thematic-break")
    );
    let named = lint("---toml\na = 1\n---\n", &LintOptions::default());
    assert!(
        !named
            .iter()
            .any(|d| d.code == "frontmatter-shaped-thematic-break")
    );
}

#[test]
fn cc_986_and_987_sigil_hint_requires_a_non_ascii_predecessor() {
    let upgrade = LintOptions {
        upgrade: true,
        ..LintOptions::default()
    };
    let ascii = lint("a@bob\n", &upgrade);
    assert!(!ascii.iter().any(|d| d.code == "sigil-after-letter"));
    let unicode = lint("ü#1a\n", &upgrade);
    let finding = unicode
        .iter()
        .find(|d| d.code == "sigil-after-letter")
        .expect("non-ASCII predecessor");
    let pos = finding.pos.unwrap();
    assert_eq!((pos.start_line, pos.start_column), (1, 2));
}

#[test]
fn heading_and_ordered_list_lint_ranges_follow_authored_markers() {
    for (input, code, expected) in [
        ("# a\nb\n", "djot-heading-continuation", (1, 1)),
        ("# a #\n", "heading-closing-hashes", (1, 5)),
        ("= a\n# b\n", "heading-marker-mixed", (1, 1)),
        ("v. a\nvii. b\n", "ordered-list-kind-split", (1, 1)),
        (" 9. a\n10. b\n", "ordered-marker-right-aligned", (2, 1)),
    ] {
        assert_eq!(first_position(input, code), expected, "{input:?}");
    }
}

#[test]
fn term_lint_ranges_follow_term_text() {
    for (input, code, expected) in [
        (":: > a\n", "definition-term-block-folded", (1, 4)),
        (":: - a\n", "definition-term-block-folded", (1, 4)),
        (":: a\n  : b\n", "description-term-multi-line", (2, 3)),
    ] {
        assert_eq!(first_position(input, code), expected, "{input:?}");
    }
    assert!(
        !lint(":: t\n  > a\n", &LintOptions::default())
            .iter()
            .any(|d| d.code == "definition-term-block-folded")
    );
}

#[test]
fn link_and_caption_hints_use_the_matched_line() {
    assert_eq!(
        first_position("[a] (b)\n", "link-gap-before-destination"),
        (1, 1)
    );
    assert!(
        !lint("[a] ()\n", &LintOptions::default())
            .iter()
            .any(|d| d.code == "link-gap-before-destination")
    );
    assert!(
        !lint("\\[a] (/u)\n", &LintOptions::default())
            .iter()
            .any(|d| d.code == "link-gap-before-destination")
    );
    assert_eq!(
        first_position(
            "![a](a.png)\n\nFigure 1: A cat\n",
            "caption-looking-paragraph"
        ),
        (3, 1)
    );
    assert_eq!(
        first_position("[Click  here](/u)\n", "non-descriptive-link-text"),
        (1, 1)
    );
}

#[test]
fn include_and_table_hints_select_the_authored_token() {
    for (input, code, expected) in [
        (
            "{table}\n{{ t.csv @code:csv @lines:1-2 }}\n",
            "include-lines-table-rows-hint",
            (2, 20),
        ),
        ("{aligns=x}\n| a |\n", "table-attribute-invalid", (1, 2)),
        (
            "{table header-rows=3}\n```csv\na\nb\n```\n",
            "table-row-count-clamped",
            (1, 8),
        ),
        ("|=< a |\n", "deprecated-cell-alignment", (1, 3)),
        ("| a | %% x\n+ b |\n", "comment-splits-table-row", (1, 6)),
    ] {
        assert_eq!(first_position(input, code), expected, "{input:?}");
    }
}

#[test]
fn empty_include_path_respects_spacing_and_quotes() {
    for input in ["{{ \"\" }}\n", "{{ #a }}\n"] {
        assert_eq!(first_position(input, "empty-include-path"), (1, 1));
    }
    assert!(
        !lint("{{#if}}\n", &LintOptions::default())
            .iter()
            .any(|d| d.code == "empty-include-path")
    );
}

#[test]
fn lint_respects_top_fragment_alert_and_indented_block_context() {
    assert!(
        !lint("[a](#top)\n", &LintOptions::default())
            .iter()
            .any(|d| d.code == "broken-fragment-link")
    );
    assert!(
        !lint("> [!FOO]\n", &LintOptions::default())
            .iter()
            .any(|d| d.code == "github-alert-marker")
    );
    for input in ["    a\n", "a\n    b\n"] {
        assert!(
            !lint(input, &LintOptions::default())
                .iter()
                .any(|d| d.code == "indented-code-block")
        );
    }
}

#[test]
fn container_markers_keep_lint_ranges_on_the_marker() {
    for (input, code, expected) in [
        ("- +\n  b\n", "deprecated-continuation-marker", (1, 3)),
        (
            "> - a\n> +\n> b\n",
            "deprecated-continuation-marker",
            (2, 3),
        ),
        ("- a\n  +\n", "deprecated-continuation-marker", (2, 3)),
        ("> x\n> - y\n", "list-marker-without-blank-line", (2, 3)),
        ("> ___\n", "thematic-break-spelling", (1, 3)),
    ] {
        assert_eq!(first_position(input, code), expected, "{input:?}");
    }
    assert!(
        !lint("x\n+ y\n", &LintOptions::default())
            .iter()
            .any(|d| d.code == "list-marker-without-blank-line")
    );
}

#[test]
fn frontmatter_quote_lint_uses_the_frontmatter_message() {
    let diagnostics = lint("---\n>foo\n---\n", &LintOptions::default());
    let finding = diagnostics
        .iter()
        .find(|d| d.code == "blockquote-marker-without-space")
        .expect("frontmatter quote-looking text");
    let pos = finding.pos.unwrap();
    assert_eq!(
        (pos.start_line, pos.start_column, pos.end_column),
        (2, 1, 5)
    );
    assert!(finding.message.contains("frontmatter content"));
}

#[test]
fn frontmatter_quote_lint_respects_disabled_rule() {
    let mut options = LintOptions::default();
    options.rules.insert(
        "blockquote-marker-without-space".to_string(),
        "off".to_string(),
    );
    assert!(
        lint("---\n>foo\n---\n", &options)
            .iter()
            .all(|finding| finding.code != "blockquote-marker-without-space")
    );
}

#[test]
fn platform_tokens_ignore_opaque_footnote_shaped_lines() {
    let mut options = LintOptions::default();
    options.platforms.push("github".to_string());
    let source = "```\n[^n]: #12\n```\n\n[^n]\n";
    assert!(
        lint(source, &options)
            .iter()
            .all(|finding| finding.code != "platform-issue-reference")
    );
    assert!(
        lint("[^n]: #12\n\n[^n]\n", &options)
            .iter()
            .any(|finding| finding.code == "platform-issue-reference")
    );
    assert!(
        lint("[^n]: #12\n\n```\n[^n]\n```\n", &options)
            .iter()
            .all(|finding| finding.code != "platform-issue-reference")
    );
}

#[test]
fn paragraph_and_strike_rules_respect_visible_nodes() {
    assert_eq!(first_position("*Title*\n", "emphasis-as-heading"), (1, 1));
    for input in ["{~a\\~>b~}\n", "{~`a~>b`~}\n", "{~[a~>b](/u)~}\n"] {
        assert!(
            !lint(input, &LintOptions::default())
                .iter()
                .any(|d| d.code == "deprecated-substitution-arrow")
        );
    }
    assert_eq!(
        first_position("{~a~>b~}\n", "deprecated-substitution-arrow"),
        (1, 1)
    );
    for input in ["|---|\n| a |\n", "| a |\n| b |\n|---|\n"] {
        assert!(
            !lint(input, &LintOptions::default())
                .iter()
                .any(|d| d.code == "deprecated-separator-row")
        );
    }
}
