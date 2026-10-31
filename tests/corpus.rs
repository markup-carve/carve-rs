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
    // Spec 14e66c05 adds 39 corpus pairs in these categories. Each new pair
    // was checked against its committed HTML before the spec pin moved.
    "a-contentless-description-marker-folds-under-a-quoted-term",
    "a-refused-first-block-plus-folds-its-content-column-line-and-the-next-root-block-ends-the-item",
    "a-lone-brace-stays-inside-forced-emphasis",
    "a-nested-item-whose-plus-is-refused-releases-the-next-root-block",
    "a-quoted-include-with-no-resolver-parses-inline-content",
    "a-second-plus-starts-its-own-paragraph-after-attachment",
    "an-enclosing-closer-ends-a-tag-name",
    "an-overindented-colon-opener-ends-the-quote-paragraph",
    "an-unclosed-nested-fence-keeps-blanks-before-outer-text",
    "citation-shaped-lines-remain-core-text",
    "combined-emphasis-finds-a-later-nonempty-closer",
    "compact-single-paragraph-quote",
    "consecutive-continuation-markers-in-a-div-hosted-quote",
    "invalid-include-line-ranges-remain-literal-in-core",
    "invalid-line-range-includes-within-paragraphs-stay-literal",
    "loose-list-starts-directly-on-its-first-paragraph",
    "multiple-footnote-backlinks-follow-inline-content",
    "paired-german-quotes-leave-no-quote-in-a-heading-id",
    "raw-inline-image-bytes-stay-inside-a-paragraph",
    "unresolved-footnote-literal-escapes-html-text",
    "url-sinks-reject-schemes-after-emitted-value-normalization",
    "a-fence-with-no-host-prefix-leaves-an-inline-run",
    "a-line-at-the-marker-column-leaves-a-closed-nested-body",
    "bidi-controls-in-math-content",
    "empty-heading-slugs-use-the-ordinary-duplicate-suffix",
    "escaped-space-in-inline-text",
    "lazy-text-stays-in-an-unclosed-quoted-admonition",
    "link-titles-accept-trailing-ascii-padding-with-either-destination",
    "table-content-parses-inline-markup",
    "a-display-math-span-with-trailing-attributes-takes-a-caption",
    "a-styled-display-math-span-takes-a-caption-across-one-blank-line",
    "a-tab-after-a-comment-marker-stays-in-its-text",
    "a-tab-cannot-pad-an-empty-link-destination",
    "a-titled-admonition-skips-an-authored-heading-id",
    "an-empty-link-destination-takes-spaces",
    "a-caption-after-a-continuation-marker-fence-stays-with-the-fence",
    "a-column-zero-marker-below-an-open-div-attaches-beside-the-div",
    "a-continuation-marker-stands-alone-at-an-open-marker-column-or-right-after-a-block-opener",
    "a-continuation-marker-uses-its-container-column",
    "a-first-block-marker-on-an-indented-opener-reads-only-its-marker-column",
    "a-first-block-marker-takes-a-list-at-a-note-s-or-description-s-marker-column",
    "a-refused-continuation-plus-remains-text",
    "a-refused-plus-without-the-inner-quote-prefix-renders-as-text",
    "a-task-item-takes-a-first-block-continuation-marker-after-its-checkbox",
    "a-tab-indented-nested-list-starts-at-its-marker",
    "a-dangling-plus-is-a-letter-and-a-nested-quote-reads-it-like-an-unnested-one",
    "a-fence-closer-outside-the-child-folds-and-leaves-the-child-open",
    "a-fence-run-folded-into-a-child-paragraph-loosens-the-outer-item",
    "a-fence-run-folded-into-a-child-paragraph-loosens-the-outer-item-when-it-closes-at-the-outer-column",
    "a-fence-run-that-dedents-out-of-a-child-keeps-the-outer-item-tight",
    "a-folded-fence-run-leaves-a-later-paragraph-loose",
    "a-folded-child-fence-keeps-the-below-column-line-in-its-paragraph",
    "a-lazily-folded-fence-opener-line-does-not-keep-a-list-tight",
    "a-lazy-colon-shaped-line-in-a-quote-paragraph-keeps-no-indentation",
    "an-unclosed-fence-run-with-no-closer-in-its-item-does-not-hide-a-blank-line",
    "a-refused-marker-leaves-an-open-fence-s-blank-payload",
    "an-indented-plus-below-a-nested-fence-stays-content-at-every-depth",
    "generated-container-documents",
    "definition-body-note-and-fence-owner-boundaries",
    "nested-plus-below-fence-and-description",
    "a-block-at-an-item-content-column-ends-its-paragraph-before-an-unclosed-fence",
    "a-definition-or-attribute-line-in-a-nested-container-leaves-no-paragraph-for-an-unmarked-line",
    "a-line-is-read-in-the-container-whose-column-it-reaches",
    "a-line-past-a-body-s-column-is-read-in-the-body-and-a-quote-does-not-rebase",
    "a-quoted-item-comment-ends-the-quote-before-an-unclosed-fence",
    "a-table-row-or-a-definition-under-a-paragraph-ends-it-before-an-unclosed-fence",
    "an-escaped-cross-reference-spelling-stays-literal",
    "an-item-comment-or-attribute-line-closes-the-item-s-paragraph",
    "an-unclosed-fence-under-a-multi-line-quote-paragraph-in-a-container-opens-nothing",
    "an-autolink-a-cross-reference-or-a-comment-is-atomic-to-every-outside-delimiter",
    "an-unclosed-fence-under-an-open-paragraph-in-a-container-opens-nothing",
    "a-delimiter-that-is-part-of-a-larger-character-is-text",
    "the-combined-bold-italic-token-does-not-open-or-close-inside-a-word",
    "inside-an-unclosed-fence-only-the-innermost-container-s-closer-is-structural",
    "a-continuation-on-a-span-marker-joins-the-cell-the-marker-resolves-into",
    "a-table-row-ends-a-description-body-and-a-lone-plus-folds-into-a-term",
    "a-below-column-line-continues-a-paragraph-only-where-one-is-open",
    "a-closed-fence-in-a-nested-child-leaves-no-paragraph-open",
    "a-code-line-left-of-an-authored-base-fence-opener-is-measured-from-that-base",
    "a-fence-closer-below-the-authored-base-is-measured-as-its-payload",
    "a-caption-scope-resets-at-a-structural-only-line",
    "a-dash-run-opens-frontmatter-only-at-the-start-and-only-a-dash-run",
    "an-ordered-list-carries-its-authored-delimiter",
    "multiple-table-bodies-have-positional-source-metadata",
    "empty-table-bodies-keep-their-source-boundaries",
    "a-table-with-no-bodies-keeps-its-head-and-foot",
    "invalid-table-body-metadata-stays-ordinary",
    "a-span-across-bodies-keeps-their-header-semantics",
    "a-head-and-foot-consuming-all-rows-leave-no-implicit-body",
    "explicit-body-counts-include-native-header-cells",

    "invalid-named-container-metadata-keeps-the-subtree",
    "a-comment-span-opened-below-every-content-column-is-located-there",
    "a-comment-span-s-closer-column-does-not-move-the-item-s-ownership",
    "a-bare-colon-opener-in-a-description-body-is-an-opener",
    "a-bare-colon-run-interrupts-a-paragraph-whether-or-not-a-line-follows-it",
    "a-caption-s-placeholder-is-any-that-does-not-begin-a-tag",
    "a-closer-below-the-container-s-column-does-not-count",
    "a-closer-does-not-rescue-a-marker-line-colon-opener-whose-body-folded-in",
    "a-code-span-closes-only-on-a-run-of-its-own-length-whatever-the-length",
    "a-comment-inside-a-forced-span-or-the-combined-token-ends-at-its-closer",
    "a-definition-body-s-open-code-fence-ends-at-a-line-below-its-column",
    "a-delimiter-after-an-underscore-or-slash-opens-only-when-that-one-pairs",
    "a-form-feed-or-a-no-break-space-is-content-wherever-whitespace-is-tested",
    "a-quote-after-a-bare-delimiter-follows-what-that-delimiter-does",
    "a-quote-after-an-escaped-quote-closes",
    "an-empty-term-marker-in-a-description-body-is-text",
    "an-item-s-fence-is-read-once-whatever-block-it-follows",
    "an-unresolved-reference-s-literal-source-is-html-escaped-like-any-other-text",
    "any-character-is-content-of-the-combined-bold-italic-token",
    "a-floating-attribute-does-not-widen-a-list-item-s-content-column",
    "an-unclosed-bare-colon-fence-opens-a-div",
    "an-item-s-attribute-block-moves-its-content-column-its-checkbox-does-not",
    "a-boolean-attribute-does-not-start-with-an-underscore",
    "a-braced-hyphen-pair-is-an-en-dash",
    "a-hyphen-run-opening-a-word-after-whitespace-is-a-flag",
    "a-lazy-marker-line-s-definition-defines-nothing-in-any-container",
    "a-marker-line-link-definition-is-collected-where-no-paragraph-is-open",
    "a-reference-definition-cannot-take-its-destination-from-the-next-line",
    "a-resumed-lazy-run-belongs-to-the-innermost-marker-line-item",
    "a-terminal-comment-in-a-quote-leaves-no-paragraph-open",
    "a-terminal-comment-line-still-leaves-an-empty-verse-line",
    "an-empty-brace-pair-is-not-a-construct",
    "adjacent-strong-spans-use-html-only-where-their-delimiters-merge",
    "empty-containers-share-one-html-body-shape",
    "an-unclosed-inline-literal-reaches-the-end-of-its-block",
    "pipe-tables-can-state-head-and-foot-row-counts",
    "the-doubled-run-is-the-canonical-arrow-in-both-families",
    "a-quote-is-reached-by-its-marker-and-a-column-never-reaches-into-one",
    "table-columns-carry-alignment-vertical-alignment-and-widths",
    "a-table-alignment-run-carries-two-independent-axes",
    "a-vertical-table-marker-needs-a-horizontal-partner",
    "a-table-cell-can-inherit-horizontal-alignment",
    "a-collected-definition-closes-the-item-paragraph",
    "an-all-blank-raw-payload-still-emits-its-line",
    // Added with the spec bump to carve 68b71c2. Every document in each of the
    // four was rendered through this engine and matched its committed HTML
    // before being listed here.
    "a-marker-glued-to-a-name-opens-nothing",
    "a-math-span-s-base-class-keeps-the-class-slot-in-place",
    "an-abbreviation-expands-inside-an-inline-container",
    "an-angle-bracket-is-escaped-only-where-it-opens-markup",
    "two-attributes-need-a-separator-between-them",
    "the-semantic-registry-holds-no-element-carve-already-spells",
    "a-language-attribute-is-exact-sugar-for-lang",
    "a-malformed-language-tag-leaves-the-whole-block-literal",
    "a-language-attribute-and-lang-are-one-key",
    "the-language-sigil-takes-no-padding",
    "a-boolean-lang-is-the-third-spelling-of-the-same-key",
    "a-semantic-name-renames-the-span-and-the-leftovers-ride-the-element",
    "a-derived-title-yields-to-an-authored-one",
    "adjacent-block-openers-in-an-attached-run-stay-separate",
    "a-caption-attaches-across-one-blank-line",
    "captions-in-list-items-do-not-create-a-paragraph-gap",
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
    "a-fenced-block-quote-is-the-block-quote-written-another-way",
    "an-authored-base-carries-opaque-payload-captions-and-nested-metadata",
    "an-invisible-fenced-block-is-not-a-list-paragraph",
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
    "a-boolean-and-a-key-value-of-the-same-name-are-one-attribute",
    "two-attributes-need-a-separator-between-them",
    "table-span-marker-in-first-column",
    "table-cell-attributes",
    "table-row-attributes",
    "table-header-cell-rowspan",
    "block-quote-continuation-marker",
    "a-heading-marker-opens-at-an-indented-top-level-column",
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
    "an-over-indented-definition-attaches-an-under-indented-one-folds",
    "image-trailing-attribute-is-strict-about-the-glue",
    "wrapped-definition-term-continuation-below-the-content-column-strips-leading-whitespace",
    "an-indented-attribute-line-attaches-at-top-level",
    "an-indented-image-and-caption-form-a-figure",
    "indented-reference-and-footnote-definitions-register",
    "indented-colon-fence-blocks-open",
    "below-content-column-div-body-in-a-list-item-stays-literal",
    "outer-item-with-an-internal-blank-before-an-attached-block-is-loose",
    "unresolved-footnote-reference-with-a-trailing-attribute-stays-literal",
    "tight-list-item-keeps-trailing-text-after-a-block-bare",
    "quote-flanking-after-an-escaped-character",
    "comment-columns-and-surviving-list-items",
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
    "a-link-destination-is-opaque-to-the-bracket-scan",
    "a-braced-comment-does-not-decide-a-quote",
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
    "a-definition-below-every-content-column-ends-the-item-and-registers",
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
    "a-definition-past-a-footnote-body-s-column-registers-from-its-authored-base",
    "a-definition-body-opener-at-or-past-its-column-stays-structural",
    "a-definition-body-s-separator-width-sets-its-content-column",
    "a-definition-list-inside-a-footnote-body-carries-its-authored-base",
    "a-footnote-body-s-authored-base-can-open-a-table-past-column-two",
    "a-recognized-opener-in-a-body-needs-no-blank-line-above-it",
    "a-sigil-fence-takes-its-attribute-line",
    "an-ordered-item-s-separator-width-sets-its-content-column",
    "below-a-definition-body-s-column-the-body-ends",
    "one-authored-base-rule-reaches-a-definition-nested-in-a-list-item",
    "text-block-alignment-renders-the-css-declaration",
    "the-continuation-marker-attaches-one-block-in-every-container",
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
    "a-tab-continues-a-list-item-just-as-two-spaces-do",
    "an-absorbed-colon-fence-leaves-a-block-quote-s-paragraph-open",
    "code-fence-metadata-slots-must-be-a-space-too",
    "link-and-image-title-slots-must-be-a-space",
    "table-cell-padding-must-be-a-space",
    "the-flush-left-line-after-a-container-a-quoted-line-opened",
    "a-below-column-marker-after-a-comment-where-no-paragraph-is-open",
    "a-below-column-line-continues-a-paragraph-only-where-one-is-open",
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
    "a-structural-attribute-leads-the-author-s-own",
    "adjacent-sibling-lists-survive-the-round-trip",
    "a-fence-keeps-the-blank-line-at-the-end-of-its-content",
    // Added with the spec bump to carve b48e905, the merge of PART 9R R2, and
    // the categories that landed between the previous pin and it. Every
    // document in each was rendered through this engine and diffed against its
    // committed HTML before being listed here. Two needed an engine change:
    // `a-footnote-in-an-unresolved-reference-is-not-a-reference` (three of four
    // documents) and `a-reference-link-s-text-survives-its-own-frame` (two of
    // four). The other seven already matched and only the pin was behind.
    "a-captioned-quote-holds-more-than-one-block",
    "a-footnote-in-an-unresolved-reference-is-not-a-reference",
    "a-footnote-in-link-text-nests-the-anchors",
    "a-footnote-in-reference-link-text-nests-the-anchors-too",
    "a-multi-letter-ordered-marker-opens-no-list",
    "a-note-body-s-own-references-resolve",
    "a-note-s-content-recognizes-no-note",
    "a-reference-link-s-text-survives-its-own-frame",
    "an-empty-inline-note-is-literal",
    // markup-carve/carve#1203, #1206 and the pin move behind them, nineteen
    // documents in three categories. Every one was rendered through this engine
    // and diffed against its committed HTML - and against carve-js f05f3a7 on
    // all five targets - before being listed here.
    //
    // Three needed an engine change, all in
    // `an-inline-note-s-content-resolves-after-the-note` (documents 3, 4 and 5):
    // a reference link, an image reference and a collapsed reference inside a
    // note's content did not resolve. The `carve` and `markdown` rows of
    // `an-image-s-alt-text-closes-where-a-link-s-text-closes` and
    // `an-editorial-comment-s-bracket-is-content-not-the-close` were the writer
    // half, fixed one branch earlier; their HTML already matched at the old pin.
    "an-editorial-comment-s-bracket-is-content-not-the-close",
    "an-image-s-alt-text-closes-where-a-link-s-text-closes",
    "an-inline-note-s-content-resolves-after-the-note",
    "composite-figures",
    // Added with the spec bump to carve 3ac58ce (markup-carve/carve#1226). Two
    // of the six documents already matched at the previous pin -- both are the
    // controls, the shapes the reordered slot must NOT change. The other four
    // carry a block after a marker run and needed the engine change.
    "cell-attributes-bind-after-the-kind-and-alignment-markers",
    "a-table-cell-s-marker-run-ends-at-a-space",
    // Added with the spec bump to carve 395c083 (markup-carve/carve#1229). Both
    // documents already matched, HTML and `.fmt` sidecar alike: the writer
    // stopped emitting the spaced form one release earlier
    // (markup-carve/carve-rs#987). What was missing was the corpus pin that says
    // so, which is what makes a regression here visible.
    "the-canonical-writer-glues-a-code-fence-to-its-info-string",
    "a-block-attached-after-an-invisible-line-leaves-the-item-tight",
    "an-abbreviation-definition-in-an-item-body-is-paragraph-text",
    "an-attribute-block-reaches-the-nested-list-it-precedes",
    "delimited-comments",
    "a-column-0-line-after-a-container-s-last-block-when-that-block-left-no-paragraph-open",
    "a-continuation-marker-attaches-one-block-and-the-boundary-is-that-block-s-extent",
    "a-continuation-row-s-open-run-and-an-escaped-closing-pipe",
    "a-label-beginning-with-an-at-sign-is-not-a-reference-label",
    "an-attribute-line-after-a-continuation-marker-attributes-the-attached-block",
    "an-unclosed-inline-run-in-a-line-block-reaches-the-end-of-the-block",
    "an-unclosed-verbatim-run-in-a-row-stops-at-the-closing-pipe",
    "which-inline-content-a-heading-id-is-derived-from",
    "a-floating-attribute-is-scoped-to-the-container-that-holds-it",
    "a-tab-after-a-fence-or-a-frontmatter-opener-depends-on-where-it-sits",
    "a-comment-fence-at-an-item-s-content-column-registers-nothing-either",
    "a-comment-fence-inside-a-colon-container-registers-nothing",
    "a-comment-fence-one-item-deeper-registers-nothing-either",
    "a-comment-fence-opened-on-an-item-s-marker-line-hides-its-body-too",
    "a-footnote-definition-inside-an-item-s-comment-registers-nothing",
    "a-wider-comment-fence-inside-an-item-hides-its-body-the-same-way",
    "an-abbreviation-inside-a-comment-defines-nothing",
    "url-list-attributes-are-probed-token-wise",
    "an-escaped-hash-keeps-its-escape-at-a-container-s-content-position",
    // Added with the spec bump to carve 9015c3b (markup-carve/carve#1333, #1334
    // and #1340), eleven documents in three categories. Every HTML but one
    // already matched at the previous pin - the exception is the document this
    // engine published a comment on, which needed the parser change. The `.fmt`
    // sidecars are the half that moved: `corpus_canonical_form` reads nine of
    // them and five were wrong, four of those under the amended §7c, which is
    // now the PROPERTY a bare newline has to satisfy rather than a list of the
    // places it does not.
    "a-comment-only-line-in-a-line-block-is-removed-before-any-inline-run",
    "a-line-block-s-hard-break-keeps-its-backslash",
    "a-line-block-s-last-body-line-keeps-its-backslash",
    "a-comment-fence-reached-through-a-quote-registers-nothing-either",
    "a-closed-inline-construct-spanning-a-verse-boundary",
    "a-bracketed-construct-s-identifiers-stay-on-one-line",
    "a-bracketed-construct-spanning-a-line-boundary",
    "a-bracketed-construct-spanning-a-verse-boundary",
    "a-container-whose-table-ends-on-a-continuation-row",
    "a-definition-at-a-container-s-content-column",
    "a-block-at-a-container-s-content-column-ends-the-paragraph-whatever-it-renders",
    "a-container-whose-table-ends-on-a-joined-header-row",
    "a-continuation-row-joins-the-row-above-it-whatever-its-cells-hold",
    "a-quote-inside-a-quote-is-asked-what-it-ends-on",
    "what-a-content-column-block-does-not-reach",
    "a-footnote-definition-s-block-runs-to-the-end-of-its-body",
    "a-definition-behind-an-alternating-container-prefix-registers-at-the-innermost-content-column",
    "a-paragraph-opened-after-a-block-in-an-item-is-still-open-for-a-lazy-line",
    "an-unterminated-container-does-not-extend-the-item-past-a-blank-line",
    "a-blank-line-before-a-sibling-marker-separates-the-items-whatever-consumed-it",
    "a-task-item-s-checkbox-is-not-decided-by-its-first-block",
    "an-attribute-line-below-a-list-item-interrupts-it",
    "an-attributed-cell-keeps-its-attributes-and-its-literal-marker",
    "an-engine-written-shape-says-what-it-is-called",
    "only-lazy-folding-demotes-a-marker-line-colon-opener",
    // markup-carve/carve#1377: a content-column heading leaves no paragraph
    // open for the following flush-left line.
    "a-heading-at-an-item-s-content-column-leaves-no-paragraph-open",
    // The parser already follows #1799's matching rule for wrapped attribute
    // lines; the spec bump adds the three byte-identical corpus assertions.
    "a-wrapped-attribute-line-leaves-no-paragraph-open",
    // markup-carve/carve#1526 and #1542: a container's span - and a definition
    // list's - ends at its last PLACED child, not at the line the scan reached.
    "a-container-s-span-ends-at-its-last-placed-child",
    "a-definition-list-ends-at-its-last-placed-child-too",
    // markup-carve/carve#1516: an escape escalation reaches the block that
    // needed one, never the whole document.
    "a-leading-escaped-caret-keeps-its-escape",
    "an-idle-escape-does-not-spread-from-the-block-that-needed-one",
    // markup-carve/carve#1513: a hard list boundary is written as exactly
    // three blank lines.
    "a-longer-run-at-a-list-boundary-is-written-as-exactly-three-blank-lines",
    // markup-carve/carve#1525: a null byte is replaced before the document is
    // read.
    "a-null-byte-is-replaced-before-the-document-is-read",
    // Corpus 400 and 401, which the pin bump for markup-carve/carve#1554
    // brings with it. No engine work in either: #1252 landed the container's
    // opening markup here and #1251 the marker at an item's content column, and
    // both categories reproduce on this build. Named rather than left out,
    // because a category absent from this list is not compared at all.
    "a-container-starts-at-its-opening-markup-even-where-its-first-child-is-unplaced",
    "a-marker-at-an-item-content-column-opens-a-sublist-first-in-the-item-or-not",
    // Corpus 402-406, which this pin bump brings with it. Every document in
    // each category was rendered through this engine and matched its committed
    // HTML byte for byte before being listed here; none needed engine work.
    // markup-carve/carve#1564: a container ends at the markup that closes it,
    // even where its last child is unplaced.
    "a-container-ends-at-the-markup-that-closes-it-even-where-its-last-child-is-unplaced",
    // markup-carve/carve#1570: the writer escapes per opener OCCURRENCE, not
    // per unit. Narrows category 396, which stays listed above and keeps its
    // own pinned canonical form.
    "an-idle-escape-does-not-spread-from-the-occurrence-that-needed-one",
    // markup-carve/carve#1583: a caption's marker separator is a run of
    // SPACES, so a tab after it is content and a marker with only a separator
    // captions nothing; a quoted figure indents like any other nested block.
    "a-caption-s-marker-separator-is-a-run-and-none-of-it-is-content",
    "a-quote-holding-a-captioned-block-indents-it-like-any-other-nested-block",
    // markup-carve/carve#1587: the same rule for a heading's marker separator.
    "a-heading-s-marker-separator-is-a-run-and-none-of-it-is-content",
    // Measured stale while adding the five above: each of these three renders
    // every one of its documents byte-identically at this pin AND at the pin
    // this bump replaced, so none of them was ever a gap this submodule move
    // created. They sat in KNOWN_GAPS because nothing asserted that a declared
    // gap still fails; `a_known_gap_still_fails` below now does.
    "a-continuation-marker-attaches-a-block-only-at-its-owner-s-marker-column",
    "a-raw-block-keeps-the-blank-line-at-the-end-of-its-payload-too",
    "a-raw-block-passes-its-attributes-through-untouched",
    "an-unterminated-fence-at-a-content-column-opens-no-block-so-the-paragraph-stays-open",
    // Corpus 405-408, which this pin bump brings with it.
    // markup-carve/carve#1633: a blank line loosens an item only where a
    // paragraph follows it, and a footnote continuation survives a blank run.
    "a-blank-line-loosens-an-item-only-when-a-paragraph-follows-it",
    "a-footnote-continuation-survives-a-blank-run",
    // markup-carve/carve#1623 and markup-carve/carve#1639: PART 9 SS17 L7's
    // consumed `loose` boolean, and the writer rule that spells it only where a
    // blank line cannot.
    "one-consumed-boolean-spells-the-looseness-no-blank-line-can",
    "the-writer-spells-looseness-only-where-a-blank-line-cannot",
    // Corpus 411-412. markup-carve/carve#1662 and markup-carve/carve#1666: a
    // block image opens only at its container's content column, so one column
    // further the same bytes are a paragraph holding an inline image - and the
    // reference spellings of a lone image at column 0 are the same block.
    // 411 ships `.fmt` sidecars (markup-carve/carve#1673) because its canonical
    // form differs from its source: the writer declines the indented spelling.
    "a-lone-indented-image-is-a-paragraph-and-its-html-cannot-say-so",
    "a-lone-reference-image-at-column-0-in-every-spelling",
    "a-comment-in-a-footnote-body-is-invisible-in-both-spellings",
    "a-floating-attribute-under-a-definition-attaches-at-column-zero",
    "an-abbreviation-definition-outside-document-level-is-not-an-invisible-line",
    "an-invisible-line-before-the-blank-does-not-cancel-the-separation",
    "an-unresolved-image-gives-its-whole-caption-slot-back-at-any-depth",
    "below-a-definition-body-s-column-an-invisible-line-folds-as-text",
    "a-colon-followed-by-only-whitespace-is-not-a-description",
    "a-first-block-marker-in-a-footnote-body-or-a-quote-attaches-only-at-its-marker-column",
    "a-block-quote-and-a-footnote-body-take-the-first-block-form",
    "a-refused-description-plus-folds-a-line-below-column-0-and-a-comment-does-not",
    "an-empty-description-body-is-written-with-the-empty-sentinel",
    "the-continuation-marker-s-column-gate-reaches-every-container",
    "a-definition-between-two-open-content-columns-reaches-the-outer-one",
    "a-marker-folds-only-strictly-between-the-item-s-base-and-content-column",
    "an-unterminated-comment-fence-in-a-list-item-is-the-line-form",
    // Added with the spec bump to carve 1b27b68. Every document in each was
    // rendered through this engine and matched its committed HTML before being
    // listed here; 451 registers a link definition only when its list-item host
    // owns it, and a definition past a colon fence in a list item is the
    // container's text (carve-rs colon-fence ownership fix).
    "a-block-opener-past-a-nested-footnote-definition-opens-in-the-item",
    "a-closed-fence-in-a-description-body-ends-it",
    "a-comment-below-a-description-body-s-column-ends-the-body",
    "a-container-closer-closes-its-container-in-a-footnote-body-too",
    "a-container-in-a-host-body-owns-a-line-past-its-own-content-column",
    "a-definition-nested-past-a-footnote-body-is-a-note-and-a-reference-below-it-resolves",
    "a-degraded-comment-fence-at-a-container-s-column-0-keeps-the-follower-in-the-item",
    "a-degraded-comment-fence-leaves-a-lazy-follower-where-the-line-form-does",
    "a-marker-folds-into-a-quote-below-it",
    "a-row-whose-every-cell-is-blank-is-not-a-table",
    "a-wrapped-attribute-block-ends-at-its-quote-and-reaches-no-line-below-it",
    "an-empty-unterminated-container-ends-at-a-flush-left-line",
    "an-opener-at-or-past-a-description-body-s-column-closes-its-paragraph",
    "an-unterminated-fence-on-a-nested-lead-in-a-description-body-owns-its-body",
    "the-host-does-not-change-which-column-a-definition-reaches",
    "a-column-0-line-after-a-description-hosted-note-is-a-document-sibling",
    "a-nested-note-s-floor-is-two-columns-past-its-own-marker",
    "a-trailing-line-after-a-consumed-definition-is-placed-by-column-reach",
    "include-directive-with-no-resolver-renders-literal",
    // Added with the spec bump to carve 9c84524. Every document in each was
    // rendered through this engine and matched its committed HTML before being
    // listed here. Three of the six are behavior this engine gained since the
    // last bump: the bare closer stops at a braced inline and at a link
    // destination (carve-rs#1638 and carve-rs#1653, PART 3 E2a), and an
    // underscore pair in text is escaped where the emitted line would pair it
    // (carve-rs#1659, PART 11 section 8a M1b).
    "a-bare-closer-does-not-reach-inside-a-braced-inline",
    "a-bare-closer-does-not-reach-inside-a-cross-reference",
    "an-autolink-or-a-cross-reference-is-atomic-to-brackets-and-braces",
    "a-bare-closer-does-not-reach-inside-a-link-destination",
    "a-block-that-opens-a-tight-item-is-written-on-the-marker-line",
    "an-underscore-pair-in-text-is-escaped-where-the-line-would-pair-it",
    "an-underscore-pair-split-across-a-line-break-is-escaped",
    "the-round-trip-comparison-normalizes-a-named-list",
    "a-quote-is-an-ordinary-link-destination-character",
    "an-empty-link-destination-is-not-a-link",
    "braced-emphasis-can-nest-its-own-kind",
    "substitution-content-is-inline-and-only-a-top-level-arrow-splits-it",
    "a-run-of-asterisks-inside-a-combined-token-is-content",
    "glued-attribute-blocks-on-an-inline-element-merge",
    "footnote-references-take-an-attribute-run-editorial-substitution-and-comment-take-none",
    // The two pairs markup-carve/carve#2224 added, arriving with the pin bump to
    // carve d85b13ae. Listed because the render was COMPARED, not because the
    // gate was in the way: carve-rs#1854 implemented that rule ahead of the pin,
    // and its `explicit_head_rowspan_keeps_one_group` and
    // `footer_rowspan_keeps_one_group` assert the same source and the same HTML
    // as these two documents, byte for byte. Those tests are green on main, so
    // this engine already produces what the corpus expects.
    "an-explicit-table-head-span-keeps-one-row-group",
    "a-table-foot-span-keeps-one-row-group",
    // markup-carve/carve#2286, arriving with this pin bump: a `::: footnotes`
    // marker inside a container does not place (CARVE-P9-073). carve-rs#1894
    // is the change that makes this row match.
    "a-footnotes-placement-marker-inside-a-container-does-not-place",
    // markup-carve/carve#2272's row, which carve-rs#1915 already answered - its
    // `a_profile_keeps_a_bodyless_named_container` pins the same four shapes.
    "a-title-or-label-fills-the-container-body-slot",
    // markup-carve/carve#2319's row, arriving with this pin bump. carve-rs#1955
    // answered #1951 and carve-rs#1956 pins the separator width.
    "a-comment-line-s-text-is-content-and-a-block-body-is-payload",
    // Added with the spec bump to carve 66661ed. Every document in each of
    // the three was rendered through this engine and diffed against its
    // committed HTML before being listed here.
    "a-footnotes-marker-renders-its-authored-blocks-before-the-placed-section",
    "an-unplaced-footnotes-marker-keeps-its-authored-blocks-inside-the-div",
    "a-core-directive-kind-class-leads-authored-attributes",
    // Added with the spec bump to carve 63c8631. All three documents of
    // markup-carve/carve#2406 were rendered through this engine and diffed
    // against their committed HTML before the category was listed here.
    "an-attribute-line-under-an-attributed-sub-item-stays-in-that-item",
    "an-unclosed-fence-after-an-attribute-line-in-a-container-ends-at-its-closer",
    "a-quoted-comment-or-attribute-line-closes-the-quote-s-paragraph",
    "a-block-opener-indented-under-a-definition-term-ends-the-term-at-every-depth",
    // Added with the spec bump to carve e00d86a9. All twenty-five documents of
    // markup-carve/carve#2458 were rendered through this engine and diffed
    // against their committed HTML before the category was listed here.
    "a-comment-or-a-definition-under-a-definition-term-folds-at-every-depth",
    // Added with the spec bump to carve 5315967c. All four documents of
    // markup-carve/carve#2464 were rendered through this engine and diffed
    // against their committed HTML before the category was listed here;
    // carve-rs#2062 already measures the residue from the fence opener.
    "a-verbatim-line-keeps-what-sits-past-its-fence-opener-not-past-its-container",
    // Added with the spec bump to carve 71b51d00. Forty-six documents across
    // five categories, and the engine work for all five had already landed as
    // its own pull request ahead of the pin - carve-rs#2118, #2121, #2122,
    // #2123 and #2124 - so the red rows on the bump draft were this guard
    // asking to be told rather than a divergence. Every document is checked
    // byte for byte against its committed HTML by
    // `all_implemented_corpus_pairs_match` the moment the category is listed
    // here, which is what makes each line a claim and not a blessing. None is
    // deferred, so KNOWN_GAPS stays empty.
    //
    // 507 is markup-carve/carve#2494: a list marker below a raised colon
    // container folds into that container's open paragraph. Sixteen documents.
    "a-list-marker-in-a-raised-colon-container-folds-into-its-open-paragraph",
    // 508 is markup-carve/carve#2503: a comment span's closer below its host's
    // column is still that span's delimiter. Six documents.
    "a-comment-span-s-closer-below-its-host-s-column-stays-a-delimiter",
    // 509 is markup-carve/carve#2509: a fence closer below a nested item's
    // column ends containers down to the one that owns the fence. Twelve
    // documents.
    "a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner",
    // 510 is markup-carve/carve#2510: a nested quoted term leaves no paragraph
    // for a lazy line to continue. One document.
    "a-nested-quoted-term-leaves-no-paragraph-for-a-lazy-line",
    // 511 is markup-carve/carve#2514 with #2517, PART 0's continuation-claim
    // rule: a closed fence establishes no claim, so an unmarked line after one
    // leaves every quote. Eleven documents.
    "a-fence-in-a-quote-stores-no-continuation-claim",
    // Added with the spec bump to carve 89157529, which brings fourteen
    // categories and 97 documents. The corpus diff between the two pins is 97
    // `.crv` files ADDED with none modified and none removed, plus one `.html`
    // golden modified - corpus 372, from markup-carve/carve#2574, which
    // carve-rs#2159 already answers.
    //
    // Every document of every category below was rendered through this engine and
    // diffed against its committed HTML before the category was listed here, and
    // `all_implemented_corpus_pairs_match` re-runs that comparison the moment the
    // line exists, which is what makes each one a claim rather than a blessing.
    // Each line names the upstream merge that brought the category, read off the
    // commit that added its first document.
    //
    // 514 is markup-carve/carve#2544: a fence a container inside a quote holds
    // open stores no continuation claim, at every depth. Twelve documents.
    "a-fence-a-container-inside-a-quote-holds-open-stores-no-claim",
    // 515 is markup-carve/carve#2549: a nested marker comment's ownership does
    // not depend on where its closer sits. Five documents.
    "a-nested-marker-comment-keeps-its-own-ownership",
    // 516 is markup-carve/carve#2547: stripping a heading's comment preserves the
    // inline spans around it. Eight documents.
    "a-heading-comment-preserves-code-span-content",
    // 517 is markup-carve/carve#2558: list tightness is read at every column the
    // item's paragraph text reaches, so a band paragraph after an invisible line
    // leaves the item loose. Five documents, and carve-rs#2143 pinned them.
    "a-band-paragraph-after-an-invisible-line-leaves-the-item-loose",
    // 518 is markup-carve/carve#2562: a trailing comment takes a tab, a run start
    // and its whole separator, in every inline host. Thirteen documents, and
    // carve-rs#2158 is the change that made the two label rows match.
    "a-trailing-comment-takes-a-tab-a-run-start-and-its-whole-separator",
    // 520 is markup-carve/carve#2569: a raw block whose target does not match is
    // dropped rather than written as text, and takes no line in the container that
    // held it. Eight documents, and carve-rs#2154 made them match.
    "a-dropped-raw-block-takes-no-line-in-the-container-that-holds-it",
    // 521 is markup-carve/carve#2574: a zero-line raw payload and a one-blank one
    // are not the same block. Nine documents, and carve-rs#2159 made them match.
    "a-zero-line-and-a-one-blank-raw-payload-are-not-the-same-block",
    // 522 is markup-carve/carve#2577: PART 8 resolves a bracket run before an
    // emphasis marker, so a marker inside a link's label is label text. Three
    // documents, and carve-rs#2161 made the one that failed match.
    "an-emphasis-marker-does-not-pair-across-a-link-bracket",
    // 523 is markup-carve/carve#2577 as well: a link title crosses a soft wrap and
    // an attribute value does not, which forked the two ohm character classes. Six
    // documents, all already conformant here.
    "a-link-title-crosses-a-soft-wrap-and-an-attribute-value-does-not",
    // 524 is markup-carve/carve#2560, pinned by #2579: zero lines preserved
    // literally is zero characters, so a closed empty code fence publishes no
    // newline. Two documents, and carve-rs#2162 made the one that failed match.
    "an-empty-code-payload-renders-no-characters",
    // 525 is markup-carve/carve#2586: a link inside a span's label keeps its
    // destination. Nine documents, all already conformant here.
    "a-link-inside-a-span-s-label-keeps-its-destination",
    // 526 is markup-carve/carve#2589: a quoted attribute value and a quoted title
    // escape different sets. Ten documents, all already conformant here.
    "a-quoted-value-and-a-quoted-title-escape-different-sets",
    // 527 is markup-carve/carve#2589 as well: a tab does not open the title slot.
    // Three documents, all already conformant here.
    "a-tab-does-not-open-the-title-slot",
    // 519 is markup-carve/carve#2563: a fence shifted inside a quoted item owns its
    // payload, so an unmarked line after it ends the quote. Four documents, three
    // of which carve-rs#2170 answered - they arrived with the pin declared in
    // BEHIND_ON_PAIR and that declaration is gone with the fix.
    "a-shifted-fence-in-a-quoted-item-stores-no-continuation-claim",
    "a-container-label-publishes-its-inline-run",
    "a-fence-after-a-footnote-quote-has-its-own-base",
    "a-footnote-body-whose-every-block-renders-nothing-is-an-empty-body",
    "an-opener-under-a-quote-in-a-nested-host-opens-at-one-column-only",
    // All 66 documents of these four categories render the pinned corpus HTML
    // byte for byte already; only the declaration was missing.
    "a-container-label-preserves-closed-inline-constructs-before-cutting-a-comment",
    "a-braced-span-cannot-close-beyond-its-bracket-run",
    "quoted-values-and-titles-retain-a-non-punctuation-backslash",
    "a-marker-line-opaque-quote-keeps-overindented-markers-literal",
    "a-denied-destination-takes-one-render-loss-row-per-sink",
    // markup-carve/carve#2732: every name lookup compares case exactly.
    "every-name-lookup-compares-case-exactly",
    // Brought in by the same pin bump; renders byte for byte with no engine work.
    "an-unreferenced-footnote-definition-takes-its-links-out-of-the-render",
    "an-info-string-does-not-change-how-a-flush-left-fence-folds-into-a-description-body",
    // markup-carve/carve#2752, arrived with the bump to carve 5fbf8aed:
    // renders byte for byte on this build with no engine work.
    "a-description-body-whose-own-block-is-a-fence-keeps-no-line-below-its-column",
    "a-below-column-line-continues-a-paragraph-only-where-one-is-open",
    // Published raw-content contract, carve#2911: all three HTML goldens match.
    "an-unterminated-raw-fence-keeps-literal-payload-termination",
    "a-closed-block-leaves-no-frame-for-the-line-below",
    "a-container-boundary-is-read-at-the-container-s-own-column",
    "a-description-keeps-a-block-at-its-body-column-whatever-leads-the-item",
    "a-line-block-payload-span-pairs-past-the-content-column",
    "a-line-final-run-of-spaces-and-tabs-is-dropped-before-the-line-is-read",
    "a-line-retained-after-a-comment-opens-a-paragraph",
    "a-paragraph-in-a-held-container-does-not-loosen-the-item",
    "a-quoted-item-that-ends-on-a-block-past-its-column-holds-no-paragraph",
    "a-tab-inside-a-line-is-content-in-a-label-and-after-a-definition-marker",
    "a-tab-is-content-in-a-heading-key-and-in-a-cross-reference-id",
    "a-closed-fence-in-a-quoted-sub-item-after-a-lazy-line-leaves-no-paragraph",
    "a-description-marker-followed-by-a-no-break-space-has-content",
    "a-footnote-reference-in-link-text-keeps-its-id-on-the-sup",
    "a-head-row-alignment-marker-sets-the-whole-column",
    "a-line-block-takes-a-lazy-line-whatever-its-last-payload-line",
    "a-quoted-term-takes-a-lazy-line",
    "a-shifted-fence-in-a-quoted-container-s-body-stores-no-continuation-claim",
    "a-titled-footnotes-marker-names-the-endnotes-section",
    "an-at-column-comment-retains-nothing",
    "an-at-sign-before-a-space-and-a-doubled-delimiter-inside-bold-italic-are-text",
    "an-opener-past-a-hosted-container-s-column-interrupts-its-open-paragraph",
    "an-opener-past-a-hosted-container-s-column-opens-there",
    "a-titled-footnotes-marker-names-the-endnotes-section",
    "an-at-sign-before-a-space-and-a-doubled-delimiter-inside-bold-italic-are-text",
    "a-closed-fence-at-a-marker-line-note-s-body-column-opens-in-the-note",
    "a-span-never-covers-another-cell",
    "a-quote-or-a-term-at-a-marker-line-note-s-body-column-opens-in-the-note",
    "a-comment-between-a-heading-and-a-colon-fence-leaves-the-fence-an-opener",
    "a-comment-in-a-marker-line-note-changes-nothing",
    "a-comment-closes-a-marker-line-note-s-paragraph-and-an-unclosed-fence-below-it-opens-in-the-note",
    "a-fence-past-the-column-after-an-attribute-line-opens-on-its-authored-base",
    "a-footnote-definition-past-a-box-s-column-is-hoisted",
    "a-fence-that-leaves-a-closed-nested-item-opens-in-the-host-it-lands-in",
    "a-link-definition-or-an-attribute-line-past-a-box-s-column-opens-on-its-authored-base",
    // CARVE-P9-082: an explicitly delimited row of blank cells is a table row.
    "an-explicitly-delimited-row-of-blank-cells-is-a-table",
    // carve#3139, arrived with the bump to carve dc8e6905.
    "a-closed-fence-in-a-nested-child-leaves-no-paragraph-open",
    "a-code-line-left-of-an-authored-base-fence-opener-is-measured-from-that-base",
    "an-autolink-is-opaque-to-the-bracket-scan",
    "editorial-markup-closes-outside-a-cross-reference-or-a-bracket-run",
    "the-combined-bold-italic-token-does-not-open-after-an-underscore-or-a-slash",
    "the-combined-bold-italic-token-does-not-open-inside-an-open-bare-italic-or-bold",
    "csv-and-tsv-table-bodies",
    "a-comment-line-leaves-a-description-s-list-item-open-for-a-fence",
    "a-first-block-plus-refuses-a-follower-at-an-intervening-column",
    "a-hosted-quote-s-first-block-plus-refuses-a-block-without-the-host-quote-s-prefix",
    "a-quote-follower-below-stacked-quote-openers-in-an-item-is-read-at-its-raw-column",
    "a-quote-opened-on-an-item-s-marker-line-attaches-a-quote-at-its-own-depth",
    "a-quote-opened-on-its-own-line-in-an-item-takes-the-first-block-form",
    "a-quote-prefixed-plus-takes-one-space-after-each-quote-marker-and-attaches-a-quote",
    "a-quote-s-first-block-plus-takes-a-list-that-carries-its-quote-prefixes-at-its-marker-column",
    "a-separate-plus-after-content-attaches-a-list-to-a-note-or-description",
    "a-stacked-first-block-plus-takes-a-follower-only-at-its-innermost-opener-s-marker-column",
    "a-wrapped-attribute-line-interrupts-like-a-one-line-attribute-line",
    "a-wrapped-attribute-line-opened-at-a-container-s-column-is-one-construct",
    "the-column-of-the-quote-that-owns-a-first-block-plus-stays-eligible-at-every-depth",
    "a-description-separator-left-of-its-item-s-column-folds-into-the-definition-body",
    "a-definition-marker-line-keeps-a-straddling-tab-s-residual-columns",
    "table-spans-keep-alignment-defaults-in-physical-columns",
    "a-bang-before-a-link-is-escaped",
    "a-bare-dot-starts-a-list-after-a-blank-line",
    "a-below-column-fence-stays-in-a-definition-paragraph",
    "a-blank-ends-the-old-quote-s-child-column",
    "a-class-key-value-shares-the-class-slot",
    "a-cross-reference-clones-heading-inline-nodes",
    "a-definition-opens-a-sibling-block-after-a-list",
    "a-description-continues-after-any-blank-run",
    "a-final-tab-does-not-change-list-ownership-in-a-description",
    "a-flush-left-description-leaves-a-deeper-host",
    "a-flush-left-description-leaves-a-nested-term-s-host",
    "a-hard-break-in-a-pipe-table-cell-survives-markdown-export",
    "a-list-import-preserves-its-hard-break",
    "a-marker-line-sublist-stays-open-in-a-description-body",
    "a-non-ascii-letter-does-not-block-a-bare-delimiter",
    "a-partial-braced-opener-before-a-terminal-backslash",
    "a-post-marker-tab-keeps-a-quote-literal",
    "a-promoted-image-leaves-the-task-checkbox-unnamed",
    "a-quoted-link-definition-in-a-footnote-body-registers-globally",
    "a-reference-definition-needs-a-separator-before-its-destination",
    "a-resolved-reference-image-also-leaves-the-checkbox-unnamed",
    "a-tabbed-table-ends-before-a-flush-left-list-line",
    "an-indented-row-after-a-table-continues-it",
    "an-invalid-attribute-block-leaves-a-definition-as-prose",
    "an-unclosed-backtick-run-still-forms-math",
    "an-unresolved-symbol-leaves-a-colon-before-a-quote",
    "authored-inline-footnote-reference-ids",
    "escaped-hyphen-run-in-a-table-cell",
    "escaped-table-hyphens-stay-literal",
    "explicit-references-do-not-use-heading-fallback",
    "frontmatter-closer-at-end-of-input",
    "indented-term-ends-with-its-unclosed-host",
    "invisible-content-and-image-promotion",
    "line-block-inline-note",
    "nested-term-after-a-list-in-a-description",
    "non-row-pipe-keeps-lazy-plus-in-a-description-item",
    "non-row-pipe-keeps-lazy-plus-in-a-nested-item",
    "non-row-pipe-keeps-lazy-plus-in-an-unclosed-div",
    "quotes-open-after-an-unresolved-symbol",
    "raw-inline-content-in-heading-ids",
    "unicode-whitespace-ends-a-link-destination",
    "a-later-term-after-a-shifted-separator-opens-in-the-outer-item",
    "a-task-item-uses-the-ordered-item-column-rule",
    "invalid-table-metadata-keeps-the-table",
    "lazy-quote-owners",
    "lint-directives-are-invisible-in-rendered-content",
    "model-inline-frames-and-html-attributes",
    "model-syntax-boundaries-and-ownership",
    "opaque-reference-and-inline-bounds",
    "the-deepest-quoted-paragraph-takes-a-lazy-line-across-missed-prefixes",
    "visible-footnote-headings",
    "a-follower-at-a-body-hosted-item-s-marker-column-attaches-to-the-item",
    "a-below-column-term-stays-in-the-footnote-paragraph",
    "a-block-glued-to-a-split-combined-token-or-a-forced-span-s-own-delimiter",
    "a-closed-footnote-child-releases-a-flush-line",
    "a-combined-token-inside-a-forced-span-stops-at-the-span-s-closer",
    "a-content-tab-does-not-hide-a-description-boundary",
    "a-description-keeps-a-list-marker-below-its-hosted-footnote",
    "a-footnote-heading-does-not-loosen-its-host-item",
    "a-footnote-label-may-hold-a-bracket",
    "a-footnote-reference-bounds-an-editorial-closer-scan",
    "a-generic-extension-parses-its-captured-content-as-inline-content",
    "a-guard-that-closes-only-inside-its-guarded-child-s-block",
    "a-heading-past-a-body-hosted-item-s-marker-column-refuses-its-plus-and-opens-in-the-description",
    "a-lazy-line-keeps-its-plus-through-three-quote-levels",
    "a-literal-bracket-run-bounds-emphasis-like-a-resolved-one",
    "a-matched-insertion-or-deletion-hides-a-substitution-s-closer",
    "a-mention-or-tag-keeps-its-full-spelling-in-a-strong-child",
    "a-native-header-continues-a-promoted-gfm-header-run",
    "a-nested-braced-span-hides-an-editorial-closer",
    "a-promoted-gfm-header-keeps-following-native-headers-before-the-body",
    "a-promoted-image-merges-block-attributes-before-inline-attributes",
    "a-quote-at-the-start-of-a-span-opens",
    "a-table-ends-a-footnote-paragraph-claim",
    "an-attribute-block-after-a-span-behind-a-guarding-opener-belongs-to-that-span",
    "an-attribute-block-that-attaches-to-nothing-is-text-from-the-first-pass",
    "an-editorial-comment-contributes-nothing-to-a-heading-id",
    "an-intervening-column-refuses-a-description-hosted-first-block",
    "inside-a-note-a-footnote-shaped-run-is-a-bracket-run",
    "inside-a-note-a-nested-note-opener-is-a-caret-and-a-bracket-run",
    "invalid-body-row-metadata-leaves-one-table-body",
    "a-task-item-uses-its-authored-term-base-one-column-past-a-child-marker",
    "a-task-item-keeps-a-lazy-term-one-column-below-its-child-marker",
    "a-hosted-footnote-keeps-a-heading-at-its-body-floor",
    "a-note-claimed-fence-line-keeps-the-item-loose",
    "a-block-glued-to-a-combined-closer-inside-a-braced-emphasis-span",
    "a-braced-opener-forms-before-an-enclosing-closer-takes-its-block",
    "a-braced-opener-whose-closers-the-spans-inside-it-take-stays-text",
    "a-glued-attribute-run-is-one-physical-line",
    "a-line-below-a-list-item-folds-into-the-open-paragraph-of-a-div-or-an-absorbed-colon-run",
    "a-verbatim-run-a-continuation-row-opens-stays-open-into-the-next-continuation-row",
    "a-wrapped-attribute-opener-indented-in-a-body-is-an-attribute-line",
    "the-rest-of-a-split-combined-token-is-read-again-as-inline-content",
    "whitespace-after-a-leading-content-tab-is-not-content",
    "a-comment-inside-a-split-combined-token-is-removed-before-pairing",
    "a-first-block-plus-hands-its-block-to-the-host-as-the-host-reads-it",
    "an-opener-is-open-where-it-stands-so-a-combined-token-after-a-guarded-star-stays-split",
    "the-empty-block-is-a-member-of-a-bracketed-span-s-attribute-run",
    // The carve 8d8bd4a9 pin; rows this engine has not reached are in BEHIND_ON_PAIR.
    "a-block-image-ignores-a-construct-that-renders-nothing",
    "a-class-key-value-splits-into-class-tokens",
    "a-closerless-comment-fence-opens-no-span-in-a-body",
    "a-code-span-bracket-run-or-arrow-inside-an-inner-pair-keeps-the-outer-opener-text",
    "a-comment-closes-only-at-a-closer-the-author-wrote",
    "a-comment-marker-at-the-start-of-editorial-content-is-text",
    "a-comment-or-link-definition-at-a-body-s-column-refuses-a-first-block-plus",
    "a-description-a-plus-filled-stays-open-for-a-sibling",
    "a-column-zero-description-is-a-sibling-after-an-indented-description-s-refused-plus",
    "a-description-body-in-an-item-refuses-a-column-zero-block-after-its-first-block-plus",
    "a-description-lead-under-a-marker-line-child-folds-into-its-paragraph",
    "a-description-left-of-its-term-measures-its-own-plus-follower",
    "a-description-line-between-two-lists-columns-is-an-outer-sibling",
    "a-description-s-first-block-plus-leaves-its-paragraph-open-for-the-next-entry",
    "a-description-s-one-attributed-paragraph-keeps-its-p",
    "a-description-sibling-is-recognized-at-any-column",
    "a-column-zero-fence-below-a-refused-nested-first-block-plus-keeps-its-payload",
    "a-fence-in-a-list-a-quote-attaches-takes-no-separator-line",
    "a-first-block-plus-does-not-take-the-closer-of-its-div",
    "a-footnote-body-refuses-a-first-block-at-its-body-column",
    "a-frontmatter-opener-s-rest-is-a-format-token-or-nothing",
    "a-heading-id-reverses-every-double-arrow",
    "a-heading-slug-that-starts-with-any-unicode-number-takes-the-s-prefix",
    "a-hyphen-run-at-the-end-of-a-span-converts",
    "a-hyphen-run-at-the-start-of-a-span-is-a-flag",
    "a-lazy-line-after-a-narrower-list-in-a-note-folds-into-its-item",
    "a-lazy-line-into-a-descendant-s-paragraph-keeps-the-descendant-the-deepest-frame",
    "a-line-below-an-empty-div-after-a-first-block-plus-is-not-lazy",
    "a-list-marker-under-a-note-paragraph-in-a-description-is-text",
    "a-literal-directive-and-a-quote-inside-a-reference-link",
    "a-long-backtick-run-or-a-long-run-of-unclosed-brackets-renders",
    "a-marker-folds-into-the-paragraph-a-first-block-plus-took",
    "a-marker-the-left-boundary-refuses-is-an-ordinary-character",
    "a-padded-plus-after-a-quote-marker-is-the-quote-s-text",
    "a-plus-at-a-stacked-item-s-marker-column-reaches-it",
    "a-quote-after-a-caption-starts-fresh",
    "a-quote-before-a-span-closer-closes",
    "a-quote-on-a-description-body-marker-line-takes-a-plus-at-its-column",
    "a-quoted-row-attribute-value-may-hold-a-pipe",
    "a-quoted-term-takes-no-lazy-marker-or-separator",
    "a-reference-link-around-a-literal-directive-renders",
    "a-refused-plus-reads-like-a-letter",
    "a-row-attribute-block-follows-the-last-closing-pipe",
    "a-span-marker-with-no-origin-keeps-its-column-s-alignment",
    "a-tab-past-an-off-stop-frame-column-is-measured-in-that-frame",
    "a-task-box-names-an-unresolved-image-by-its-alt-as-written",
    "a-toc-marker-that-places-nothing-renders-the-kind-floor",
    "a-token-inside-a-literal-include-directive-opens-nothing",
    "an-ambiguous-first-marker-keeps-its-own-dialect",
    "an-arrow-a-highlight-closes-leaves-its-rest-to-read-with-what-follows",
    "an-attribute-run-after-a-lone-plus-leads-a-sublist-at-the-content-column",
    "an-escaped-space-and-a-hard-break-work-inside-a-forced-span",
    "an-escaped-space-before-a-comment-marker-separates-it",
    "an-id-dropped-with-an-unresolved-reference-reserves-nothing",
    "an-indented-attribute-block-that-wraps-across-lines-is-an-attribute-line",
    "an-indented-continuation-row-continues-the-table-above",
    "an-invalid-table-count-keeps-native-header-cells-in-the-body",
    "an-item-on-a-body-s-marker-line-refuses-a-column-zero-block-after-its-plus",
    "an-open-div-s-closer-is-not-absorbed",
    "an-unclosed-fence-opener-interrupts-a-quote-or-item-paragraph",
    "an-unprefixed-line-does-not-reach-a-quoted-note-s-first-block",
    "an-unresolved-reference-keeps-its-whole-attribute-run-as-text",
    "block-reading-uses-only-carve-whitespace",
    "content-behind-a-lead-tab-offers-no-first-block",
    "the-entry-after-a-first-block-plus-description-is-its-sibling",
    "the-equals-of-a-comparison-neither-opens-nor-closes-a-highlight",
    "the-equals-of-a-not-equal-sign-neither-opens-nor-closes-a-highlight",
    // The carve 3679436f pin; rows this engine has not reached are in BEHIND_ON_PAIR.
    "a-block-after-a-nested-note-at-the-outer-note-s-body-column-stays-in-that-note",
    "a-comment-closer-below-the-content-column-changes-no-ownership",
    "a-definition-at-a-plus-attached-description-s-body-column-registers",
    "a-definition-at-column-0-stands-in-the-document",
    "a-description-marker-line-without-a-paragraph-gives-the-next-line-no-lazy-host",
    "a-first-block-plus-shape-reads-the-one-attach-rule",
    "a-lazy-line-continues-a-quoted-term-only-as-plain-text",
    "a-lazy-line-folds-into-the-paragraph-a-line-after-an-ended-descendant-started",
    "a-line-after-a-marker-line-folded-into-a-sublist-item-folds-too",
    "a-marker-under-a-quoted-description-paragraph-in-an-item-is-text",
    "a-nested-unclosed-fence-keeps-every-blank-line",
    "a-plus-a-second-plus-follows-hands-the-quote-its-next-block",
    "a-plus-after-a-note-in-an-item-gives-the-note-its-list",
    "a-quoted-colon-line-is-absorbed-only-by-its-own-paragraph",
    "a-refused-plus-before-an-attribute-run-and-a-sibling-is-a-letter",
    "a-whitespace-run-after-a-tab-sets-the-content-column-where-the-content-starts",
    "an-accepted-plus-needs-a-follower-that-reaches-its-host",
    "an-indented-list-starts-a-new-list-at-a-different-marker",
    // The carve 5643d59c pin; rows this engine has not reached are in BEHIND_ON_PAIR.
    "a-blank-before-a-plus-in-a-stacked-sublist-does-not-loosen-the-outer-item",
    "a-blanked-style-still-takes-the-alignment",
    "a-braced-comment-line-after-a-plus-is-the-attached-paragraph",
    "a-bullet-line-after-a-hostless-caption-line-leaves-later-indented-blocks-out-of-the-paragraph",
    "a-caption-host-and-a-continuation-cell-read-what-the-block-holds",
    "a-caret-shaped-lazy-line-keeps-a-quote-in-an-item",
    "a-cell-s-escaped-pipe-reaches-a-link-destination",
    "a-child-on-a-note-s-marker-line-is-measured-from-its-source-column",
    "a-cloned-heading-drops-its-ids",
    "a-closer-or-caption-at-a-note-marker-line-item-s-content-column-is-the-item-s",
    "a-code-span-before-a-literal-directive-closes-inside-it",
    "a-comment-at-the-start-of-content-counts-as-nothing-for-quotes",
    "a-comment-block-does-not-count-toward-a-description-s-blocks",
    "a-comment-block-in-a-line-block-leaves-nothing",
    "a-comment-fence-on-a-nested-marker-line-closes-at-the-first-exact-width-closer",
    "a-computed-span-drops-the-author-copy-of-its-key",
    "a-continuation-cell-past-the-last-column-is-a-new-cell",
    "a-cross-reference-to-a-repeated-explicit-id-finds-the-first",
    "a-cross-reference-to-an-empty-caption-label-is-the-number-alone",
    "a-denied-href-or-src-on-a-non-link-is-blanked",
    "a-description-s-first-block-plus-in-a-footnote-body-takes-a-paragraph-at-the-note-s-body-column",
    "a-div-closes-only-on-a-run-of-its-own-width",
    "a-fence-a-note-s-marker-line-item-opens-ends-at-a-column-zero-line",
    "a-fence-on-a-note-line-in-an-item-holds-no-paragraph",
    "a-first-block-plus-whose-next-line-misses-the-opener-s-marker-column-is-a-letter",
    "a-hard-break-is-whitespace-to-a-typography-flank",
    "a-leading-content-tab-sets-the-item-s-content-column-where-the-content-starts",
    "a-line-at-a-body-s-column-after-a-plus-attached-quote-holding-an-empty-div-stays-in-the-body",
    "a-line-at-a-description-s-body-column-in-a-div-in-an-item-stays-in-the-description",
    "a-line-below-a-bare-run-a-paragraph-absorbed-in-a-div-folds-into-it",
    "a-line-below-a-description-after-a-div-in-an-item-leaves-the-description",
    "a-line-below-a-note-description-past-a-closed-block-leaves-the-note",
    "a-line-below-a-note-div-past-a-closed-block-leaves-the-note",
    "a-line-below-an-unclosed-fence-in-a-described-item-keeps-its-lazy-line",
    "a-line-below-the-column-after-a-note-hosted-description-leaves-the-note",
    "a-line-folded-into-a-child-s-paragraph-is-not-the-item-s-text",
    "a-list-marker-between-an-item-s-columns-is-a-sibling-only-on-every-axis",
    "a-list-marker-inside-an-item-ends-a-caption-like-a-paragraph",
    "a-literal-bracket-run-is-a-typography-region",
    "a-lone-plus-in-a-fenced-quote-is-text",
    "a-longer-bare-colon-run-does-not-end-a-line-block",
    "a-marker-after-a-note-plus-paragraph-is-text",
    "a-marker-past-a-term-ends-its-note",
    "a-marker-that-misses-a-description-folds-into-its-item",
    "a-non-core-element-name-and-a-quote-after-a-closed-span",
    "a-note-body-and-an-extension-s-content-hold-a-comment-s-run",
    "a-note-in-a-losing-note-keeps-its-whole-body",
    "a-paragraph-a-lone-plus-attaches-stays-the-item-s-open-paragraph",
    "a-paragraph-after-a-closed-third-level-item-takes-a-lazy-line",
    "a-paragraph-after-a-note-attached-fence-closed-past-a-blank-takes-a-lazy-line",
    "a-paragraph-at-an-indented-quote-s-own-marker-column-takes-its-first-block-plus",
    "a-plus-after-a-table-with-an-indented-row-attaches",
    "a-plus-at-a-quote-on-a-later-description-body-s-marker-line-attaches-the-next-block",
    "a-plus-at-the-column-of-a-container-opened-on-another-marker-line-attaches-the-next-block",
    "a-quote-at-the-content-column-stays-in-the-item-after-a-refused-separator",
    "a-quote-in-a-bare-include-token-opens-no-run",
    "a-refused-plus-after-a-block-a-quote-s-plus-attached-folds-into-the-open-paragraph",
    "a-refused-plus-after-an-attached-block-ends-nothing",
    "a-second-description-after-a-task-item-s-marker-line-term-opens-its-own-description",
    "a-second-description-in-an-indented-description-list-opens-its-own-description",
    "a-separator-below-a-term-after-an-item-paragraph-leaves-its-quote",
    "a-separator-below-a-term-in-a-note-item-stays-term-text",
    "a-single-quoted-value-holds-a-closing-brace-on-a-block-attribute-line",
    "a-standalone-code-include-with-no-resolver-is-literal-in-every-host",
    "a-task-marker-at-a-nested-task-item-s-content-column-nests-in-that-item",
    "a-title-on-the-attribute-line-beats-a-fence-header-title",
    "a-typed-div-s-directive-classes-lead-its-attributes",
    "a-wide-gap-before-a-task-box-leaves-the-lazy-frame-out-of-a-term",
    "a-widths-list-writes-one-col-per-entry",
    "a-wrapped-attribute-at-a-footnote-body-s-column-is-one-construct",
    "a-zero-ordered-marker-writes-its-start",
    "an-abbreviation-definition-after-a-plus-is-skipped-like-a-link-definition",
    "an-abbreviation-definition-below-a-content-column-stands-in-the-document",
    "an-abbreviation-does-not-match-in-a-mention-or-an-inline-literal",
    "an-abbreviation-expansion-is-not-scanned-again",
    "an-attribute-merge-and-a-cross-reference-read-the-authored-value",
    "an-email-autolink-takes-every-email-character-in-its-domain",
    "an-extension-s-content-closes-at-the-first-unescaped-bracket",
    "an-html-attribute-is-written-once-whatever-its-case",
    "an-id-key-is-a-spelling-of-the-id-slot",
    "an-id-key-spells-the-id-slot",
    "an-indented-caret-line-after-a-quote-paragraph-that-starts-with-a-caret-folds-into-it",
    "an-indented-comment-block-in-a-line-block-leaves-nothing",
    "an-indented-dash-line-neither-opens-nor-closes-frontmatter",
    "an-invalid-colon-opener-past-a-description-s-content-column-absorbs-the-bare-run",
    "an-item-hands-its-blank-run-to-a-description-or-note-child-s-open-fence",
    "an-unclosed-fence-in-a-description-body-keeps-its-trailing-blank-line",
    "an-undefined-label-and-a-cloned-heading-title",
    "an-unresolved-cross-reference-strips-a-bidi-control",
    "attribute-hardening-probes-the-value-as-emitted",
    "brackets-nested-past-the-cap-stay-literal",
    "every-folded-caret-line-loses-its-leading-whitespace-in-a-deep-term",
    "invisible-lines-after-a-plus-are-skipped-and-the-first-visible-line-decides",
    "the-first-caption-registers-a-repeated-id",
    "a-caption-line-holding-only-a-comment-makes-no-caption",
    "a-delimited-body-s-overlapping-header-and-footer-make-no-header-cells",
    "a-fence-opened-at-a-block-start-in-a-note-keeps-no-paragraph-open",
    "a-folded-marker-keeps-a-note-item-measured-from-its-source-column",
    "a-lazy-line-in-a-note-does-not-keep-the-note-host-open",
    "a-lazy-line-keeps-a-description-in-a-div-in-an-item",
    "a-plus-after-an-attached-block-reaches-its-host-through-quote-prefixes",
    "a-table-writes-one-col-per-column-entry",
    "an-accepted-plus-follower-keeps-its-column-through-an-indented-quote-prefix",
    "an-invisible-item-lead-and-a-caption-line-after-an-accepted-plus",
    "an-item-on-a-note-marker-line-takes-its-plus-at-its-own-marker-column",
    "an-unclosed-fence-under-a-description-paragraph-in-a-note-holds-no-paragraph",
    "every-list-item-measures-its-content-against-its-own-content-column",
    "the-third-confirmation-round-s-model-findings",
];

