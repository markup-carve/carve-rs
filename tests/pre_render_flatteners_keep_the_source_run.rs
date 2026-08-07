//! Pre-render passes that derive DISPLAY text must not resolve the smart
//! punctuation glyph themselves (markup-carve/carve-rs#769).
//!
//! The smart-typography switch is DOCUMENT-GLOBAL and applies to EVERY target
//! (PART 9 §19, AST REPRESENTATION): with it set to source, "every trigger
//! character survives as the ASCII the author typed". A pass that runs before
//! any renderer and flattens an inline tree to a glyph string has answered that
//! question in a subsystem the switch cannot reach, so no renderer change can
//! recover the run - the same argument PART 9R R4 makes for the cross-reference
//! label (markup-carve/carve-rs#768).
//!
//! The inverse is pinned just as hard: an IDENTIFIER must stay byte-identical in
//! both modes, because an id may not depend on presentational typography (PART 9
//! §19 says heading ids are "BYTE-IDENTICAL either way"). Every case below that
//! asserts a run also asserts the id beside it did not move.

use carve::{
    CrossrefStyle, HeadingNumbers, HeadingNumbersOptions, Index, Options, Profile,
    SmartTypographyMode, TableOfContents, TocPlacement,
};

const SOURCE: SmartTypographyMode = SmartTypographyMode::Source;
const GLYPH: SmartTypographyMode = SmartTypographyMode::Glyph;

fn opts(smart: SmartTypographyMode) -> Options<'static> {
    let mut o = Options::new();
    o.smart_typography = smart;
    o
}

// ---------------------------------------------------------------------------
// 1. profile degrade-to-text: the INLINE extractor
// ---------------------------------------------------------------------------

fn minimal(smart: SmartTypographyMode) -> Options<'static> {
    let mut o = opts(smart);
    o.profile = Some(Profile::minimal());
    o
}

/// `minimal` denies `link`, so the link degrades to its label text. The label is
/// the author's own run; degrading it must not spell it in glyphs the caller did
/// not ask for.
#[test]
fn a_degraded_inline_keeps_the_source_run_on_every_target() {
    let src = "A [x -- y](https://e.example) b\n";

    assert_eq!(
        carve::to_html_with_options(src, &minimal(SOURCE)).trim(),
        "<p>A x -- y b</p>"
    );
    assert_eq!(
        carve::to_plain_text_with_options(src, &minimal(SOURCE)).trim(),
        "A x -- y b"
    );
    assert_eq!(
        carve::to_markdown_with_options(src, &minimal(SOURCE)).trim(),
        "A x -- y b"
    );
}

/// The default is unchanged: the glyph is still what a caller who did not ask
/// for the source run gets.
#[test]
fn a_degraded_inline_still_resolves_the_glyph_by_default() {
    let src = "A [x -- y](https://e.example) b\n";
    assert_eq!(
        carve::to_html_with_options(src, &minimal(GLYPH)).trim(),
        "<p>A x \u{2013} y b</p>"
    );
}

// ---------------------------------------------------------------------------
// 2. profile degrade-to-text: the BLOCK extractor (the second producer)
// ---------------------------------------------------------------------------

/// `block_to_text` is a separate producer of the same value: a denied BLOCK is
/// replaced by a paragraph of flattened text. Fixing only the inline extractor
/// would have left this one glyph-pinned.
#[test]
fn a_degraded_block_keeps_the_source_run() {
    let quote = "> A -- b\n";
    assert_eq!(
        carve::to_html_with_options(quote, &minimal(SOURCE)).trim(),
        "<p>&gt; A -- b</p>"
    );
    assert_eq!(
        carve::to_html_with_options(quote, &minimal(GLYPH)).trim(),
        "<p>&gt; A \u{2013} b</p>"
    );

    // A table degrades through the row/cell arms of the same extractor.
    let table = "| a -- b |\n|---|\n| c -- d |\n";
    let out = carve::to_html_with_options(table, &minimal(SOURCE)).to_string();
    assert!(out.contains("a -- b"), "{out}");
    assert!(out.contains("c -- d"), "{out}");
}

// ---------------------------------------------------------------------------
// 3. the index entry's display text
// ---------------------------------------------------------------------------

fn index_html(smart: SmartTypographyMode) -> String {
    let index = Index::new();
    let mut o = opts(smart);
    o.extensions.push(&index);
    // `Options` borrows the extension, so render inside this scope.
    let out = carve::to_html_with_options(
        "A :index[the \"quoted\" -- term] here.\n\n::: index\n:::",
        &o,
    );
    out.trim().to_string()
}

