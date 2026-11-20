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
fn list_opener_uses_its_owner_and_visual_column() {
    assert_eq!(
        spans("* a\n\n   :: a\n", "list-item-block-overindented"),
        vec![(8, 12)]
    );
    assert_eq!(
        spans("* a\n\n\t## H\n", "list-item-block-overindented"),
        vec![(6, 10)]
    );
}

#[test]
fn hyphen_runs_use_the_parsed_paragraph_and_item() {
    assert_eq!(
        spans("> { #id }\n-----\n", "setext-heading-underline"),
        vec![(10, 15)]
    );
    assert!(spans("> { #id }\n-----\n", "thematic-break-spelling").is_empty());
    assert!(spans(":: t\n: d\n  - ----\n", "thematic-break-spelling").is_empty());
}

#[test]
fn nested_opener_uses_div_content_column() {
    assert_eq!(
        spans("   :::\n  ***\n", "block-opener-overindented"),
        vec![(3, 6)]
    );
}

#[test]
fn caption_checks_neighbor_behind_container_prefixes() {
    assert_eq!(
        spans("> : x\n> | a |\n", "caption-looking-paragraph"),
        vec![(2, 3)]
    );
}

#[test]
fn colon_run_uses_its_actual_open_container() {
    assert!(spans("1. ::::\n:::\n", "colon-fence-length-mismatch").is_empty());
    assert_eq!(
        spans("{.c}\n::::\n:::\n", "colon-fence-length-mismatch"),
        vec![(10, 13)]
    );
    assert_eq!(
        spans("::::\n{#id .c k=v}\n:::\n", "colon-fence-length-mismatch"),
        vec![(18, 21)]
    );
}

#[test]
fn attribute_run_must_be_an_attribute_leaf() {
    assert!(spans("a\n{.c}\nb\n", "unattached-block-attribute").is_empty());
    assert!(spans(":: a\n{.c}\n", "unattached-block-attribute").is_empty());
}

#[test]
fn text_quote_prefix_does_not_hide_inline_attribute() {
    assert_eq!(
        spans(">{.c}\n", "unattached-inline-attribute"),
        vec![(1, 5)]
    );
}

#[test]
fn lazy_term_line_is_not_a_definition() {
    assert!(spans("i) :: a\n[a]: u\n", "definition-in-container").is_empty());
}

#[test]
fn nested_list_columns_use_actual_marker_positions() {
    assert_eq!(
        spans(
            "- a.\n     * +\n    - c\n",
            "list-marker-below-content-column"
        ),
        vec![(18, 21)]
    );
}

#[test]
fn marker_in_nested_div_is_a_paragraph_line() {
    assert_eq!(
        spans(
            "10. :::\n    a\n    - x\n",
            "list-marker-without-blank-line"
        ),
        vec![(18, 21)]
    );
}

#[test]
fn sibling_item_interrupts_a_term() {
    assert!(spans("2) :: a\n10. -\n", "description-term-multi-line").is_empty());
}

#[test]
fn lazy_heading_lines_do_not_join_the_outline() {
    assert!(spans(":x\n\t  # H\n ### H\n", "heading-level-skip").is_empty());
}

#[test]
fn hidden_comment_body_has_no_html_tag() {
    assert!(
        spans(
            "\t>:::\n## H\ni) i) %%%%\n       %%% \n>  a <b>\n",
            "bare-html-tag"
        )
        .is_empty()
    );
}
