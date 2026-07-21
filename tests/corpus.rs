//! Spec-corpus integration tests.
//!
//! Walks `tests/spec/tests/corpus/` (a git submodule of
//! `markup-carve/carve`), pairing every `NN-slug.crv` with its
//! `NN-slug.html` and asserting that `carve::to_html` produces
//! byte-identical output after trimming.
//!
//! Every pair in an implemented category is checked by
//! `all_implemented_corpus_pairs_match`; named tests keep representative
//! failures scoped.

use std::fs;
use std::path::PathBuf;

const IMPLEMENTED: &[&str] = &[
    "01-emphasis",
    "02-headings",
    "03-links",
    "04-images",
    "05-lists",
    "06-task-lists",
    "07-blockquote-with-attribution",
    "08-image-with-caption",
    "09-tables",
    "10-tables-with-rowspan-and-colspan",
    "11-fenced-code",
    "12-inline-code",
    "13-attributes",
    "14-frontmatter",
    "15-heading-ids",
    "16-reference-link",
    "17-collapsed-reference-link",
    "18-unresolved-reference-link",
    "19-smart-typography-dashes-and-quotes",
    "20-smart-typography-arrows-and-symbols",
    "21-math",
    "22-footnotes",
    "23-inline-footnotes",
    "24-generic-divs",
    "25-definition-lists",
    "26-comments",
    "27-raw-blocks",
    "28-hard-line-breaks",
    "29-non-breaking-space",
    "30-raw-inline",
    "31-ordered-list-start-and-delimiter",
    "32-ordered-list-dialects",
    "33-editorial-markup",
    "34-thematic-breaks",
    "35-cross-reference",
    "36-autolinks",
    "37-escapes",
    "38-bare-urls-stay-literal",
    "39-inline-span",
    "40-superscript-and-subscript",
    "41-line-blocks",
    "42-admonitions",
    "43-abbreviations",
    "44-mentions-and-tags",
    "45-inline-extensions",
    "46-symbols",
    "47-numbered-cross-references",
    "48-table-column-alignment",
    "49-table-per-cell-alignment-override",
    "50-headerless-table-alignment",
    "51-table-without-alignment",
    "52-table-alignment-with-colspan",
    "53-table-doubled-alignment-marker",
    "54-fenced-code-shorter-inner-fence",
    "55-blockquote-caption-after-a-blank-line",
    "56-table-cell-escaped-pipe",
    "57-table-cell-pipe-inside-code-span",
    "58-abbreviation-matches-on-word-boundaries-only",
    "59-mention-ignores-email-addresses",
    "60-tag-requires-a-word-boundary",
    "61-table-stacked-rowspan",
    "62-smart-typography-escapes-and-code",
    "63-table-multi-line-cell-continuation",
    "64-table-rowspan-with-multi-line-content",
    "65-ordered-marker-vs-prose",
    "66-footnote-with-multiple-blocks",
    "67-empty-delimiters",
    "68-nested-containers",
    "69-attribute-edge-cases",
    "70-escape-coverage",
    "71-parenthesized-ordered-marker",
    "72-emphasis-edge-cases",
    "73-list-nesting-and-looseness",
    "74-doubled-emphasis-delimiters",
    "75-nested-brackets-in-link-text",
    "76-reference-labels-are-case-sensitive",
    "77-two-char-delimiter-runs",
    "78-trailing-attribute-block-edge-cases",
    "79-paragraph-interruption",
    "80-blockquote-lazy-continuation",
    "81-fenced-code-language-with-punctuation",
    "82-multi-line-headings",
    "83-blockquote-lazy-continuation-stops-at-a-fenced-block",
    "84-list-lazy-continuation",
    "85-compact-list-blocks",
    "86-list-continuation-marker",
    "87-block-attribute-lines",
    "88-list-item-attributes",
    "89-mention-and-tag-name-boundaries",
    "90-superscript-in-a-table-cell",
    "91-nested-comment-fences",
    "92-strong-emphasis-starting-with-a-link",
    "93-abbreviation-definition-interrupts-a-paragraph",
    "94-literal-less-than-in-prose",
    "95-boolean-attributes",
    "96-table-span-marker-in-first-column",
    "97-table-cell-attributes",
    "98-table-row-attributes",
    "99-table-header-cell-rowspan",
    "100-block-quote-continuation-marker",
    "101-heading-marker-column-zero",
    "102-paragraph-trailing-whitespace",
    "103-marker-line-nested-lists",
    "104-blocked-span-marker-renders-as-empty-cell",
    "105-colspan-marker-scans-left-past-a-consumed-cell",
    "106-security-hardening",
    "107-link-destination-stops-at-the-first-parenthesis",
    "108-empty-link-and-image-titles-are-preserved",
    "109-cross-references-resolve-inside-footnote-bodies",
    "110-unquoted-attribute-values-may-contain-dots-and-colons",
    "111-a-pipe-pair-with-no-cell-is-not-a-table",
    "112-adjacent-attribute-blocks-on-one-line-merge",
    "113-a-continuation-row-needs-a-body-row",
    "114-fence-opener-with-a-nested-list-body-inside-a-list-item",
    "115-footnote-definition-inside-a-container-is-collected",
    "116-cyclic-cross-reference-resolves-to-one-level",
    "117-trojan-source-heading-ids-are-nfc-normalized-and-strip-invisible-controls",
    "118-trojan-source-rendered-text-and-code-strip-bidi-override-controls",
    "119-scheme-probe-strips-unicode-whitespace",
    "120-footnotes-placement",
    "121-classes-are-deduplicated",
    "122-code-span-and-image-trailing-attributes-are-strict",
    "123-a-bare-attribute-block-on-its-own-line-is-literal",
    "124-a-backslash-in-a-link-destination-is-a-literal-character",
    "125-autolink-display-keeps-the-raw-content",
    "126-editorial-markup-takes-a-trailing-attribute",
    "127-emphasis-opener-slash-adjacency",
    "128-bold-italic-delimiter-needs-content",
    "129-emphasis-span-closes-before-a-following-delimiter",
    "130-thematic-break-requires-contiguous-markers",
    "131-sublist-marker-interrupts-a-continuation-paragraph",
    "132-footnote-definition-requires-an-inline-body",
    "133-footnote-definition-separator-must-be-a-space",
    "134-link-reference-definition-separator-must-be-a-space",
    "135-abbreviation-definition-separator-must-be-a-space",
    "136-unclaimed-openers-stay-literal",
    "137-include-directive-with-no-resolver-renders-literal",
];