// Spec-main categories tracked by separate implementation work. Keep them
// explicit so a new category still trips the completeness gate below.
//
// Empty, and an entry earns its place only by FAILING: `a_known_gap_still_fails`
// renders every document in a declared gap and refuses a category that already
// matches. The three entries this list used to hold had all been conformant
// since before the pin they were written against, and nothing noticed, because
// a gap was only ever read as "skip this".
const KNOWN_GAPS: &[&str] = &[];

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

fn expected_corpus_size() -> usize {
    crate::common::expected_corpus_size()
}

/// Pairs this engine renders DIFFERENTLY from the pinned corpus, because it
/// reached a spec rule the vendored `spec` submodule predates.
///
/// Mirrors the same table in carve-js (`test/corpus.test.ts`) and carve-php
/// (`tests/CarveCorpusTest.php`). Each entry FAILS IN BOTH DIRECTIONS, which is
/// the whole point: the output must equal what the spec now states, so an engine
/// regression is caught exactly as the corpus would have caught it, and it must
/// still DIFFER from the pinned golden, so an entry that went stale when the
/// submodule bumped fails and has to be deleted in the same commit.
/// Empty is the normal end state: the pin catching up is what retires an entry,
/// and the `assert_ne!` in `check_pair` is what forces the deletion rather than
/// leaving a declaration that no longer declares anything.
const AHEAD_OF_PIN: &[(&str, &str, &str)] = &[];

