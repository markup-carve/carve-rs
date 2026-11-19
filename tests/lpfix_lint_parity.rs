use carve::lint::{LintOptions, lint};

fn spans(source: &str, rule: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == rule)
        .filter_map(|finding| finding.pos)
        .map(|pos| (pos.start_offset, pos.end_offset))
        .collect()
}

#[test]
fn indented_code_requires_an_indented_paragraph() {
    for source in ["\n      {{ x.crv }}\n", "\n    a. x\n", "\n      = H\n"] {
        assert!(
            spans(source, "indented-code-block").is_empty(),
            "{source:?}"
        );
    }
    assert_eq!(
        spans("\n    plain text\n", "indented-code-block"),
        vec![(5, 15)]
    );
}

#[test]
fn separator_row_allows_trailing_whitespace() {
    assert_eq!(
        spans("| A | B |\n|---|---| \n", "deprecated-separator-row"),
        vec![(10, 20)]
    );
    assert_eq!(
        spans("| A | B |\n|---|---|\t\n", "deprecated-separator-row"),
        vec![(10, 20)]
    );
}

#[test]
fn block_lint_follows_the_parsed_leaf() {
    for source in ["a\n# H #\n", "\t:: t\n: ## H #\n"] {
        assert!(
            spans(source, "heading-closing-hashes").is_empty(),
            "{source:?}"
        );
    }
    assert_eq!(spans("== H #\n", "heading-closing-hashes"), vec![(5, 6)]);
    assert_eq!(
        spans("{{ x.crv#s }}\n-----\n", "setext-heading-underline"),
        Vec::new()
    );
    assert_eq!(
        spans("paragraph\n-----\n", "setext-heading-underline"),
        vec![(10, 15)]
    );
}

#[test]
fn unattached_attribute_requires_its_own_leaf_line() {
    for source in ["term\n{#id .c k=v}\n:::\n", "> :\n{#id}\n>\n"] {
        assert!(
            spans(source, "unattached-block-attribute").is_empty(),
            "{source:?}"
        );
    }
    for source in ["* {#id}\n", "> {#id}\n", ":: t\n: {#id}\n"] {
        assert_eq!(
            spans(source, "unattached-block-attribute").len(),
            1,
            "{source:?}"
        );
    }
}

#[test]
fn list_item_overindent_requires_a_block_opener() {
    for source in ["* a\n   ![alt](i.png)\n", "* $$x$${#m}\n   ![alt](i.png)\n"] {
        assert!(
            spans(source, "list-item-block-overindented").is_empty(),
            "{source:?}"
        );
    }
}

#[test]
fn quote_and_term_markers_follow_their_parsed_owner() {
    assert_eq!(spans(">: > [!NOTE]\n", "github-alert-marker"), Vec::new());
    for source in [":: t\n> ---\n", ":: t\n  - ----\n"] {
        assert_eq!(spans(source, "thematic-break-spelling"), Vec::new());
    }
    for source in ["> :::\n> * ^\n", "> :::: \n> - [x]\n", "> ::::\n> - [x]\n"] {
        assert_eq!(spans(source, "list-marker-without-blank-line"), Vec::new());
    }
}

#[test]
fn ordered_alignment_compares_marker_ends() {
    assert_eq!(
        spans("  10. x\n1. y\n", "ordered-marker-right-aligned"),
        Vec::new()
    );
    assert_eq!(
        spans("[^n]: 10. +|\n   1. :::\n", "ordered-marker-right-aligned"),
        Vec::new()
    );
    assert_eq!(
        spans(" 9. x\n10. y\n", "ordered-marker-right-aligned").len(),
        1
    );
}

#[test]
fn footnote_label_range_excludes_a_preceding_attribute() {
    assert_eq!(
        spans("{.c}\n[^f]: a\n", "unused-footnote-definition"),
        vec![(5, 9)]
    );
}

#[test]
fn inline_attributes_allow_spaces_inside_braces() {
    for source in ["a { #id }\n", "+ { #id }\n"] {
        assert_eq!(
            spans(source, "unattached-inline-attribute"),
            vec![(2, 9)],
            "{source:?}"
        );
    }
}

#[test]
fn empty_definition_after_quote_marker_is_reported() {
    assert_eq!(
        spans("> [a]:\n", "empty-reference-definition"),
        vec![(2, 6)]
    );
}

