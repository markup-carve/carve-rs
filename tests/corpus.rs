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
    "adjacent-block-openers-in-an-attached-run-stay-separate",
    "a-caption-attaches-across-one-blank-line",
    "a-container-a-lazy-line-folded-into-is-still-open",
    "two-blank-lines-detach-a-caption",
    "a-list-marker-at-the-content-column-inside-an-open-fence",
    "a-boundary-line-inside-an-open-fence-does-not-end-the-container",
    "a-flush-left-line-needs-an-open-paragraph-to-fold-into",
    "openers-past-the-nesting-cap-are-one-paragraph",
    "opaque-spans-inside-a-container",
    "blocks-that-render-to-nothing",
    "bare-dot-ordered-markers",
    "emphasis",
    "headings",
    "links",
    "images",
    "lists",
    "task-lists",
    "blockquote-with-attribution",
    "image-with-caption",
    "tables",
    "tables-with-rowspan-and-colspan",
    "fenced-code",
    "inline-code",
    "attributes",
    "frontmatter",
    "heading-ids",
    "reference-link",
    "collapsed-reference-link",
    "unresolved-reference-link",
    "smart-typography-dashes-and-quotes",
    "smart-typography-arrows-and-symbols",
    "math",
    "footnotes",
    "inline-footnotes",
    "generic-divs",
    "definition-lists",
    "comments",
    "raw-blocks",
    "hard-line-breaks",
    "non-breaking-space",
    "raw-inline",
    "ordered-list-start-and-delimiter",
    "ordered-list-dialects",
    "editorial-markup",
    "thematic-breaks",
    "cross-reference",
    "autolinks",
    "escapes",
    "bare-urls-stay-literal",
    "inline-span",
    "superscript-and-subscript",
    "line-blocks",
    "line-endings-and-a-byte-order-mark",
    "admonitions",
    "abbreviations",
    "mentions-and-tags",
    "inline-extensions",
    "symbols",
    "numbered-cross-references",
    "table-column-alignment",
    "table-per-cell-alignment-override",
    "headerless-table-alignment",
    "table-without-alignment",
    "table-alignment-with-colspan",
    "table-doubled-alignment-marker",
    "fenced-code-shorter-inner-fence",
    "blockquote-caption-after-a-blank-line",
    "table-cell-escaped-pipe",
    "table-cell-pipe-inside-code-span",
    "abbreviation-matches-on-word-boundaries-only",
    "mention-ignores-email-addresses",
    "tag-requires-a-word-boundary",
    "table-stacked-rowspan",
    "smart-typography-escapes-and-code",
    "table-multi-line-cell-continuation",
    "table-rowspan-with-multi-line-content",
    "ordered-marker-vs-prose",
    "footnote-with-multiple-blocks",
    "empty-delimiters",
    "nested-containers",
    "attribute-edge-cases",
    "escape-coverage",
    "parenthesized-ordered-marker",
    "emphasis-edge-cases",
    "list-nesting-and-looseness",
    "doubled-emphasis-delimiters",
    "nested-brackets-in-link-text",
    "reference-labels-are-case-sensitive",
    "two-char-delimiter-runs",
    "trailing-attribute-block-edge-cases",
    "paragraph-interruption",
    "blockquote-lazy-continuation",
    "fenced-code-language-with-punctuation",
    "single-line-headings",
    "blockquote-lazy-continuation-stops-at-a-fenced-block",
    "list-lazy-continuation",
    "compact-list-blocks",
    "list-continuation-marker",
    "block-attribute-lines",
    "list-item-attributes",
    "mention-and-tag-name-boundaries",
    "superscript-in-a-table-cell",
    "nested-comment-fences",
    "strong-emphasis-starting-with-a-link",
    "abbreviation-definition-interrupts-a-paragraph",
    "literal-less-than-in-prose",
    "boolean-attributes",
    "table-span-marker-in-first-column",
    "table-cell-attributes",
    "table-row-attributes",
    "table-header-cell-rowspan",
    "block-quote-continuation-marker",
    "heading-marker-column-zero",
    "paragraph-trailing-whitespace",
    "marker-line-nested-lists",
    "blocked-span-marker-renders-as-empty-cell",
    "colspan-marker-scans-left-past-a-consumed-cell",
    "security-hardening",
    "link-destination-parentheses-balance",
    "empty-link-and-image-titles-are-preserved",
    "cross-references-resolve-inside-footnote-bodies",
    "unquoted-attribute-values-may-contain-dots-and-colons",
    "a-pipe-pair-with-no-cell-is-not-a-table",
    "adjacent-attribute-blocks-on-one-line-merge",
    "a-continuation-row-needs-a-body-row",
    "fence-opener-with-a-nested-list-body-inside-a-list-item",
    "footnote-definition-inside-a-container-is-collected",
    "cyclic-cross-reference-resolves-to-one-level",
    "trojan-source-heading-ids-are-nfc-normalized-and-strip-invisible-controls",
    "trojan-source-rendered-text-and-code-strip-bidi-override-controls",
    "scheme-probe-strips-unicode-whitespace",
    "footnotes-placement",
    "classes-are-deduplicated",
    "code-span-and-image-trailing-attributes-are-strict",
    "a-bare-attribute-block-on-its-own-line-is-literal",
    "a-backslash-in-a-link-destination-is-a-literal-character",
    "autolink-display-keeps-the-raw-content",
    "editorial-markup-takes-a-trailing-attribute",
    "emphasis-opener-slash-adjacency",
    "bold-italic-delimiter-needs-content",
    "emphasis-span-closes-before-a-following-delimiter",
    "thematic-break-requires-contiguous-markers",
    "sublist-marker-interrupts-a-continuation-paragraph",
    "footnote-definition-requires-an-inline-body",
    "footnote-definition-separator-must-be-a-space",
    "link-reference-definition-separator-must-be-a-space",
    "abbreviation-definition-separator-must-be-a-space",
    "unclaimed-openers-stay-literal",
    "inline-literal",
    "all-space-verbatim-content",
    "trailing-whitespace-boundaries",
    "table-row-closing-pipe",
    "post-blank-list-continuation-content-column-model",
    "nested-item-looseness-does-not-propagate-to-the-outer-item",
    "definition-list-as-a-first-class-block-opener",
    "table-as-a-block-opener-in-a-list-item",
    "adjacent-slash-and-underscore-emphasis-nest",
    "colon-fence-as-a-block-opener-in-a-list-item",
    "fence-folds-as-lazy-inline-code-above-the-content-column",
    "abbreviation-title-escapes-its-markup-characters",
    "indented-ordered-marker-content-column-includes-the-marker-indent",
    "leading-attribute-brace-before-an-inline-span-stays-literal",
    "attribute-block-after-a-mention-stays-literal",
    "under-indented-definition-attaches-over-indented-definition-folds",
    "image-trailing-attribute-is-strict-about-the-glue",
    "wrapped-definition-term-continuation-below-the-content-column-strips-leading-whitespace",
    "indented-attribute-line-stays-literal",
    "indented-image-and-caption-stay-literal",
    "indented-reference-and-footnote-definitions-stay-literal",
    "indented-colon-fence-blocks-stay-literal",
    "below-content-column-div-body-in-a-list-item-stays-literal",
    "outer-item-with-an-internal-blank-before-an-attached-block-is-loose",
    "unresolved-footnote-reference-with-a-trailing-attribute-stays-literal",
    "tight-list-item-keeps-trailing-text-after-a-block-bare",
    "quote-flanking-after-an-escaped-character",
    "comment-fence-with-trailing-text",
    "unterminated-comment-fence",
    "widened-verbatim-fences",
    "only-the-id-hoists-to-the-section-wrapper",
    "headings-inside-containers-are-not-wrapped",
    "attribute-order-on-an-unwrapped-heading",
    "attribute-braces-on-a-list-item-marker-line",
    "implicit-heading-references-with-no-definition",
    "a-marker-separator-is-a-space-never-a-tab",
    "a-continuation-row-carries-no-trailing-text",
    "a-definition-attached-by-a-continuation-marker-is-collected-and-the-item-keeps-no-trace",
    "a-definition-inside-a-definition-list-dd-is-collected-and-the-entry-keeps-no-trace",
    "a-footnote-body-s-last-block-when-it-is-not-a-paragraph-gets-a-synthesized-paragraph-for-the-backlink",
    "a-format-character-before-a-scheme-is-not-stripped-and-is-inert",
    "a-line-at-a-footnote-definition-s-own-column-followed-by-non-blank-text-forms-its-own-tight-block",
    "a-single-percent-is-not-a-comment",
    "a-tab-after-a-heading-quote-or-caption-marker-leaves-the-line-as-prose",
    "a-tab-reaches-a-footnote-body-s-column-just-as-two-spaces-do",
    "a-table-delimiter-cell-needs-at-least-one-dash",
    "an-abbreviation-term-is-one-ascii-alphanumeric-word",
    "an-at-sign-is-a-reference-label-character-everywhere-but-the-first-position",
    "an-empty-abbreviation-term-is-not-a-definition",
    "an-uppercase-roman-numeral-is-a-list-marker",
    "two-backticks-are-not-a-code-fence-opening-or-closing",
    "two-dashes-are-not-a-thematic-break",
    "a-link-definition-written-before-a-footnote-stays-before-it",
    "a-zero-width-character-in-a-reference-definition-destination",
    "a-block-image-is-separated-from-the-block-after-it-on-every-target",
    "a-tab-indent-is-the-column-it-reaches-whatever-the-line-holds",
    "a-tab-separates-two-attributes-and-pads-a-block-as-a-space-does",
    "the-same-column-written-with-four-spaces",
    "sibling-markers-that-reach-one-column-are-one-list",
    "heading-index-plain-text-covers-visible-leaves-and-rejects-an-empty-key",
    "the-continuation-marker-at-an-item-s-own-column-and-what-follows-it",
    "a-continuation-marker-after-a-blank-line-in-the-item",
    "a-continuation-marker-after-a-blank-line-in-a-loose-item",
    "an-attribute-name-admits-no-colon",
    "an-inline-attribute-block-does-not-span-lines-but-an-attribute-line-does",
    "trailing-whitespace-after-a-block-marker",
    "a-multi-line-raw-block-is-placed-at-its-opening-and-verbatim-after-it",
    "a-tab-as-the-first-character-of-a-definition-term",
    "a-repeated-definition-which-one-wins",
    "two-abbreviation-definitions",
    "an-abbreviation-definition-is-recognized-only-at-document-level",
    "a-list-item-does-not-define-an-abbreviation-either",
    "a-comment-is-recognized-at-any-column",
    "a-definition-below-every-content-column-folds-as-text",
    "a-caret-is-a-reference-label-not-an-empty-footnote",
    "an-invisible-line-does-not-cancel-a-blank-line-separation",
    "a-comment-fence-is-a-comment-at-any-column-too",
    "a-floating-attribute-stops-at-the-item-boundary",
    "a-comment-under-a-nested-item-does-not-close-it",
    "a-definition-inside-a-comment-registers-nothing",
    "a-blank-after-a-comment-still-ends-the-item",
    "a-comment-fence-under-a-nested-item-does-not-close-it-either",
    "a-collapsed-reference-is-matched-by-the-label-the-author-wrote",
    "an-abbreviation-at-a-list-item-s-content-column-is-still-not-a-definition",
    "a-definition-inside-a-container-is-collected-at-that-container-s-content-column",
    "trailing-attributes-on-a-link-reference-definition",
    "a-block-attribute-line-inside-a-quote-ends-the-paragraph-above-it",
    "a-collapsed-image-reference-uses-its-alt-text-as-the-label",
    "a-combined-bold-italic-span-may-cross-a-line",
    "a-comment-ends-the-paragraph-it-sits-under",
    "a-comment-fence-at-column-0-ends-the-item-a-line-does-not",
    "a-definition-on-a-footnote-body-s-continuation-line-is-collected",
    "a-description-line-needs-a-term-above-it",
    "a-div-does-not-define-an-abbreviation-either",
    "a-flush-left-line-after-a-footnote-definition-belongs-to-the-document",
    "a-footnote-body-holds-blocks-and-they-render-where-they-were-written",
    "a-heading-id-keeps-a-non-ascii-space",
    "a-heading-in-a-footnote-body-takes-an-id-but-no-section-wrapper",
    "a-marker-attribute-may-hold-a-quoted-brace",
    "a-nested-list-in-a-footnote-body-stays-nested",
    "a-quote-marker-is-plus-a-space-and-a-lazy-line-keeps-its-own-text",
    "a-reference-image-takes-a-caption",
    "a-tag-inside-a-literal-brace-run-is-still-a-tag",
    "an-attribute-line-inside-a-footnote-body-attaches-inside-it",
    "an-image-takes-a-reference-the-way-a-link-does",
    "an-unresolved-image-reference-stays-literal",
    "an-unresolved-reference-image-takes-no-caption",
    "one-definition-serves-a-link-and-an-image",
    "a-definition-below-a-footnote-body-s-column-is-the-document-s-own-text",
    "a-definition-past-a-footnote-body-s-column-is-the-body-s-own-text",
    "a-footnote-body-s-own-column-is-two-and-a-third-column-is-its-text",
    // The `[Café][]` half folds NFC, the `[file][]` half must NOT fold
    // compatibility - `# ﬁle` (U+FB01) stays unreachable. This engine already
    // produced the fixture byte-for-byte, so the entry is the whole change
    // (carve#725, carve#729).
    "a-heading-reference-folds-unicode-normalization-but-not-compatibility",
    // PART 7 decides these terminals by POSITION: a tab is syntax only in a
    // line's leading indentation run, so every slot on a colon-fence opener is
    // spelled `space`. The separator category enrolls the two-space opener as
    // well, which is what keeps the run from being narrowed to one space
    // (carve#908, carve-rs#722).
    "colon-fence-separator-must-be-a-space",
    "colon-fence-metadata-slots-must-be-a-space-too",
    // carve#912: four productions spell their padding slot as exactly ONE
    // `space`, and the lax readers narrow (carve-rs#744).
    "a-link-title-takes-exactly-one-space",
    "a-code-fence-opener-takes-exactly-one-space",
    "a-frontmatter-opener-takes-exactly-one-space",
    "a-reference-definition-s-metadata-slots-take-exactly-one-space",
    // carve#911: the definition line is ANCHORED at end of line, so what
    // follows the destination and the optional title makes the production fail
    // and the line is a paragraph (carve-rs#746). `16-reference-link-5` moved
    // with it.
    "a-reference-definition-is-anchored-at-end-of-line",
    // carve#892: the marker-to-content separator is a RUN of ASCII spaces, and
    // the first character that is not one BEGINS the content (carve-rs#748).
    "a-definition-marker-s-separator-is-a-space-and-it-is-a-run",
    // carve#926: a whitespace run at the end of a CONTENT LINE is dropped, on
    // every line and not just a block's last (carve-rs#751).
    "trailing-whitespace-on-a-content-line-is-dropped",
    // carve#844/#860: outside ASCII, `url_char` admits any character that is
    // not whitespace, not a format character and not a control character
    // (carve-rs#755).
    "an-autolink-body-admits-non-ascii-and-excludes-format-characters",
    // carve#906: every whitespace slot of the INLINE attribute block takes
    // `space`; the block-attribute LINE keeps `whitespace` (carve-rs#757).
    "the-inline-attribute-interior-is-space-only-the-attribute-line-is-not",
    // carve#888: `quoted_value` excludes a newline in both alternatives, so a
    // break inside the quotes ends the block (carve-rs#758).
    "a-quoted-attribute-value-stops-at-the-newline",
    // carve#939: PART 1 S4 folds a flush-left line into the innermost OPEN
    // paragraph, and an UNTERMINATED `::: ` div in a container holds one.
    "a-real-div-in-a-container-and-the-flush-left-line-after-it",
    // Categories this engine already produced byte for byte when they landed;
    // the entry is the whole change.
    "a-blank-line-holds-spaces-and-tabs-and-nothing-else",
    "a-definition-body-continuation-indented-past-its-column-is-lazy-text",
    "a-tab-continues-a-list-item-just-as-two-spaces-do",
    "an-absorbed-colon-fence-leaves-a-block-quote-s-paragraph-open",
    "code-fence-metadata-slots-must-be-a-space-too",
    "link-and-image-title-slots-must-be-a-space",
    "table-cell-padding-must-be-a-space",
    "the-flush-left-line-after-a-container-a-quoted-line-opened",
    "a-below-column-marker-after-a-comment-where-no-paragraph-is-open",
    "a-collapsed-reference-reaches-a-heading-by-the-heading-s-rendered-text",
    // carve#950: a fenced body is not a paragraph, so a line below the item's
    // content column closes the item instead of folding into it (carve-rs#770).
    // Three of its seven rows moved here; the other four already passed.
    "a-fence-opened-on-a-list-marker-line-body-below-the-content-column",
    "an-empty-footnote-body-is-written-with-the-empty-sentinel",
    "a-ragged-table-keeps-each-row-s-cell-count",
    "a-column-zero-definition-ends-an-open-list-item",
    "a-caret-line-does-not-end-a-paragraph-it-cannot-caption",
    "heading-index-plain-text-covers-visible-leaves-and-rejects-an-empty-key",
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
    let slug = &resolve_slug(slug);
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
    let slug = strip_leading_number(slug);
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
    let slug = strip_leading_number(slug);
    if let Some((head, tail)) = slug.rsplit_once('-') {
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            return head;
        }
    }
    slug
}

