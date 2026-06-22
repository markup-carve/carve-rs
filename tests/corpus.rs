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
    "13-admonitions",
    "14-abbreviations",
    "15-mentions-and-tags",
    "16-inline-extensions",
    "17-attributes",
    "18-frontmatter",
    "19-heading-ids",
    "20-table-column-alignment",
    "21-table-per-cell-alignment-override",
    "22-headerless-table-alignment",
    "23-table-without-alignment",
    "24-table-alignment-with-colspan",
    "25-table-doubled-alignment-marker",
    "26-fenced-code-shorter-inner-fence",
    "27-blockquote-caption-after-a-blank-line",
    "28-table-cell-escaped-pipe",
    "29-table-cell-pipe-inside-code-span",
    "30-abbreviation-matches-on-word-boundaries-only",
    "31-mention-ignores-email-addresses",
    "32-tag-requires-a-word-boundary",
    "33-table-stacked-rowspan",
    "34-reference-link",
    "35-collapsed-reference-link",
    "36-unresolved-reference-link",
    "37-smart-typography-dashes-and-quotes",
    "38-smart-typography-arrows-and-symbols",
    "39-smart-typography-escapes-and-code",
    "40-table-multi-line-cell-continuation",
    "41-table-rowspan-with-multi-line-content",
    "42-math",
    "43-footnotes",
    "44-generic-divs",
    "45-definition-lists",
    "46-comments",
    "47-raw-blocks",
    "48-hard-line-breaks",
    "49-non-breaking-space",
    "50-raw-inline",
    "51-emoji",
    "52-ordered-list-start-and-delimiter",
    "53-ordered-list-dialects",
    "54-ordered-marker-vs-prose",
    "55-footnote-with-multiple-blocks",
    "56-editorial-markup",
    "57-thematic-breaks",
    "58-cross-reference",
    "59-autolinks",
    "60-escapes",
    "61-empty-delimiters",
    "62-bare-urls-stay-literal",
    "63-nested-containers",
    "64-attribute-edge-cases",
    "65-escape-coverage",
    "66-inline-span",
    "67-superscript-and-subscript",
    "68-parenthesized-ordered-marker",
    "69-emphasis-edge-cases",
    "70-list-nesting-and-looseness",
    "71-doubled-emphasis-delimiters",
    "72-nested-brackets-in-link-text",
    "73-reference-labels-are-case-sensitive",
    "74-two-char-delimiter-runs",
    "75-trailing-attribute-block-edge-cases",
    "76-paragraph-interruption",
    "77-blockquote-lazy-continuation",
    "78-fenced-code-language-with-punctuation",
    "79-multi-line-headings",
    "80-blockquote-lazy-continuation-stops-at-a-fenced-block",
    "81-list-lazy-continuation",
    "82-compact-list-blocks",
    "83-list-continuation-marker",
    "84-block-attribute-lines",
    "85-numbered-cross-references",
    "86-inline-footnotes",
    "87-list-item-attributes",
    "88-line-blocks",
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