#[test]
fn an_index_entry_keeps_the_source_run_and_its_slug_does_not_move() {
    let source = index_html(SOURCE);
    let glyph = index_html(GLYPH);

    assert!(
        source.contains("<li>the \"quoted\" -- term "),
        "display text should carry the author's run: {source}"
    );
    assert!(
        glyph.contains("<li>the \u{201c}quoted\u{201d} \u{2013} term "),
        "the glyph stays the default: {glyph}"
    );

    // The SLUG is an identifier: byte-identical in both modes, in the marker's
    // id and in the back-link href alike.
    for out in [&source, &glyph] {
        assert!(
            out.contains("id=\"idx-the-quoted-term-1\""),
            "slug must not follow the mode: {out}"
        );
        assert!(
            out.contains("href=\"#idx-the-quoted-term-1\""),
            "back-link must not follow the mode: {out}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. the table-of-contents entry text
// ---------------------------------------------------------------------------

#[test]
fn a_toc_entry_keeps_the_source_run_and_its_href_does_not_move() {
    let src = "# The \"quoted\" -- heading\n";

    for (smart, want) in [
        (SOURCE, "The &quot;quoted&quot; -- heading"),
        (GLYPH, "The \u{201c}quoted\u{201d} \u{2013} heading"),
    ] {
        let toc = TableOfContents::new();
        let mut o = opts(smart);
        o.extensions.push(&toc);
        let out = carve::to_html_with_options(src, &o);
        assert!(out.contains(want), "{smart:?}: {out}");
        assert!(
            out.contains("href=\"#The-quoted-heading\""),
            "the id must not follow the mode: {out}"
        );
    }
}

/// The `::: toc` placement directive collects through a second walk
/// (`collect_all_entries`); it is a separate call site of the same flattener.
#[test]
fn a_placed_toc_entry_keeps_the_source_run() {
    let src = "::: toc\n:::\n\n# The \"quoted\" -- heading\n";

    let placed = TocPlacement::new();
    let mut o = opts(SOURCE);
    o.extensions.push(&placed);
    let out = carve::to_html_with_options(src, &o);
    assert!(out.contains("The &quot;quoted&quot; -- heading"), "{out}");
    assert!(out.contains("href=\"#The-quoted-heading\""), "{out}");
}

// ---------------------------------------------------------------------------
// 5. the numbered cross-reference label
// ---------------------------------------------------------------------------

fn numbered(style: CrossrefStyle, smart: SmartTypographyMode) -> String {
    let ext = HeadingNumbers::with_options(HeadingNumbersOptions {
        crossref: style,
        ..Default::default()
    });
    let mut o = opts(smart);
    o.extensions.push(&ext);
    carve::to_html_with_options(
        "# The \"quoted\" -- heading\n\nSee </#The-quoted-heading>\n",
        &o,
    )
    .trim()
    .to_string()
}

/// The heading obeyed the mode and the reference to it did not, in the same line
/// of output. `headingNumbers` replaces the `heading_ref` before render, so the
/// core cross-reference index (markup-carve/carve-rs#768) never sees it.
#[test]
fn a_numbered_crossref_label_keeps_the_source_run() {
    let out = numbered(CrossrefStyle::NumberTitle, SOURCE);
    assert!(
        out.contains("<h1><span class=\"section-number\">1</span> The \"quoted\" -- heading</h1>"),
        "{out}"
    );
    assert!(
        out.contains(">Section 1 - The \"quoted\" -- heading</a>"),
        "{out}"
    );

    let out = numbered(CrossrefStyle::Title, SOURCE);
    assert!(out.contains(">The \"quoted\" -- heading</a>"), "{out}");
}

#[test]
fn a_numbered_crossref_label_still_resolves_the_glyph_by_default() {
    let out = numbered(CrossrefStyle::NumberTitle, GLYPH);
    assert!(
        out.contains(">Section 1 - The \u{201c}quoted\u{201d} \u{2013} heading</a>"),
        "{out}"
    );
}

/// CONTROL. `crossref: number` renders `{label} {N}` - manufactured text no node
/// of the document ever held, like a caption's LABEL + NUMBER. There is no
/// author run in it, so no mutation of the flatteners can change this line; it
/// is here to bound the claim, not to evidence it.
#[test]
fn control_a_number_only_label_holds_no_author_run() {
    assert_eq!(
        numbered(CrossrefStyle::Number, SOURCE),
        numbered(CrossrefStyle::Number, GLYPH).replace(
            "The \u{201c}quoted\u{201d} \u{2013} heading",
            "The \"quoted\" -- heading"
        ),
        "only the HEADING may differ between the two modes"
    );
    assert!(numbered(CrossrefStyle::Number, SOURCE).contains(">Section 1</a>"));
}

/// The heading id is an identifier and must be byte-identical in both modes,
/// including the id `headingNumbers` computes for itself to key the rewrite.
#[test]
fn a_numbered_heading_id_does_not_follow_the_mode() {
    for smart in [SOURCE, GLYPH] {
        let out = numbered(CrossrefStyle::NumberTitle, smart);
        assert!(
            out.contains("<section id=\"The-quoted-heading\">"),
            "{smart:?}: {out}"
        );
        assert!(
            out.contains("href=\"#The-quoted-heading\""),
            "{smart:?}: {out}"
        );
    }
}