/// Drop a corpus file's leading `NN-`.
///
/// The number is the SPEC'S ORDERING, not an identity: inserting a section
/// upstream renumbers everything after it. Keying anything here by it meant a
/// renumbering reported ~70 categories as missing that had not changed at all,
/// and every `corpus_test!` below read a filename that had moved.
fn strip_leading_number(slug: &str) -> &str {
    match slug.split_once('-') {
        Some((head, rest)) if !head.is_empty() && head.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => slug,
    }
}

/// The on-disk slug for a category, whatever number it currently carries.
fn resolve_slug(slug: &str) -> String {
    if corpus_dir().join(format!("{slug}.crv")).is_file() {
        return slug.to_string();
    }
    corpus_pairs()
        .into_iter()
        .find(|pair| strip_leading_number(pair) == slug)
        .unwrap_or_else(|| panic!("no corpus pair for category {slug}"))
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
            pairs.iter().any(|p| strip_leading_number(p) == *slug),
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

corpus_test!(c01_emphasis, "emphasis");
corpus_test!(c02_headings, "headings");
corpus_test!(c03_links, "links");
corpus_test!(c04_images, "images");
corpus_test!(c05_lists, "lists");
corpus_test!(c06_task_lists, "task-lists");
corpus_test!(
    c07_blockquote_with_attribution,
    "blockquote-with-attribution"
);
corpus_test!(c08_image_with_caption, "image-with-caption");
corpus_test!(c09_tables, "tables");
corpus_test!(
    c10_tables_with_rowspan_and_colspan,
    "tables-with-rowspan-and-colspan"
);
corpus_test!(c11_fenced_code, "fenced-code");
corpus_test!(c12_inline_code, "inline-code");
corpus_test!(c13_attributes, "attributes");
corpus_test!(c14_frontmatter, "frontmatter");
corpus_test!(c15_heading_ids, "heading-ids");
corpus_test!(c16_reference_link, "reference-link");
corpus_test!(c17_collapsed_reference_link, "collapsed-reference-link");
corpus_test!(c18_unresolved_reference_link, "unresolved-reference-link");
corpus_test!(
    c19_smart_typography_dashes_and_quotes,
    "smart-typography-dashes-and-quotes"
);
corpus_test!(
    c20_smart_typography_arrows_and_symbols,
    "smart-typography-arrows-and-symbols"
);
corpus_test!(c21_math, "math");
corpus_test!(c22_footnotes, "footnotes");
corpus_test!(c23_inline_footnotes, "inline-footnotes");
corpus_test!(c24_generic_divs, "generic-divs");
corpus_test!(c25_definition_lists, "definition-lists");
corpus_test!(c26_comments, "comments");
corpus_test!(c27_raw_blocks, "raw-blocks");
corpus_test!(c28_hard_line_breaks, "hard-line-breaks");
corpus_test!(c29_non_breaking_space, "non-breaking-space");
corpus_test!(c30_raw_inline, "raw-inline");
corpus_test!(
    c31_ordered_list_start_and_delimiter,
    "ordered-list-start-and-delimiter"
);
corpus_test!(c32_ordered_list_dialects, "ordered-list-dialects");
corpus_test!(c33_editorial_markup, "editorial-markup");
corpus_test!(c34_thematic_breaks, "thematic-breaks");
corpus_test!(c35_cross_reference, "cross-reference");
corpus_test!(c36_autolinks, "autolinks");
corpus_test!(c37_escapes, "escapes");
corpus_test!(c38_bare_urls_stay_literal, "bare-urls-stay-literal");
corpus_test!(c39_inline_span, "inline-span");
corpus_test!(c40_superscript_and_subscript, "superscript-and-subscript");
corpus_test!(c41_line_blocks, "line-blocks");
corpus_test!(c42_admonitions, "admonitions");
corpus_test!(c43_abbreviations, "abbreviations");
corpus_test!(c44_mentions_and_tags, "mentions-and-tags");
corpus_test!(c45_inline_extensions, "inline-extensions");
corpus_test!(c46_symbols, "symbols");
corpus_test!(c47_numbered_cross_references, "numbered-cross-references");
corpus_test!(c48_table_column_alignment, "table-column-alignment");
corpus_test!(
    c49_table_per_cell_alignment_override,
    "table-per-cell-alignment-override"
);
corpus_test!(c50_headerless_table_alignment, "headerless-table-alignment");
corpus_test!(c51_table_without_alignment, "table-without-alignment");
corpus_test!(
    c52_table_alignment_with_colspan,
    "table-alignment-with-colspan"
);
corpus_test!(
    c53_table_doubled_alignment_marker,
    "table-doubled-alignment-marker"
);
corpus_test!(
    c54_fenced_code_shorter_inner_fence,
    "fenced-code-shorter-inner-fence"
);
corpus_test!(
    c55_blockquote_caption_after_a_blank_line,
    "blockquote-caption-after-a-blank-line"
);
corpus_test!(c56_table_cell_escaped_pipe, "table-cell-escaped-pipe");
corpus_test!(
    c57_table_cell_pipe_inside_code_span,
    "table-cell-pipe-inside-code-span"
);
corpus_test!(
    c58_abbreviation_matches_on_word_boundaries_only,
    "abbreviation-matches-on-word-boundaries-only"
);
corpus_test!(
    c59_mention_ignores_email_addresses,
    "mention-ignores-email-addresses"
);
corpus_test!(
    c60_tag_requires_a_word_boundary,
    "tag-requires-a-word-boundary"
);
corpus_test!(c61_table_stacked_rowspan, "table-stacked-rowspan");
corpus_test!(
    c62_smart_typography_escapes_and_code,
    "smart-typography-escapes-and-code"
);
corpus_test!(
    c63_table_multi_line_cell_continuation,
    "table-multi-line-cell-continuation"
);
corpus_test!(
    c64_table_rowspan_with_multi_line_content,
    "table-rowspan-with-multi-line-content"
);
corpus_test!(c65_ordered_marker_vs_prose, "ordered-marker-vs-prose");
corpus_test!(
    c66_footnote_with_multiple_blocks,
    "footnote-with-multiple-blocks"
);
corpus_test!(c67_empty_delimiters, "empty-delimiters");
corpus_test!(c68_nested_containers, "nested-containers");
corpus_test!(c69_attribute_edge_cases, "attribute-edge-cases");
corpus_test!(c70_escape_coverage, "escape-coverage");
corpus_test!(
    c71_parenthesized_ordered_marker,
    "parenthesized-ordered-marker"
);
corpus_test!(c72_emphasis_edge_cases, "emphasis-edge-cases");
corpus_test!(c73_list_nesting_and_looseness, "list-nesting-and-looseness");
corpus_test!(
    c74_doubled_emphasis_delimiters,
    "doubled-emphasis-delimiters"
);
corpus_test!(
    c75_nested_brackets_in_link_text,
    "nested-brackets-in-link-text"
);
corpus_test!(
    c76_reference_labels_are_case_sensitive,
    "reference-labels-are-case-sensitive"
);
corpus_test!(c77_two_char_delimiter_runs, "two-char-delimiter-runs");
corpus_test!(
    c78_trailing_attribute_block_edge_cases,
    "trailing-attribute-block-edge-cases"
);
corpus_test!(c79_paragraph_interruption, "paragraph-interruption");
corpus_test!(
    c80_blockquote_lazy_continuation,
    "blockquote-lazy-continuation"
);
corpus_test!(
    c81_fenced_code_language_with_punctuation,
    "fenced-code-language-with-punctuation"
);
corpus_test!(c82_single_line_headings, "single-line-headings");
corpus_test!(
    c83_blockquote_lazy_continuation_stops_at_a_fenced_block,
    "blockquote-lazy-continuation-stops-at-a-fenced-block"
);
corpus_test!(c84_list_lazy_continuation, "list-lazy-continuation");
corpus_test!(c85_compact_list_blocks, "compact-list-blocks");
corpus_test!(c86_list_continuation_marker, "list-continuation-marker");
corpus_test!(c87_block_attribute_lines, "block-attribute-lines");
corpus_test!(c88_list_item_attributes, "list-item-attributes");
corpus_test!(
    c89_mention_and_tag_name_boundaries,
    "mention-and-tag-name-boundaries"
);
corpus_test!(
    c90_superscript_in_a_table_cell,
    "superscript-in-a-table-cell"
);
corpus_test!(c91_nested_comment_fences, "nested-comment-fences");
corpus_test!(
    c92_strong_emphasis_starting_with_a_link,
    "strong-emphasis-starting-with-a-link"
);
corpus_test!(
    c93_abbreviation_definition_interrupts_a_paragraph,
    "abbreviation-definition-interrupts-a-paragraph"
);
corpus_test!(c94_literal_less_than_in_prose, "literal-less-than-in-prose");
corpus_test!(c95_boolean_attributes, "boolean-attributes");
corpus_test!(
    c96_table_span_marker_in_first_column,
    "table-span-marker-in-first-column"
);
corpus_test!(c97_table_cell_attributes, "table-cell-attributes");
corpus_test!(c98_table_row_attributes, "table-row-attributes");
corpus_test!(c99_table_header_cell_rowspan, "table-header-cell-rowspan");
corpus_test!(
    c100_block_quote_continuation_marker,
    "block-quote-continuation-marker"
);
corpus_test!(
    c101_heading_marker_column_zero,
    "heading-marker-column-zero"
);
corpus_test!(
    c102_paragraph_trailing_whitespace,
    "paragraph-trailing-whitespace"
);
corpus_test!(c103_marker_line_nested_lists, "marker-line-nested-lists");
corpus_test!(
    c104_blocked_span_marker_renders_as_empty_cell,
    "blocked-span-marker-renders-as-empty-cell"
);
corpus_test!(
    c105_colspan_marker_scans_left_past_a_consumed_cell,
    "colspan-marker-scans-left-past-a-consumed-cell"
);
corpus_test!(c106_security_hardening, "security-hardening");
corpus_test!(
    c107_link_destination_parentheses_balance,
    "link-destination-parentheses-balance"
);
corpus_test!(
    c108_empty_link_and_image_titles_are_preserved,
    "empty-link-and-image-titles-are-preserved"
);
corpus_test!(
    c109_cross_references_resolve_inside_footnote_bodies,
    "cross-references-resolve-inside-footnote-bodies"
);
corpus_test!(
    c110_unquoted_attribute_values_may_contain_dots_and_colons,
    "unquoted-attribute-values-may-contain-dots-and-colons"
);
corpus_test!(
    c111_a_pipe_pair_with_no_cell_is_not_a_table,
    "a-pipe-pair-with-no-cell-is-not-a-table"
);
corpus_test!(
    c112_adjacent_attribute_blocks_on_one_line_merge,
    "adjacent-attribute-blocks-on-one-line-merge"
);
corpus_test!(
    c113_a_continuation_row_needs_a_body_row,
    "a-continuation-row-needs-a-body-row"
);
corpus_test!(
    c114_fence_opener_with_a_nested_list_body_inside_a_list_item,
    "fence-opener-with-a-nested-list-body-inside-a-list-item"
);
corpus_test!(
    c115_footnote_definition_inside_a_container_is_collected,
    "footnote-definition-inside-a-container-is-collected"
);
corpus_test!(
    c116_cyclic_cross_reference_resolves_to_one_level,
    "cyclic-cross-reference-resolves-to-one-level"
);
corpus_test!(
    c117_trojan_source_heading_ids_are_nfc_normalized_and_strip_invisible_controls,
    "trojan-source-heading-ids-are-nfc-normalized-and-strip-invisible-controls"
);
corpus_test!(
    c118_trojan_source_rendered_text_and_code_strip_bidi_override_controls,
    "trojan-source-rendered-text-and-code-strip-bidi-override-controls"
);
corpus_test!(
    c119_scheme_probe_strips_unicode_whitespace,
    "scheme-probe-strips-unicode-whitespace"
);
corpus_test!(c120_footnotes_placement, "footnotes-placement");
corpus_test!(c121_classes_are_deduplicated, "classes-are-deduplicated");
corpus_test!(
    c122_code_span_and_image_trailing_attributes_are_strict,
    "code-span-and-image-trailing-attributes-are-strict"
);
corpus_test!(
    c123_a_bare_attribute_block_on_its_own_line_is_literal,
    "a-bare-attribute-block-on-its-own-line-is-literal"
);
corpus_test!(
    c124_a_backslash_in_a_link_destination_is_a_literal_character,
    "a-backslash-in-a-link-destination-is-a-literal-character"
);
corpus_test!(
    c125_autolink_display_keeps_the_raw_content,
    "autolink-display-keeps-the-raw-content"
);
corpus_test!(
    c126_editorial_markup_takes_a_trailing_attribute,
    "editorial-markup-takes-a-trailing-attribute"
);
corpus_test!(
    c127_emphasis_opener_slash_adjacency,
    "emphasis-opener-slash-adjacency"
);
corpus_test!(
    c128_bold_italic_delimiter_needs_content,
    "bold-italic-delimiter-needs-content"
);
corpus_test!(
    c129_emphasis_span_closes_before_a_following_delimiter,
    "emphasis-span-closes-before-a-following-delimiter"
);
corpus_test!(
    c130_thematic_break_requires_contiguous_markers,
    "thematic-break-requires-contiguous-markers"
);
corpus_test!(
    c131_sublist_marker_interrupts_a_continuation_paragraph,
    "sublist-marker-interrupts-a-continuation-paragraph"
);
corpus_test!(
    c132_footnote_definition_requires_an_inline_body,
    "footnote-definition-requires-an-inline-body"
);
corpus_test!(
    c133_footnote_definition_separator_must_be_a_space,
    "footnote-definition-separator-must-be-a-space"
);
corpus_test!(
    c134_link_reference_definition_separator_must_be_a_space,
    "link-reference-definition-separator-must-be-a-space"
);
corpus_test!(
    c135_abbreviation_definition_separator_must_be_a_space,
    "abbreviation-definition-separator-must-be-a-space"
);
corpus_test!(
    c_opaque_spans_inside_a_container,
    "opaque-spans-inside-a-container"
);
corpus_test!(
    c_blocks_that_render_to_nothing,
    "blocks-that-render-to-nothing"
);
corpus_test!(c_bare_dot_ordered_markers, "bare-dot-ordered-markers");
corpus_test!(
    c_openers_past_the_nesting_cap,
    "openers-past-the-nesting-cap-are-one-paragraph"
);
corpus_test!(
    c_flush_left_line_needs_an_open_paragraph,
    "a-flush-left-line-needs-an-open-paragraph-to-fold-into"
);
corpus_test!(
    c_comment_is_recognized_at_any_column,
    "a-comment-is-recognized-at-any-column"
);
corpus_test!(
    c_line_endings_and_a_byte_order_mark,
    "line-endings-and-a-byte-order-mark"
);
corpus_test!(
    c_continuation_marker_after_a_blank_line_in_a_loose_item,
    "a-continuation-marker-after-a-blank-line-in-a-loose-item"
);
corpus_test!(
    c_tab_separates_two_attributes,
    "a-tab-separates-two-attributes-and-pads-a-block-as-a-space-does"
);
corpus_test!(
    c_inline_attribute_block_does_not_span_lines,
    "an-inline-attribute-block-does-not-span-lines-but-an-attribute-line-does"
);
corpus_test!(
    c_colon_fence_separator_must_be_a_space,
    "colon-fence-separator-must-be-a-space"
);
corpus_test!(
    c_colon_fence_metadata_slots_must_be_a_space_too,
    "colon-fence-metadata-slots-must-be-a-space-too"
);
corpus_test!(
    c_fence_opened_on_a_list_marker_line,
    "a-fence-opened-on-a-list-marker-line-body-below-the-content-column"
);