fn corpus_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("tests/spec/tests/corpus")
}

fn corpus_pairs() -> Vec<String> {
    let dir = corpus_dir();
    if !dir.exists() {
        panic!(
            "Spec corpus not found at {}.\n\
             Did you initialize the submodule?\n  git submodule update --init",
            dir.display()
        );
    }
    let mut out: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("crv") {
                return None;
            }
            path.file_stem().and_then(|s| s.to_str()).map(String::from)
        })
        .collect();
    out.sort();
    out
}

fn check_pair(slug: &str) {
    let dir = corpus_dir();
    let crv = dir.join(format!("{slug}.crv"));
    let html = dir.join(format!("{slug}.html"));
    let source = fs::read_to_string(&crv).unwrap_or_else(|e| panic!("read {}: {e}", crv.display()));
    let expected =
        fs::read_to_string(&html).unwrap_or_else(|e| panic!("read {}: {e}", html.display()));
    let actual = carve::to_html(&source);
    pretty_assert_eq(slug, expected.trim(), actual.trim());
}

fn is_implemented_pair(slug: &str) -> bool {
    IMPLEMENTED.iter().any(|implemented| {
        slug == *implemented
            || slug.strip_prefix(implemented).is_some_and(|rest| {
                rest.starts_with('-') && rest[1..].bytes().all(|b| b.is_ascii_digit())
            })
    })
}

fn pretty_assert_eq(slug: &str, expected: &str, actual: &str) {
    if expected == actual {
        return;
    }
    panic!(
        "corpus pair `{slug}` did not match.\n\n\
         ----- expected -----\n{expected}\n\
         ----- actual -------\n{actual}\n\
         --------------------\n",
    );
}