/// Pairs this engine renders differently from the pinned corpus because it has
/// not reached the rule yet - the mirror of [`AHEAD_OF_PIN`].
///
/// A category holding a document this engine already answers cannot go in
/// `KNOWN_GAPS`, which is all-or-nothing: `a_known_gap_still_fails` refuses a
/// deferred document that matches, and it is right to. Leaving the category out of
/// IMPLEMENTED instead compares NONE of it, so a pin bump lands green while
/// asserting nothing about the documents beside the ones that fail. So the
/// category is listed and the individual failing pairs are declared here.
///
/// Each entry fails in BOTH directions, exactly as an `AHEAD_OF_PIN` entry does:
/// the output must equal what this engine produces today, so an unrelated
/// regression is still caught on a pair the corpus can no longer speak for, and it
/// must still DIFFER from the golden, so the entry has to be deleted in the commit
/// that fixes the rule rather than left behind as a declaration that no longer
/// declares anything.
///
const BEHIND_ON_PAIR: &[(&str, &str, &str)] = &[
    ("454-a-block-opener-past-a-nested-footnote-definition-opens-in-the-item-4", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<ul>\n  <li>body\n    <ul>\n      <li>item</li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>"),
    ("637-definition-body-note-and-fence-owner-boundaries-4", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<ol>\n  <li>d\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ol>"),
    ("796-a-tab-past-an-off-stop-frame-column-is-measured-in-that-frame", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<ul>\n  <li>\n    <ol>\n      <li>a</li>\n    </ol>\n    <pre><code>x\n</code></pre>\n  </li>\n</ul>"),
    ("796-a-tab-past-an-off-stop-frame-column-is-measured-in-that-frame-2", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<ul>\n  <li>\n    <ul>\n      <li>\n        <ol start=\"10\">\n          <li>a</li>\n        </ol>\n        <p>b</p>\n      </li>\n    </ul>\n  </li>\n</ul>"),
    ("800-a-list-marker-under-a-note-paragraph-in-a-description-is-text-3", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>a</li>\n    </ul>\n    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled aria-label=\"child\"> child</li>\n    </ul>\n  </dd>\n</dl>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("821-a-quote-after-a-caption-starts-fresh", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<figure>\n  <blockquote>\n    <ul>\n      <li>t</li>\n    </ul>\n  </blockquote>\n  <figcaption>p</figcaption>\n</figure>\n<blockquote><p># h\nl</p></blockquote>"),
    ("823-a-fence-in-a-list-a-quote-attaches-takes-no-separator-line-2", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<blockquote>\n  <p>t</p>\n  <ul>\n    <li>\n      <pre><code>\n</code></pre>\n    </li>\n  </ul>\n  <p>b</p>\n</blockquote>"),
    ("823-a-fence-in-a-list-a-quote-attaches-takes-no-separator-line-3", "inline or layout ruling at the 8d8bd4a9 pin not ported yet", "<blockquote>\n  <p>t</p>\n  <ul>\n    <li>x\n      <pre><code>\n</code></pre>\n    </li>\n  </ul>\n  <p>b</p>\n</blockquote>"),
    ("842-a-quoted-colon-line-is-absorbed-only-by-its-own-paragraph", "inline or layout ruling at the 3679436f pin not ported yet", "<blockquote>\n  <p>e\n:::e</p>\n  <div>\n\n  </div>\n</blockquote>\n<p>l</p>"),
    ("842-a-quoted-colon-line-is-absorbed-only-by-its-own-paragraph-2", "inline or layout ruling at the 3679436f pin not ported yet", "<blockquote>\n  <blockquote><p>e\n:::e</p></blockquote>\n  <div>\n    <p>l</p>\n  </div>\n</blockquote>"),
    ("842-a-quoted-colon-line-is-absorbed-only-by-its-own-paragraph-3", "inline or layout ruling at the 3679436f pin not ported yet", "<blockquote>\n  <blockquote>\n    <p>e\n:::e</p>\n    <div>\n\n    </div>\n  </blockquote>\n  <p>l</p>\n</blockquote>"),
    ("842-a-quoted-colon-line-is-absorbed-only-by-its-own-paragraph-4", "inline or layout ruling at the 3679436f pin not ported yet", "<blockquote>\n  <blockquote>\n    <p>e\n:::e</p>\n    <div>\n\n    </div>\n  </blockquote>\n</blockquote>\n<p>l</p>"),
    ("851-a-definition-at-column-0-stands-in-the-document", "inline or layout ruling at the 3679436f pin not ported yet", "<ul>\n  <li>n\n*[A]: b</li>\n</ul>\n<p>A</p>"),
    ("851-a-definition-at-column-0-stands-in-the-document-2", "inline or layout ruling at the 3679436f pin not ported yet", "<ul>\n  <li>\n    <blockquote><p>n\n*[A]: b</p></blockquote>\n  </li>\n</ul>\n<p>A</p>"),
    ("851-a-definition-at-column-0-stands-in-the-document-3", "inline or layout ruling at the 3679436f pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>n\n*[A]: b</dd>\n</dl>\n<p>A</p>"),
    ("853-a-block-after-a-nested-note-at-the-outer-note-s-body-column-stays-in-that-note", "inline or layout ruling at the 3679436f pin not ported yet", "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled aria-label=\"~~~ code ~~~ after\"> ~~~\ncode\n~~~\nafter <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>text<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>text<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("853-a-block-after-a-nested-note-at-the-outer-note-s-body-column-stays-in-that-note-3", "inline or layout ruling at the 3679436f pin not ported yet", "<ul>\n  <li><p>~~~\ncode\n~~~\nafter <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p></li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>text<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>text<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("853-a-block-after-a-nested-note-at-the-outer-note-s-body-column-stays-in-that-note-4", "inline or layout ruling at the 3679436f pin not ported yet", "<ul>\n  <li>::: n\nx\n:::\nafter <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>text<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>text<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("853-a-block-after-a-nested-note-at-the-outer-note-s-body-column-stays-in-that-note-5", "inline or layout ruling at the 3679436f pin not ported yet", "<ul>\n  <li>~~~\n~~~\n~</li>\n</ul>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>t<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>r<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("853-a-comment-closer-below-the-content-column-changes-no-ownership-3", "inline or layout ruling at the 3679436f pin not ported yet", "<ul>\n  <li>a\n    <ul>\n      <li></li>\n    </ul>\n    { - x\n  </li>\n</ul>"),
    ("853-a-comment-closer-below-the-content-column-changes-no-ownership-4", "inline or layout ruling at the 3679436f pin not ported yet", "<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a</p>\n      <p>x<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("853-a-line-after-a-marker-line-folded-into-a-sublist-item-folds-too", "inline or layout ruling at the 3679436f pin not ported yet", "<ol>\n  <li>\n    <ul>\n      <li>a\n        <ul>\n          <li>y\n1. ~~~</li>\n        </ul>\n      </li>\n    </ul>\n    z\n  </li>\n</ol>"),
    ("853-a-marker-under-a-quoted-description-paragraph-in-an-item-is-text", "inline or layout ruling at the 3679436f pin not ported yet", "<ul>\n  <li>\n    <blockquote>\n      <dl>\n        <dt>t</dt>\n        <dd>d</dd>\n      </dl>\n      <ul>\n        <li>z</li>\n      </ul>\n    </blockquote>\n  </li>\n</ul>"),
    ("226-a-definition-attached-by-a-continuation-marker-is-collected-and-the-item-keeps-no-trace", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>a</li>\n</ul>\n<p>see <a href=\"/u\">t</a></p>"),
    ("430-below-a-definition-body-s-column-an-invisible-line-folds-as-text-4", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>d\n*[A]: a\ntail</dd>\n</dl>"),
    ("853-an-abbreviation-definition-below-a-content-column-stands-in-the-document", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>n\n*[A]: b</li>\n</ul>\n<p>A</p>"),
    ("853-an-abbreviation-definition-below-a-content-column-stands-in-the-document-2", "ruling at the 5643d59c pin not ported yet", "<ol start=\"10\">\n  <li>n\n*[A]: b</li>\n</ol>\n<p>A</p>"),
    ("853-an-abbreviation-definition-below-a-content-column-stands-in-the-document-4", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>n\n*[A]: b</dd>\n</dl>\n<p>A</p>"),
    ("853-an-abbreviation-definition-below-a-content-column-stands-in-the-document-5", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>n\n*[A]: b</dd>\n</dl>\n<p>A</p>"),
    ("853-an-abbreviation-definition-below-a-content-column-stands-in-the-document-6", "ruling at the 5643d59c pin not ported yet", "<p>A<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n\n*[A]: b<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("854-a-quote-in-a-bare-include-token-opens-no-run", "ruling at the 5643d59c pin not ported yet", "<p>{{ it’s_x <span class=\"mention\"><strong>@lines</strong></span>:0-1 }} it’s_y }}</p>"),
    ("854-a-refused-plus-before-an-attribute-run-and-a-sibling-is-a-letter-5", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <ul x=\"\">\n      <li>c</li>\n    </ul>\n  </dd>\n</dl>"),
    ("855-a-line-below-an-unclosed-fence-in-a-described-item-keeps-its-lazy-line", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>x</p>\n  </dd>\n</dl>\n<p>tail</p>"),
    ("855-a-line-below-an-unclosed-fence-in-a-described-item-keeps-its-lazy-line-2", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    <p>x</p>\n  </dd>\n</dl>\n<p>tail</p>"),
    ("855-a-line-below-an-unclosed-fence-in-a-described-item-keeps-its-lazy-line-3", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n        <p>x</p>\n      </dd>\n    </dl>\n    tail\n  </li>\n</ul>"),
    ("855-a-plus-after-a-table-with-an-indented-row-attaches", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>a\n    <table>\n      <tbody>\n        <tr><td>x</td></tr>\n        <tr><td>y</td></tr>\n      </tbody>\n    </table>\n  </li>\n</ul>\n<p>+\nH</p>"),
    ("855-a-refused-plus-after-an-attached-block-ends-nothing", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>o\n    <blockquote><p>q\n+\n&gt; r</p></blockquote>\n  </li>\n</ul>"),
    ("855-a-refused-plus-after-an-attached-block-ends-nothing-2", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code>+\n</code></pre>"),
    ("855-invisible-lines-after-a-plus-are-skipped-and-the-first-visible-line-decides", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li></li>\n</ul>\n<p>text</p>"),
    ("855-invisible-lines-after-a-plus-are-skipped-and-the-first-visible-line-decides-2", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li></li>\n</ul>\n<p>text</p>"),
    ("855-invisible-lines-after-a-plus-are-skipped-and-the-first-visible-line-decides-3", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li></li>\n  <li>b</li>\n</ul>"),
    ("855-invisible-lines-after-a-plus-are-skipped-and-the-first-visible-line-decides-4", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>text</li>\n</ul>"),
    ("855-invisible-lines-after-a-plus-are-skipped-and-the-first-visible-line-decides-6", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li></li>\n</ul>\n<p>text</p>"),
    ("856-a-quote-at-the-content-column-stays-in-the-item-after-a-refused-separator", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>\n    <blockquote>\n      <dl>\n        <dt>t</dt>\n      </dl>\n    </blockquote>\n    : a\n  </li>\n</ul>\n<blockquote><p>q</p></blockquote>"),
    ("860-an-id-key-is-a-spelling-of-the-id-slot-3", "ruling at the 5643d59c pin not ported yet", "<p><strong id=\"\">x</strong></p>"),
    ("860-an-id-key-is-a-spelling-of-the-id-slot-4", "ruling at the 5643d59c pin not ported yet", "<table>\n  <tbody>\n    <tr><td id=\"\">x</td></tr>\n  </tbody>\n</table>"),
    ("865-a-list-marker-inside-an-item-ends-a-caption-like-a-paragraph-3", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>\n    <figure>\n      <blockquote><p>q</p></blockquote>\n      <figcaption>p\n - b</figcaption>\n    </figure>\n  </li>\n</ul>"),
    ("866-an-abbreviation-definition-after-a-plus-is-skipped-like-a-link-definition", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>*[A]: b\ntext</li>\n</ul>\n<p>A</p>"),
    ("866-an-abbreviation-definition-after-a-plus-is-skipped-like-a-link-definition-2", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>*[A]: b\ntext</dd>\n</dl>\n<p>A</p>"),
    ("866-an-abbreviation-definition-after-a-plus-is-skipped-like-a-link-definition-3", "ruling at the 5643d59c pin not ported yet", "<blockquote><p>*[A]: b\ntext</p></blockquote>\n<p>A</p>"),
    ("866-an-abbreviation-definition-after-a-plus-is-skipped-like-a-link-definition-6", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>+\n*[A]: b\ntext</li>\n</ul>\n<p>A</p>"),
    ("867-an-indented-comment-block-in-a-line-block-leaves-nothing", "ruling at the 5643d59c pin not ported yet", "<div class=\"line-block\">\n  <p>&nbsp;&nbsp;%%%<br>\n&nbsp;&nbsp;secret<br>\n&nbsp;&nbsp;%%%</p>\n</div>"),
    ("867-an-indented-comment-block-in-a-line-block-leaves-nothing-2", "ruling at the 5643d59c pin not ported yet", "<div class=\"line-block\">\n  <p>a<br>\n&nbsp;&nbsp;&nbsp;%%%<br>\nsecret<br>\n<br>\nc</p>\n</div>"),
    ("868-a-line-at-a-body-s-column-after-a-plus-attached-quote-holding-an-empty-div-stays-in-the-body", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>\n    <blockquote>\n      <div>\n\n      </div>\n    </blockquote>\n  </li>\n</ul>\n<p>x</p>"),
    ("868-a-line-at-a-body-s-column-after-a-plus-attached-quote-holding-an-empty-div-stays-in-the-body-2", "ruling at the 5643d59c pin not ported yet", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <div>\n\n      </div>\n    </blockquote>\n  </dd>\n</dl>\n<p>x</p>"),
    ("868-a-paragraph-after-a-note-attached-fence-closed-past-a-blank-takes-a-lazy-line", "ruling at the 5643d59c pin not ported yet", "<p>see</p>\n<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code>a\n\n\n</code></pre>\n      <p>c<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("868-a-plus-at-the-column-of-a-container-opened-on-another-marker-line-attaches-the-next-block-3", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x\n+<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("871-a-div-closes-only-on-a-run-of-its-own-width", "ruling at the 5643d59c pin not ported yet", "<div>\n  <p>:::e\n::::\nq\n:::\nafter</p>\n</div>"),
    ("871-a-div-closes-only-on-a-run-of-its-own-width-2", "ruling at the 5643d59c pin not ported yet", "<ul>\n  <li>\n    <div>\n      <p>:::e\n::::\n:::\nl</p>\n    </div>\n  </li>\n</ul>"),
    ("871-a-div-closes-only-on-a-run-of-its-own-width-3", "ruling at the 5643d59c pin not ported yet", "<blockquote>\n  <div>\n    <p>:::e\n::::\nq\n:::</p>\n  </div>\n</blockquote>\n<p>after</p>"),
    ("222-a-tab-as-the-first-character-of-a-definition-term", "new corpus pair at the 19f78369 spec pin, engine port pending", "<dl>\n  <dt>x</dt>\n</dl>"),
    ("439-a-colon-followed-by-only-whitespace-is-not-a-description-8", "new corpus pair at the 19f78369 spec pin, engine port pending", "<dl>\n  <dt>t</dt>\n  <dd>text</dd>\n</dl>\n<p>flush</p>"),
    ("868-a-plus-at-the-column-of-a-container-opened-on-another-marker-line-attaches-the-next-block-2", "new corpus pair at the 19f78369 spec pin, engine port pending", "<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>a\n+\np</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("872-the-third-confirmation-round-s-model-findings-10", "new corpus pair at the 19f78369 spec pin, engine port pending", "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled aria-label=\"q\"> q\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"),
    ("872-the-third-confirmation-round-s-model-findings-11", "new corpus pair at the 19f78369 spec pin, engine port pending", "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled aria-label=\"q\"> q\n    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled aria-label=\"r\"> r</li>\n    </ul>\n  </li>\n</ul>"),
    ("872-the-third-confirmation-round-s-model-findings-12", "new corpus pair at the 19f78369 spec pin, engine port pending", "<ul>\n  <li>{{ _x_ @code:js }}\n    ^ c\n  </li>\n</ul>"),
    ("872-the-third-confirmation-round-s-model-findings-2", "new corpus pair at the 19f78369 spec pin, engine port pending", "<blockquote>\n  <ul>\n    <li></li>\n  </ul>\n  <ul>\n    <li>c</li>\n  </ul>\n</blockquote>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("872-the-third-confirmation-round-s-model-findings-3", "new corpus pair at the 19f78369 spec pin, engine port pending", "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>a<br>\nlazy</p>\n    </div>\n  </li>\n</ul>\n<div>\n  <p>tail</p>\n</div>"),
    ("872-the-third-confirmation-round-s-model-findings-4", "new corpus pair at the 19f78369 spec pin, engine port pending", "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>a<br>\nlazy</p>\n    </div>\n  </dd>\n</dl>\n<div>\n  <p>tail</p>\n</div>"),
    ("872-the-third-confirmation-round-s-model-findings", "new corpus pair at the 19f78369 spec pin, engine port pending", "<ul>\n  <li></li>\n</ul>\n<ul>\n  <li>child</li>\n</ul>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("873-a-delimited-body-s-overlapping-header-and-footer-make-no-header-cells", "new corpus pair at the 19f78369 spec pin, engine port pending", "<table>\n  <tbody>\n    <tr><th scope=\"row\">a</th></tr>\n    <tr><th scope=\"row\">b</th></tr>\n  </tbody>\n</table>"),
    ("873-a-plus-after-an-attached-block-reaches-its-host-through-quote-prefixes-2", "new corpus pair at the 19f78369 spec pin, engine port pending", "<blockquote>\n  <ul>\n    <li>a\n      <h1 id=\"h\">h</h1>\n    </li>\n  </ul>\n  <p>+</p>\n  <h1 id=\"h-2\">h</h1>\n</blockquote>"),
    ("873-an-invisible-item-lead-and-a-caption-line-after-an-accepted-plus-2", "new corpus pair at the 19f78369 spec pin, engine port pending", "<ul>\n  <li><p>Body.</p></li>\n  <li><p>x</p></li>\n</ul>"),
    ("873-an-invisible-item-lead-and-a-caption-line-after-an-accepted-plus-3", "new corpus pair at the 19f78369 spec pin, engine port pending", "<ul>\n  <li><p>Body.</p></li>\n  <li><p>x</p></li>\n</ul>"),
    ("873-an-item-on-a-note-marker-line-takes-its-plus-at-its-own-marker-column-2", "new corpus pair at the 19f78369 spec pin, engine port pending", "<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>a\n          p\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("873-an-item-on-a-note-marker-line-takes-its-plus-at-its-own-marker-column", "new corpus pair at the 19f78369 spec pin, engine port pending", "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>i\n+\nx</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("875-an-accepted-plus-follower-keeps-its-column-through-an-indented-quote-prefix-2", "new corpus pair at the 19f78369 spec pin, engine port pending", "<blockquote>\n  <ul>\n    <li>\n      <ul>\n        <li>\n          <ul>\n            <li></li>\n          </ul>\n        </li>\n      </ul>\n      x\n    </li>\n  </ul>\n</blockquote>"),
    ("878-a-folded-marker-keeps-a-note-item-measured-from-its-source-column-2", "new corpus pair at the 19f78369 spec pin, engine port pending", "<p>q</p>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <ul>\n            <li>a\n* z\n              <h1 id=\"H\">H</h1>\n            </li>\n          </ul>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ("878-a-folded-marker-keeps-a-note-item-measured-from-its-source-column", "new corpus pair at the 19f78369 spec pin, engine port pending", "<p>q</p>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>a\n* z\n          <pre><code></code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
];

fn check_pair(slug: &str) {
    if let Some(failure) = pair_failure(slug) {
        panic!("{failure}");
    }
}

fn pair_failure(slug: &str) -> Option<String> {
    let slug = &resolve_slug(slug);
    let dir = corpus_dir();
    let crv = dir.join(format!("{slug}.crv"));
    let html = dir.join(format!("{slug}.html"));
    let source = fs::read_to_string(&crv).unwrap_or_else(|e| panic!("read {}: {e}", crv.display()));
    let expected =
        fs::read_to_string(&html).unwrap_or_else(|e| panic!("read {}: {e}", html.display()));
    let actual = carve::to_html(&source);
    let (expected, actual) = (expected.trim(), actual.trim());

    if let Some((_, reason, ahead)) = AHEAD_OF_PIN.iter().find(|(name, _, _)| name == slug) {
        return declared_pair_failure(slug, "AHEAD_OF_PIN", reason, ahead, expected, actual);
    }
    if let Some((_, reason, behind)) = BEHIND_ON_PAIR.iter().find(|(name, _, _)| name == slug) {
        return declared_pair_failure(slug, "BEHIND_ON_PAIR", reason, behind, expected, actual);
    }
    mismatch(slug, expected, actual)
}

fn declared_pair_failure(
    slug: &str,
    list: &str,
    reason: &str,
    declared: &str,
    expected: &str,
    actual: &str,
) -> Option<String> {
    if expected == actual {
        return Some(format!(
            "{slug} now matches the pinned corpus: delete its {list} entry"
        ));
    }
    mismatch(&format!("{slug} ({reason})"), declared.trim(), actual)
}

#[test]
fn ahead_of_pin_names_only_cases_the_corpus_has() {
    // A declaration left behind after an upstream RENAME would match no case,
    // run no assertion, and read as coverage.
    let pairs = corpus_pairs();
    let orphaned: Vec<_> = AHEAD_OF_PIN
        .iter()
        .map(|(slug, _, _)| *slug)
        .filter(|slug| !pairs.iter().any(|p| p == slug))
        .collect();
    assert!(
        orphaned.is_empty(),
        "AHEAD_OF_PIN names case(s) the corpus does not have: {orphaned:?}",
    );
}

#[test]
fn behind_on_pair_names_only_cases_the_corpus_has() {
    // The same orphan trap as AHEAD_OF_PIN's: a declaration left behind after an
    // upstream rename matches no case, runs no assertion, and reads as coverage.
    let pairs = corpus_pairs();
    let orphaned: Vec<_> = BEHIND_ON_PAIR
        .iter()
        .map(|(slug, _, _)| *slug)
        .filter(|slug| !pairs.iter().any(|p| p == slug))
        .collect();
    assert!(
        orphaned.is_empty(),
        "BEHIND_ON_PAIR names case(s) the corpus does not have: {orphaned:?}",
    );
}

#[test]
fn every_behind_on_pair_case_is_reached() {
    // `check_pair` only runs for a pair `is_implemented_pair` accepts, and
    // KNOWN_GAPS wins over IMPLEMENTED, so an entry whose category is deferred or
    // unlisted executes NEITHER of its two assertions. Declaring the pair is the
    // whole reason its category is listed; this is what proves it.
    let pairs = corpus_pairs();
    let mut unreachable = Vec::new();
    for (slug, _, _) in BEHIND_ON_PAIR {
        let category = base_category(slug, &pairs);
        if KNOWN_GAPS.contains(&category) {
            unreachable.push(format!("{slug} (category deferred in KNOWN_GAPS)"));
        } else if !IMPLEMENTED.contains(&category) {
            unreachable.push(format!("{slug} (category absent from IMPLEMENTED)"));
        }
        if AHEAD_OF_PIN.iter().any(|(name, _, _)| name == slug) {
            unreachable.push(format!("{slug} (also declared AHEAD_OF_PIN)"));
        }
    }
    assert!(
        unreachable.is_empty(),
        "BEHIND_ON_PAIR entr(ies) that `check_pair` never reaches: {unreachable:?}",
    );
}

#[test]
fn no_case_is_both_deferred_and_ahead_of_pin() {
    // THE PRECEDENCE TRAP. `check_pair` reads AHEAD_OF_PIN and returns, but it
    // only ever runs for a pair `is_implemented_pair` accepts - so an entry
    // whose category sits in KNOWN_GAPS instead of IMPLEMENTED is never
    // reached, and NEITHER of its two assertions ever executes, in either
    // direction. It reads as coverage of a rule this engine has moved past
    // while asserting nothing at all. carve-php had exactly one entry in that
    // state (markup-carve/carve-php#1583's sweep found it), which is what this
    // refuses.
    let mut unreachable = Vec::new();
    let pairs = corpus_pairs();
    for (slug, _, _) in AHEAD_OF_PIN {
        let category = base_category(slug, &pairs);
        if KNOWN_GAPS.contains(&category) {
            unreachable.push(format!("{slug} (category deferred in KNOWN_GAPS)"));
        } else if !IMPLEMENTED.contains(&category) {
            unreachable.push(format!("{slug} (category absent from IMPLEMENTED)"));
        }
    }
    assert!(
        unreachable.is_empty(),
        "AHEAD_OF_PIN entr(ies) that `check_pair` never reaches: {unreachable:?}",
    );
}

#[test]
fn a_known_gap_still_fails() {
    // A gap that quietly started passing is worse than no gap: the category is
    // conformant, unlisted in IMPLEMENTED, and therefore compared against
    // nothing at all. Mirrors what AHEAD_OF_PIN asserts in its own direction.
    let mut stale = Vec::new();
    let pairs = corpus_pairs();
    for slug in &pairs {
        let slug = slug.as_str();
        let category = base_category(slug, &pairs);
        if !KNOWN_GAPS.contains(&category) {
            continue;
        }
        let dir = corpus_dir();
        let source = fs::read_to_string(dir.join(format!("{slug}.crv")))
            .unwrap_or_else(|e| panic!("read {slug}.crv: {e}"));
        let expected = fs::read_to_string(dir.join(format!("{slug}.html")))
            .unwrap_or_else(|e| panic!("read {slug}.html: {e}"));
        if expected.trim() == carve::to_html(&source).trim() {
            stale.push(slug);
        }
    }
    assert!(
        stale.is_empty(),
        "KNOWN_GAPS document(s) already match: move the category to IMPLEMENTED: {stale:?}",
    );
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

fn mismatch(slug: &str, expected: &str, actual: &str) -> Option<String> {
    (expected != actual).then(|| {
        format!(
            "corpus pair `{slug}` did not match.\n\n\
             ----- expected -----\n{expected}\n\
             ----- actual -------\n{actual}\n\
             --------------------\n",
        )
    })
}

#[test]
fn corpus_pairs_present() {
    assert_eq!(
        corpus_pairs().len(),
        expected_corpus_size(),
        "the corpus is truncated"
    );
}

/// Reduce a corpus pair slug to its base category: `NN-slug` or
/// `NN-slug-MM` -> `NN-slug`. A trailing `-<digits>` is a variant suffix and is
/// dropped so all variants of a category map to the single IMPLEMENTED entry.
/// A CATEGORY MAY END IN A NUMBER OF ITS OWN, so the variant suffix is dropped
/// only when what remains names a pair that exists.
/// `an-empty-description-body-claims-no-line-below-column-0` is the category and
/// `-0-2` through `-0-7` are its variants; stripping the `-0` off the base pair
/// invented a category no pair carries, which `all_corpus_categories_implemented`
/// then demanded and `all_implemented_pairs_exist` refused - one category the two
/// tests could not both be satisfied on.
fn base_category<'a>(slug: &'a str, pairs: &[String]) -> &'a str {
    let slug = strip_leading_number(slug);
    if let Some((head, tail)) = slug.rsplit_once('-') {
        if !tail.is_empty()
            && tail.bytes().all(|b| b.is_ascii_digit())
            && pairs.iter().any(|pair| strip_leading_number(pair) == head)
        {
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
    let pairs = corpus_pairs();
    for slug in &pairs {
        let category = base_category(slug, &pairs);
        if !IMPLEMENTED.contains(&category) && !KNOWN_GAPS.contains(&category) {
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
    let failures: Vec<_> = corpus_pairs()
        .iter()
        .filter(|slug| is_implemented_pair(slug))
        .filter_map(|slug| pair_failure(slug))
        .collect();
    assert!(
        failures.is_empty(),
        "{} corpus pair(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n"),
    );
}

#[test]
fn a_declared_pair_fails_when_stale_or_drifted() {
    let failure = |declared, actual| {
        declared_pair_failure("s", "BEHIND_ON_PAIR", "r", declared, "golden", actual)
    };
    assert_eq!(failure("today", "today"), None);
    assert!(failure("golden", "golden")
        .unwrap()
        .contains("delete its BEHIND_ON_PAIR entry"));
    assert!(failure("today", "golden")
        .unwrap()
        .contains("delete its BEHIND_ON_PAIR entry"));
    assert!(failure("today", "drifted")
        .unwrap()
        .contains("did not match"));
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
    c_boolean_and_key_value_same_name,
    "a-boolean-and-a-key-value-of-the-same-name-are-one-attribute"
);
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
    "a-heading-marker-opens-at-an-indented-top-level-column"
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
corpus_test!(
    c_cell_attributes_bind_after_the_kind_and_alignment_markers,
    "cell-attributes-bind-after-the-kind-and-alignment-markers"
);
corpus_test!(
    c_a_table_cell_s_marker_run_ends_at_a_space,
    "a-table-cell-s-marker-run-ends-at-a-space"
);
corpus_test!(
    c_the_canonical_writer_glues_a_code_fence_to_its_info_string,
    "the-canonical-writer-glues-a-code-fence-to-its-info-string"
);
corpus_test!(
    c_a_footnotes_placement_marker_inside_a_container_does_not_place,
    "a-footnotes-placement-marker-inside-a-container-does-not-place"
);
corpus_test!(
    c_a_title_or_label_fills_the_container_body_slot,
    "a-title-or-label-fills-the-container-body-slot"
);
corpus_test!(
    c_a_comment_line_s_text_is_content_and_a_block_body_is_payload,
    "a-comment-line-s-text-is-content-and-a-block-body-is-payload"
);
corpus_test!(
    c_a_footnotes_marker_renders_its_authored_blocks_before_the_placed_section,
    "a-footnotes-marker-renders-its-authored-blocks-before-the-placed-section"
);
corpus_test!(
    c_an_unplaced_footnotes_marker_keeps_its_authored_blocks_inside_the_div,
    "an-unplaced-footnotes-marker-keeps-its-authored-blocks-inside-the-div"
);
corpus_test!(
    c_a_core_directive_kind_class_leads_authored_attributes,
    "a-core-directive-kind-class-leads-authored-attributes"
);
corpus_test!(
    c_an_attribute_line_under_an_attributed_sub_item_stays_in_that_item,
    "an-attribute-line-under-an-attributed-sub-item-stays-in-that-item"
);
corpus_test!(
    c_an_unclosed_fence_after_an_attribute_line_in_a_container_ends_at_its_closer,
    "an-unclosed-fence-after-an-attribute-line-in-a-container-ends-at-its-closer"
);
corpus_test!(
    c_a_quoted_comment_or_attribute_line_closes_the_quote_s_paragraph,
    "a-quoted-comment-or-attribute-line-closes-the-quote-s-paragraph"
);
corpus_test!(
    c_a_block_opener_indented_under_a_definition_term_is_term_text_at_every_depth,
    "a-block-opener-indented-under-a-definition-term-ends-the-term-at-every-depth"
);
corpus_test!(
    c_an_explicitly_delimited_row_of_blank_cells_is_a_table,
    "an-explicitly-delimited-row-of-blank-cells-is-a-table"
);

#[test]
fn declared_population_counts_pairs_and_ignores_literal_fences() {
    let source = "````text\n::: compare\n```carve\nfake\n```\n```html\nfake\n```\n:::\n````\n::: compare no-render\n````carve\n::: compare\n```html\nliteral\n```\n:::\n````\n```html\n<p>first</p>\n```\n```carve\nsecond\n```\n```html\n<p>second</p>\n```\n:::\n";
    assert_eq!(crate::common::count_declared_pairs(source), Ok(2));
    assert_eq!(
        crate::common::count_declared_pairs(&source.replace('\n', "\r\n")),
        Ok(2)
    );
}

#[test]
fn declared_population_refuses_invalid_source() {
    for source in [
        "::: compare\n:::",
        "::: compare\n```carve\nx\n```\n:::",
        "::: compare\n```html\nx\n```\n:::",
        "::: compare",
    ] {
        assert!(
            crate::common::count_declared_pairs(source).is_err(),
            "{source:?}"
        );
    }
}

corpus_test!(
    c_span_marker_continuation,
    "a-continuation-on-a-span-marker-joins-the-cell-the-marker-resolves-into"
);
corpus_test!(
    c_table_row_ends_description_body,
    "a-table-row-ends-a-description-body-and-a-lone-plus-folds-into-a-term"
);
corpus_test!(c_csv_and_tsv_table_bodies, "csv-and-tsv-table-bodies");
corpus_test!(
    c_continuation_marker_uses_its_container_column,
    "a-continuation-marker-uses-its-container-column"
);
corpus_test!(
    c_first_block_marker_on_an_indented_opener_reads_two_columns,
    "a-first-block-marker-on-an-indented-opener-reads-only-its-marker-column"
);
corpus_test!(
    c_refused_continuation_plus_remains_text,
    "a-refused-continuation-plus-remains-text"
);
corpus_test!(
    c_refused_plus_without_the_inner_quote_prefix_renders_as_text,
    "a-refused-plus-without-the-inner-quote-prefix-renders-as-text"
);
corpus_test!(
    c_task_item_takes_a_first_block_continuation_marker_after_its_checkbox,
    "a-task-item-takes-a-first-block-continuation-marker-after-its-checkbox"
);