// One generated test function per implemented slug — keeps panics
// scoped so failures point at the responsible pair.
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
corpus_test!(c13_admonitions, "13-admonitions");
corpus_test!(c14_abbreviations, "14-abbreviations");
corpus_test!(c15_mentions_and_tags, "15-mentions-and-tags");
corpus_test!(c16_inline_extensions, "16-inline-extensions");
corpus_test!(c17_attributes, "17-attributes");
corpus_test!(c18_frontmatter, "18-frontmatter");
corpus_test!(
    c85_numbered_cross_references,
    "85-numbered-cross-references"
);
corpus_test!(c86_inline_footnotes, "86-inline-footnotes");
corpus_test!(c19_heading_ids, "19-heading-ids");
corpus_test!(c20_table_column_alignment, "20-table-column-alignment");
corpus_test!(
    c21_table_per_cell_alignment_override,
    "21-table-per-cell-alignment-override"
);
corpus_test!(
    c22_headerless_table_alignment,
    "22-headerless-table-alignment"
);
corpus_test!(c23_table_without_alignment, "23-table-without-alignment");
corpus_test!(
    c24_table_alignment_with_colspan,
    "24-table-alignment-with-colspan"
);
corpus_test!(
    c25_table_doubled_alignment_marker,
    "25-table-doubled-alignment-marker"
);
corpus_test!(
    c26_fenced_code_shorter_inner_fence,
    "26-fenced-code-shorter-inner-fence"
);
corpus_test!(
    c27_blockquote_caption_after_a_blank_line,
    "27-blockquote-caption-after-a-blank-line"
);
corpus_test!(c28_table_cell_escaped_pipe, "28-table-cell-escaped-pipe");
corpus_test!(
    c29_table_cell_pipe_inside_code_span,
    "29-table-cell-pipe-inside-code-span"
);
corpus_test!(
    c30_abbreviation_matches_on_word_boundaries_only,
    "30-abbreviation-matches-on-word-boundaries-only"
);
corpus_test!(
    c31_mention_ignores_email_addresses,
    "31-mention-ignores-email-addresses"
);
corpus_test!(
    c32_tag_requires_a_word_boundary,
    "32-tag-requires-a-word-boundary"
);
corpus_test!(c33_table_stacked_rowspan, "33-table-stacked-rowspan");
corpus_test!(c34_reference_link, "34-reference-link");
corpus_test!(c35_collapsed_reference_link, "35-collapsed-reference-link");
corpus_test!(
    c36_unresolved_reference_link,
    "36-unresolved-reference-link"
);
corpus_test!(
    c37_smart_typography_dashes_and_quotes,
    "37-smart-typography-dashes-and-quotes"
);
corpus_test!(
    c38_smart_typography_arrows_and_symbols,
    "38-smart-typography-arrows-and-symbols"
);
corpus_test!(
    c39_smart_typography_escapes_and_code,
    "39-smart-typography-escapes-and-code"
);
corpus_test!(
    c40_table_multi_line_cell_continuation,
    "40-table-multi-line-cell-continuation"
);
corpus_test!(
    c41_table_rowspan_with_multi_line_content,
    "41-table-rowspan-with-multi-line-content"
);
corpus_test!(c42_math, "42-math");
corpus_test!(c43_footnotes, "43-footnotes");
corpus_test!(c44_generic_divs, "44-generic-divs");
corpus_test!(c45_definition_lists, "45-definition-lists");
corpus_test!(c46_comments, "46-comments");
corpus_test!(c47_raw_blocks, "47-raw-blocks");
corpus_test!(c48_hard_line_breaks, "48-hard-line-breaks");
corpus_test!(c49_non_breaking_space, "49-non-breaking-space");
corpus_test!(c50_raw_inline, "50-raw-inline");
corpus_test!(c51_emoji, "51-emoji");
corpus_test!(
    c52_ordered_list_start_and_delimiter,
    "52-ordered-list-start-and-delimiter"
);
corpus_test!(c53_ordered_list_dialects, "53-ordered-list-dialects");
corpus_test!(c54_ordered_marker_vs_prose, "54-ordered-marker-vs-prose");
corpus_test!(
    c55_footnote_with_multiple_blocks,
    "55-footnote-with-multiple-blocks"
);
corpus_test!(c56_editorial_markup, "56-editorial-markup");
corpus_test!(c57_thematic_breaks, "57-thematic-breaks");
corpus_test!(c58_cross_reference, "58-cross-reference");
corpus_test!(c59_autolinks, "59-autolinks");
corpus_test!(c60_escapes, "60-escapes");
corpus_test!(c61_empty_delimiters, "61-empty-delimiters");
corpus_test!(c62_bare_urls_stay_literal, "62-bare-urls-stay-literal");
corpus_test!(c63_nested_containers, "63-nested-containers");
corpus_test!(c64_attribute_edge_cases, "64-attribute-edge-cases");
corpus_test!(c65_escape_coverage, "65-escape-coverage");
corpus_test!(c66_inline_span, "66-inline-span");
corpus_test!(
    c67_superscript_and_subscript,
    "67-superscript-and-subscript"
);
corpus_test!(
    c68_parenthesized_ordered_marker,
    "68-parenthesized-ordered-marker"
);
corpus_test!(c69_emphasis_edge_cases, "69-emphasis-edge-cases");
corpus_test!(
    c70_list_nesting_and_looseness,
    "70-list-nesting-and-looseness"
);
corpus_test!(
    c71_doubled_emphasis_delimiters,
    "71-doubled-emphasis-delimiters"
);
corpus_test!(
    c72_nested_brackets_in_link_text,
    "72-nested-brackets-in-link-text"
);
corpus_test!(
    c73_reference_labels_are_case_sensitive,
    "73-reference-labels-are-case-sensitive"
);
corpus_test!(c74_two_char_delimiter_runs, "74-two-char-delimiter-runs");
corpus_test!(
    c75_trailing_attribute_block_edge_cases,
    "75-trailing-attribute-block-edge-cases"
);
corpus_test!(c76_paragraph_interruption, "76-paragraph-interruption");
corpus_test!(
    c77_blockquote_lazy_continuation,
    "77-blockquote-lazy-continuation"
);
corpus_test!(
    c78_fenced_code_language_with_punctuation,
    "78-fenced-code-language-with-punctuation"
);
corpus_test!(c79_multi_line_headings, "79-multi-line-headings");
corpus_test!(
    c80_blockquote_lazy_continuation_stops_at_a_fenced_block,
    "80-blockquote-lazy-continuation-stops-at-a-fenced-block"
);
corpus_test!(c81_list_lazy_continuation, "81-list-lazy-continuation");
corpus_test!(c82_compact_list_blocks, "82-compact-list-blocks");
corpus_test!(c83_list_continuation_marker, "83-list-continuation-marker");
corpus_test!(c84_block_attribute_lines, "84-block-attribute-lines");
corpus_test!(c87_list_item_attributes, "87-list-item-attributes");
corpus_test!(c88_line_blocks, "88-line-blocks");
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