#[test]
fn corpus_pairs_present() {
    assert!(!corpus_pairs().is_empty(), "no .crv files found in corpus");
}

/// Reduce a corpus pair slug to its base category: `NN-slug` or
/// `NN-slug-MM` -> `NN-slug`. A trailing `-<digits>` is a variant suffix and is
/// dropped so all variants of a category map to the single IMPLEMENTED entry.
fn base_category(slug: &str) -> &str {
    if let Some((head, tail)) = slug.rsplit_once('-') {
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            return head;
        }
    }
    slug
}

/// Reverse of `all_implemented_pairs_exist`: every base category present in the
/// corpus submodule must appear in IMPLEMENTED. Without this, a brand-new spec
/// corpus category is silently unchecked (this gap once left
/// `100-block-quote-continuation-marker` unvalidated). A missing category here
/// forces an IMPLEMENTED update.
#[test]
fn all_corpus_categories_implemented() {
    let mut missing: Vec<String> = Vec::new();
    for slug in corpus_pairs() {
        let category = base_category(&slug);
        if !IMPLEMENTED.contains(&category) {
            let category = category.to_string();
            if !missing.contains(&category) {
                missing.push(category);
            }
        }
    }
    missing.sort();
    assert!(
        missing.is_empty(),
        "corpus categories not in IMPLEMENTED (add them, then implement/verify): {missing:?}"
    );
}

#[test]
fn all_implemented_pairs_exist() {
    let pairs = corpus_pairs();
    for slug in IMPLEMENTED {
        assert!(
            pairs.iter().any(|p| p == slug),
            "IMPLEMENTED references `{slug}` but no such corpus pair exists. \
             Either the slug is wrong or the submodule is out of date."
        );
    }
}

#[test]
fn all_implemented_corpus_pairs_match() {
    for slug in corpus_pairs() {
        if is_implemented_pair(&slug) {
            check_pair(&slug);
        }
    }
}

// One generated test function per implemented base category, keeping panics
// scoped so failures point at the responsible pair. One representative slug
// (the un-suffixed base pair) per category; every variant pair is still
// checked by `all_implemented_corpus_pairs_match`.
macro_rules! corpus_test {
    ($name:ident, $slug:literal) => {
        #[test]
        fn $name() {
            check_pair($slug);
        }
    };
}