#[test]
fn opener_indent_uses_the_container_content_column_and_tab_stops() {
    for source in [
        "[^n]: a\n\n      > q\n",
        ":: term\n:   > a\n",
        ":: term\n:  :: word\n",
        "[^n]: :::\n == H ##\n",
    ] {
        assert!(
            spans(source, "block-opener-overindented").is_empty(),
            "{source:?}"
        );
    }
    assert_eq!(
        spans(":: a\n: key\n\n\t:: term\n", "block-opener-overindented"),
        vec![(13, 15)]
    );
    assert_eq!(
        spans("  ````=html\n", "block-opener-overindented"),
        vec![(2, 3)]
    );
    assert_eq!(
        spans("   ```=html\n", "block-opener-overindented"),
        vec![(3, 4)]
    );
}

#[test]
fn continued_heading_attribute_is_reported() {
    assert_eq!(
        spans("### H\n {#id}\n", "heading-trailing-attribute"),
        vec![(7, 12)]
    );
}

#[test]
fn caption_like_line_looks_for_an_image_anywhere_above() {
    assert_eq!(
        spans("2) ![alt](i.png)\n   : word\n", "caption-looking-paragraph"),
        vec![(20, 21)]
    );
    assert_eq!(
        spans(">![](i.png)\n: {%c%}\n", "caption-looking-paragraph"),
        vec![(12, 13)]
    );
}

#[test]
fn a_lazy_line_literal_quote_does_not_start_a_definition() {
    assert_eq!(
        spans("[^f]: > a\n> [ ]: u\n", "blank-reference-label"),
        Vec::new()
    );
    assert_eq!(spans("> [ ]: u\n", "blank-reference-label"), vec![(2, 6)]);
}

#[test]
fn item_body_column_accounts_for_quotes_and_tabs() {
    assert_eq!(
        spans(
            "> 10. %%\n>    : a\n",
            "list-item-body-below-content-column"
        ),
        vec![(14, 17)]
    );
    assert_eq!(
        spans(
            "\ti) :: term\n\n     +|\n",
            "list-item-body-below-content-column"
        ),
        vec![(18, 20)]
    );
}

#[test]
fn description_column_requires_an_open_term_above() {
    for source in ["    [^f]: :: term\n:\n", ":: t\n- [ ] :: t\n:\n"] {
        assert!(
            spans(source, "description-separator-below-content-column").is_empty(),
            "{source:?}"
        );
    }
}

#[test]
fn continuation_plus_is_not_a_list_marker() {
    assert_eq!(
        spans("- + x\n + item\n", "list-marker-below-content-column"),
        Vec::new()
    );
}

#[test]
fn indented_heading_continuation_after_quote_prefix() {
    assert_eq!(
        spans(
            "> i) == H\n>        $$x$${#m}\t\n",
            "deprecated-indented-line-under-heading"
        ),
        vec![(10, 29)]
    );
}

#[test]
fn lazy_quote_line_is_an_opener_candidate() {
    assert_eq!(
        spans(
            "[^f]: > [^m]: key [ref][]\n> [\n",
            "block-opener-without-blank-line"
        ),
        vec![(26, 27)]
    );
    assert_eq!(
        spans("    * > > +\n> >\n", "block-opener-without-blank-line"),
        vec![(12, 13)]
    );
}

#[test]
fn colon_mismatch_requires_an_open_container() {
    assert_eq!(
        spans(":::\n::::\n:::\n", "colon-fence-length-mismatch"),
        vec![(4, 8)]
    );
    for (source, expected) in [
        ("  :::\n::::\n", (6, 10)),
        ("  ::::\n:::\n", (7, 10)),
        ("\t::::\n:::\n", (6, 9)),
        ("* ::: warning\n   ::::\n   :::\n", (17, 21)),
    ] {
        assert_eq!(
            spans(source, "colon-fence-length-mismatch"),
            vec![expected],
            "{source:?}"
        );
    }
}

#[test]
fn later_list_markers_in_quoted_and_footnote_text() {
    assert_eq!(
        spans("> ::\n>   2) :: word\n", "list-marker-without-blank-line"),
        vec![(9, 19)]
    );
    assert_eq!(
        spans("[^f]: :: more\n   - x\n", "list-marker-without-blank-line"),
        vec![(17, 20)]
    );
}

#[test]
fn every_unused_definition_is_reported() {
    assert_eq!(
        spans("[a]: <>\n[a]: u\n", "unused-link-reference-definition"),
        vec![(0, 7), (8, 14)]
    );
}

#[test]
fn item_term_continuation_has_a_multiline_diagnostic() {
    assert_eq!(
        spans("* :: key\n * %%#region\n", "description-term-multi-line"),
        vec![(10, 21)]
    );
}