corpus_test!(c01_emphasis, "01-emphasis");
corpus_test!(c02_headings, "02-headings");
corpus_test!(c03_links, "03-links");
corpus_test!(c04_images, "04-images");
corpus_test!(c05_lists, "05-lists");
corpus_test!(c06_task_lists, "06-task-lists");
corpus_test!(
    c07_blockquote_with_attribution,
    "07-blockquote-with-attribution"
);
corpus_test!(c08_image_with_caption, "08-image-with-caption");
corpus_test!(c09_tables, "09-tables");
corpus_test!(
    c10_tables_with_rowspan_and_colspan,
    "10-tables-with-rowspan-and-colspan"
);
corpus_test!(c11_fenced_code, "11-fenced-code");
corpus_test!(c12_inline_code, "12-inline-code");
corpus_test!(c13_attributes, "13-attributes");
corpus_test!(c14_frontmatter, "14-frontmatter");
corpus_test!(c15_heading_ids, "15-heading-ids");
corpus_test!(c16_reference_link, "16-reference-link");
corpus_test!(c17_collapsed_reference_link, "17-collapsed-reference-link");
corpus_test!(
    c18_unresolved_reference_link,
    "18-unresolved-reference-link"
);
corpus_test!(
    c19_smart_typography_dashes_and_quotes,
    "19-smart-typography-dashes-and-quotes"
);
corpus_test!(
    c20_smart_typography_arrows_and_symbols,
    "20-smart-typography-arrows-and-symbols"
);
corpus_test!(c21_math, "21-math");
corpus_test!(c22_footnotes, "22-footnotes");
corpus_test!(c23_inline_footnotes, "23-inline-footnotes");
corpus_test!(c24_generic_divs, "24-generic-divs");
corpus_test!(c25_definition_lists, "25-definition-lists");
corpus_test!(c26_comments, "26-comments");
corpus_test!(c27_raw_blocks, "27-raw-blocks");
corpus_test!(c28_hard_line_breaks, "28-hard-line-breaks");
corpus_test!(c29_non_breaking_space, "29-non-breaking-space");
corpus_test!(c30_raw_inline, "30-raw-inline");
corpus_test!(
    c31_ordered_list_start_and_delimiter,
    "31-ordered-list-start-and-delimiter"
);
corpus_test!(c32_ordered_list_dialects, "32-ordered-list-dialects");
corpus_test!(c33_editorial_markup, "33-editorial-markup");
corpus_test!(c34_thematic_breaks, "34-thematic-breaks");
corpus_test!(c35_cross_reference, "35-cross-reference");
corpus_test!(c36_autolinks, "36-autolinks");
corpus_test!(c37_escapes, "37-escapes");
corpus_test!(c38_bare_urls_stay_literal, "38-bare-urls-stay-literal");
corpus_test!(c39_inline_span, "39-inline-span");
corpus_test!(
    c40_superscript_and_subscript,
    "40-superscript-and-subscript"
);
corpus_test!(c41_line_blocks, "41-line-blocks");
corpus_test!(c42_admonitions, "42-admonitions");
corpus_test!(c43_abbreviations, "43-abbreviations");
corpus_test!(c44_mentions_and_tags, "44-mentions-and-tags");
corpus_test!(c45_inline_extensions, "45-inline-extensions");
corpus_test!(c46_symbols, "46-symbols");
corpus_test!(
    c47_numbered_cross_references,
    "47-numbered-cross-references"
);
corpus_test!(c48_table_column_alignment, "48-table-column-alignment");
corpus_test!(
    c49_table_per_cell_alignment_override,
    "49-table-per-cell-alignment-override"
);
corpus_test!(
    c50_headerless_table_alignment,
    "50-headerless-table-alignment"
);
corpus_test!(c51_table_without_alignment, "51-table-without-alignment");
corpus_test!(
    c52_table_alignment_with_colspan,
    "52-table-alignment-with-colspan"
);
corpus_test!(
    c53_table_doubled_alignment_marker,
    "53-table-doubled-alignment-marker"
);
corpus_test!(
    c54_fenced_code_shorter_inner_fence,
    "54-fenced-code-shorter-inner-fence"
);
corpus_test!(
    c55_blockquote_caption_after_a_blank_line,
    "55-blockquote-caption-after-a-blank-line"
);
corpus_test!(c56_table_cell_escaped_pipe, "56-table-cell-escaped-pipe");
corpus_test!(
    c57_table_cell_pipe_inside_code_span,
    "57-table-cell-pipe-inside-code-span"
);
corpus_test!(
    c58_abbreviation_matches_on_word_boundaries_only,
    "58-abbreviation-matches-on-word-boundaries-only"
);
corpus_test!(
    c59_mention_ignores_email_addresses,
    "59-mention-ignores-email-addresses"
);
corpus_test!(
    c60_tag_requires_a_word_boundary,
    "60-tag-requires-a-word-boundary"
);
corpus_test!(c61_table_stacked_rowspan, "61-table-stacked-rowspan");
corpus_test!(
    c62_smart_typography_escapes_and_code,
    "62-smart-typography-escapes-and-code"
);
corpus_test!(
    c63_table_multi_line_cell_continuation,
    "63-table-multi-line-cell-continuation"
);
corpus_test!(
    c64_table_rowspan_with_multi_line_content,
    "64-table-rowspan-with-multi-line-content"
);
corpus_test!(c65_ordered_marker_vs_prose, "65-ordered-marker-vs-prose");
corpus_test!(
    c66_footnote_with_multiple_blocks,
    "66-footnote-with-multiple-blocks"
);
corpus_test!(c67_empty_delimiters, "67-empty-delimiters");
corpus_test!(c68_nested_containers, "68-nested-containers");
corpus_test!(c69_attribute_edge_cases, "69-attribute-edge-cases");
corpus_test!(c70_escape_coverage, "70-escape-coverage");
corpus_test!(
    c71_parenthesized_ordered_marker,
    "71-parenthesized-ordered-marker"
);
corpus_test!(c72_emphasis_edge_cases, "72-emphasis-edge-cases");
corpus_test!(
    c73_list_nesting_and_looseness,
    "73-list-nesting-and-looseness"
);
corpus_test!(
    c74_doubled_emphasis_delimiters,
    "74-doubled-emphasis-delimiters"
);
corpus_test!(
    c75_nested_brackets_in_link_text,
    "75-nested-brackets-in-link-text"
);
corpus_test!(
    c76_reference_labels_are_case_sensitive,
    "76-reference-labels-are-case-sensitive"
);
corpus_test!(c77_two_char_delimiter_runs, "77-two-char-delimiter-runs");
corpus_test!(
    c78_trailing_attribute_block_edge_cases,
    "78-trailing-attribute-block-edge-cases"
);
corpus_test!(c79_paragraph_interruption, "79-paragraph-interruption");
corpus_test!(
    c80_blockquote_lazy_continuation,
    "80-blockquote-lazy-continuation"
);
corpus_test!(
    c81_fenced_code_language_with_punctuation,
    "81-fenced-code-language-with-punctuation"
);
corpus_test!(c82_multi_line_headings, "82-multi-line-headings");
corpus_test!(
    c83_blockquote_lazy_continuation_stops_at_a_fenced_block,
    "83-blockquote-lazy-continuation-stops-at-a-fenced-block"
);
corpus_test!(c84_list_lazy_continuation, "84-list-lazy-continuation");
corpus_test!(c85_compact_list_blocks, "85-compact-list-blocks");
corpus_test!(c86_list_continuation_marker, "86-list-continuation-marker");
corpus_test!(c87_block_attribute_lines, "87-block-attribute-lines");
corpus_test!(c88_list_item_attributes, "88-list-item-attributes");
corpus_test!(
    c89_mention_and_tag_name_boundaries,
    "89-mention-and-tag-name-boundaries"
);
corpus_test!(
    c90_superscript_in_a_table_cell,
    "90-superscript-in-a-table-cell"
);
corpus_test!(c91_nested_comment_fences, "91-nested-comment-fences");
corpus_test!(
    c92_strong_emphasis_starting_with_a_link,
    "92-strong-emphasis-starting-with-a-link"
);
corpus_test!(
    c93_abbreviation_definition_interrupts_a_paragraph,
    "93-abbreviation-definition-interrupts-a-paragraph"
);
corpus_test!(
    c94_literal_less_than_in_prose,
    "94-literal-less-than-in-prose"
);
corpus_test!(c95_boolean_attributes, "95-boolean-attributes");
corpus_test!(
    c96_table_span_marker_in_first_column,
    "96-table-span-marker-in-first-column"
);
corpus_test!(c97_table_cell_attributes, "97-table-cell-attributes");
corpus_test!(c98_table_row_attributes, "98-table-row-attributes");
corpus_test!(
    c99_table_header_cell_rowspan,
    "99-table-header-cell-rowspan"
);
corpus_test!(
    c100_block_quote_continuation_marker,
    "100-block-quote-continuation-marker"
);
corpus_test!(
    c101_heading_marker_column_zero,
    "101-heading-marker-column-zero"
);
corpus_test!(
    c102_paragraph_trailing_whitespace,
    "102-paragraph-trailing-whitespace"
);
corpus_test!(
    c103_marker_line_nested_lists,
    "103-marker-line-nested-lists"
);
corpus_test!(
    c104_blocked_span_marker_renders_as_empty_cell,
    "104-blocked-span-marker-renders-as-empty-cell"
);
corpus_test!(
    c105_colspan_marker_scans_left_past_a_consumed_cell,
    "105-colspan-marker-scans-left-past-a-consumed-cell"
);
corpus_test!(c106_security_hardening, "106-security-hardening");
corpus_test!(
    c107_link_destination_stops_at_the_first_parenthesis,
    "107-link-destination-stops-at-the-first-parenthesis"
);
corpus_test!(
    c108_empty_link_and_image_titles_are_preserved,
    "108-empty-link-and-image-titles-are-preserved"
);
corpus_test!(
    c109_cross_references_resolve_inside_footnote_bodies,
    "109-cross-references-resolve-inside-footnote-bodies"
);
corpus_test!(
    c110_unquoted_attribute_values_may_contain_dots_and_colons,
    "110-unquoted-attribute-values-may-contain-dots-and-colons"
);
corpus_test!(
    c111_a_pipe_pair_with_no_cell_is_not_a_table,
    "111-a-pipe-pair-with-no-cell-is-not-a-table"
);
corpus_test!(
    c112_adjacent_attribute_blocks_on_one_line_merge,
    "112-adjacent-attribute-blocks-on-one-line-merge"
);
corpus_test!(
    c113_a_continuation_row_needs_a_body_row,
    "113-a-continuation-row-needs-a-body-row"
);
corpus_test!(
    c114_fence_opener_with_a_nested_list_body_inside_a_list_item,
    "114-fence-opener-with-a-nested-list-body-inside-a-list-item"
);
corpus_test!(
    c115_footnote_definition_inside_a_container_is_collected,
    "115-footnote-definition-inside-a-container-is-collected"
);
corpus_test!(
    c116_cyclic_cross_reference_resolves_to_one_level,
    "116-cyclic-cross-reference-resolves-to-one-level"
);
corpus_test!(
    c117_trojan_source_heading_ids_are_nfc_normalized_and_strip_invisible_controls,
    "117-trojan-source-heading-ids-are-nfc-normalized-and-strip-invisible-controls"
);
corpus_test!(
    c118_trojan_source_rendered_text_and_code_strip_bidi_override_controls,
    "118-trojan-source-rendered-text-and-code-strip-bidi-override-controls"
);
corpus_test!(
    c119_scheme_probe_strips_unicode_whitespace,
    "119-scheme-probe-strips-unicode-whitespace"
);
corpus_test!(c120_footnotes_placement, "120-footnotes-placement");
corpus_test!(
    c121_classes_are_deduplicated,
    "121-classes-are-deduplicated"
);
corpus_test!(
    c122_code_span_and_image_trailing_attributes_are_strict,
    "122-code-span-and-image-trailing-attributes-are-strict"
);
corpus_test!(
    c123_a_bare_attribute_block_on_its_own_line_is_literal,
    "123-a-bare-attribute-block-on-its-own-line-is-literal"
);
corpus_test!(
    c124_a_backslash_in_a_link_destination_is_a_literal_character,
    "124-a-backslash-in-a-link-destination-is-a-literal-character"
);
corpus_test!(
    c125_autolink_display_keeps_the_raw_content,
    "125-autolink-display-keeps-the-raw-content"
);
corpus_test!(
    c126_editorial_markup_takes_a_trailing_attribute,
    "126-editorial-markup-takes-a-trailing-attribute"
);
corpus_test!(
    c127_emphasis_opener_slash_adjacency,
    "127-emphasis-opener-slash-adjacency"
);
corpus_test!(
    c128_bold_italic_delimiter_needs_content,
    "128-bold-italic-delimiter-needs-content"
);
corpus_test!(
    c129_emphasis_span_closes_before_a_following_delimiter,
    "129-emphasis-span-closes-before-a-following-delimiter"
);
corpus_test!(
    c130_thematic_break_requires_contiguous_markers,
    "130-thematic-break-requires-contiguous-markers"
);
corpus_test!(
    c131_sublist_marker_interrupts_a_continuation_paragraph,
    "131-sublist-marker-interrupts-a-continuation-paragraph"
);
corpus_test!(
    c132_footnote_definition_requires_an_inline_body,
    "132-footnote-definition-requires-an-inline-body"
);
corpus_test!(
    c133_footnote_definition_separator_must_be_a_space,
    "133-footnote-definition-separator-must-be-a-space"
);
corpus_test!(
    c134_link_reference_definition_separator_must_be_a_space,
    "134-link-reference-definition-separator-must-be-a-space"
);
corpus_test!(
    c135_abbreviation_definition_separator_must_be_a_space,
    "135-abbreviation-definition-separator-must-be-a-space"
);
