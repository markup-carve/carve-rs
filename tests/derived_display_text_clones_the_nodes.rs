//! DERIVED DISPLAY TEXT CLONES THE SAME NODES (PART 9R R4,
//! markup-carve/carve#957).
//!
//! R4 binds every consumer that derives display text from a heading, not the
//! crossref alone: a render-stage transform may not undo a core resolution rule.
//! The CORE half landed with carve-rs#768 - a resolved `heading_ref` renders the
//! target heading's cloned nodes. `headingNumbers` then REPLACES that node before
//! render, so the core index never sees it, and its own label was a flattened
//! string: the source run, the emphasis, the code span and the escape were all
//! destroyed at the derivation site, where no renderer downstream can recover
//! them.
//!
//! THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE INJECTION is the half aimed at this
//! engine specifically. Cross-references resolve at RENDER time here, AFTER
//! `headingNumbers` has injected its `section-number` span into the heading, so
//! the clone has to come from the PRISTINE heading and not from the live one.
//! Two implementations may differ HERE BY CONSTRUCTION rather than by defect,
//! which is why naming the side is the whole content of that half.

use carve::{CrossrefStyle, HeadingNumbers, HeadingNumbersOptions, Options, SmartTypographyMode};

const MARKUP: &str = "# A *bold* `c` h\n\nSee </#A-bold-c-h>\n";

fn numbered(src: &str, style: CrossrefStyle, smart: SmartTypographyMode) -> String {
    let ext = HeadingNumbers::with_options(HeadingNumbersOptions {
        crossref: style,
        ..Default::default()
    });
    let mut o = Options::new();
    o.smart_typography = smart;
    o.extensions.push(&ext);
    carve::to_html_with_options(src, &o)
}

fn numbered_md(src: &str, style: CrossrefStyle) -> String {
    let ext = HeadingNumbers::with_options(HeadingNumbersOptions {
        crossref: style,
        ..Default::default()
    });
    let mut o = Options::new();
    o.extensions.push(&ext);
    carve::to_markdown_with_options(src, &o)
}

/// The reference line only, so an assertion talks about the label and not about
/// the heading it was taken from.
fn reference_line(html: &str) -> String {
    html.lines()
        .find(|l| l.contains("<p>See "))
        .unwrap_or_else(|| panic!("no reference line in {html}"))
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// The label is made of nodes.
// ---------------------------------------------------------------------------

/// `{label} {number} - {TITLE}`: the numbering, the prefix and the separator are
/// the extension's own business, and the TITLE part is the heading's nodes.
#[test]
fn a_numbered_crossref_label_carries_the_headings_markup() {
    assert_eq!(
        reference_line(&numbered(
            MARKUP,
            CrossrefStyle::NumberTitle,
            SmartTypographyMode::Glyph
        )),
        "<p>See <a href=\"#A-bold-c-h\">Section 1 - A <strong>bold</strong> <code>c</code> h</a></p>"
    );
}

/// The title-only style is the same clone with no manufactured text in front.
#[test]
fn the_title_style_is_the_clone_on_its_own() {
    assert_eq!(
        reference_line(&numbered(
            MARKUP,
            CrossrefStyle::Title,
            SmartTypographyMode::Glyph
        )),
        "<p>See <a href=\"#A-bold-c-h\">A <strong>bold</strong> <code>c</code> h</a></p>"
    );
}

/// A second target proves the nodes travel rather than the HTML: Markdown gets
/// the same markup in its own spelling, which a flattened string could not have
/// produced on any target.
#[test]
fn the_label_reaches_markdown_as_markup_too() {
    assert!(
        numbered_md(MARKUP, CrossrefStyle::NumberTitle)
            .contains("See [Section 1 - A **bold** `c` h](#A-bold-c-h)"),
        "{}",
        numbered_md(MARKUP, CrossrefStyle::NumberTitle)
    );
}

/// An escape is a node of its own carrying the AUTHORED form, so it survives the
/// clone where flattening spelled it as its resolved character.
#[test]
fn the_label_keeps_an_escaped_character() {
    let out = numbered(
        "# A \\* h\n\nSee </#A-h>\n",
        CrossrefStyle::Title,
        SmartTypographyMode::Glyph,
    );
    assert!(out.contains(">A * h</a>"), "{out}");
}

// ---------------------------------------------------------------------------
// The side of the injection the label comes from.
// ---------------------------------------------------------------------------

/// THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE INJECTION. The heading carries a
/// `section-number` span this very pass added; the label carries the number ONCE,
/// from the extension's own prefix, and never the span. Taking the clone from the
/// live heading would put `1` in twice and a `<span>` inside the anchor.
#[test]
fn the_label_never_carries_the_injected_section_number() {
    let out = numbered(
        "# H\n\nSee </#H>\n",
        CrossrefStyle::NumberTitle,
        SmartTypographyMode::Glyph,
    );
    assert!(
        out.contains("<h1><span class=\"section-number\">1</span> H</h1>"),
        "{out}"
    );
    assert_eq!(
        reference_line(&out),
        "<p>See <a href=\"#H\">Section 1 - H</a></p>"
    );
    assert!(
        !reference_line(&out).contains("section-number"),
        "the injected span reached the label: {out}"
    );
}

/// The same statement with the title style, where the number is not part of the
/// label at all - so a leak would show as a bare `1` rather than as a repeat.
#[test]
fn the_title_style_label_holds_no_number_at_all() {
    assert_eq!(
        reference_line(&numbered(
            "# H\n\nSee </#H>\n",
            CrossrefStyle::Title,
            SmartTypographyMode::Glyph
        )),
        "<p>See <a href=\"#H\">H</a></p>"
    );
}

// ---------------------------------------------------------------------------
// The mode question moves back to the renderer, which is what cloning buys.
// ---------------------------------------------------------------------------

/// Smart typography is DOCUMENT-GLOBAL and applies to every target (PART 9 §19).
/// The label used to be spelled in the mode at derivation time; it is nodes now,
/// so each renderer spells them in the mode it was given.
#[test]
fn the_label_follows_the_smart_typography_mode_on_both_settings() {
    let src = "# The \"q\" -- h\n\nSee </#The-q-h>\n";
    assert!(
        reference_line(&numbered(
            src,
            CrossrefStyle::Title,
            SmartTypographyMode::Source
        ))
        .contains(">The \"q\" -- h</a>"),
        "{}",
        numbered(src, CrossrefStyle::Title, SmartTypographyMode::Source)
    );
    assert!(
        reference_line(&numbered(
            src,
            CrossrefStyle::Title,
            SmartTypographyMode::Glyph
        ))
        .contains(">The \u{201c}q\u{201d} \u{2013} h</a>"),
        "{}",
        numbered(src, CrossrefStyle::Title, SmartTypographyMode::Glyph)
    );
}

/// The heading id does not follow the mode: an identifier may not depend on
/// presentational typography, and it is byte-identical either way (PART 9 §19).
#[test]
fn control_the_heading_id_does_not_follow_the_mode() {
    let src = "# The \"q\" -- h\n\nSee </#The-q-h>\n";
    for smart in [SmartTypographyMode::Source, SmartTypographyMode::Glyph] {
        let out = numbered(src, CrossrefStyle::Title, smart);
        assert!(out.contains("<section id=\"The-q-h\">"), "{smart:?}: {out}");
        assert!(out.contains("href=\"#The-q-h\""), "{smart:?}: {out}");
    }
}

// ---------------------------------------------------------------------------
// Controls that bound the rule.
// ---------------------------------------------------------------------------

/// `crossref: number` renders `{label} {N}` - manufactured text no node of the
/// document ever held, like a caption's LABEL + NUMBER. R4 does not reach it, so
/// no change to the clone can move this line. It is here to bound the claim.
#[test]
fn control_a_number_only_label_holds_no_author_run() {
    assert_eq!(
        reference_line(&numbered(
            MARKUP,
            CrossrefStyle::Number,
            SmartTypographyMode::Glyph
        )),
        "<p>See <a href=\"#A-bold-c-h\">Section 1</a></p>"
    );
}

/// A crossref to an UNNUMBERED heading is left to the core resolution, which
/// already clones (carve-rs#768). Asserted so a fix keyed on "this extension is
/// active" rather than on "this heading is numbered" would show.
#[test]
fn control_an_unnumbered_target_still_resolves_through_the_core() {
    let ext = HeadingNumbers::with_options(HeadingNumbersOptions {
        min_level: 2,
        ..Default::default()
    });
    let mut o = Options::new();
    o.extensions.push(&ext);
    let out = carve::to_html_with_options("# A *b* h\n\nSee </#A-b-h>\n", &o);
    assert!(out.contains(">A <strong>b</strong> h</a>"), "{out}");
}

/// A reference written INSIDE a link's label still renders unwrapped, and the
/// cloned markup travels with it. Since PART 12 section 3a the inner node stays a
/// node and the render seam unwraps it, so this path no longer has to flatten the
/// label to avoid nesting an anchor.
#[test]
fn control_a_reference_inside_a_link_label_keeps_the_markup() {
    let ext = HeadingNumbers::new();
    let mut o = Options::new();
    o.extensions.push(&ext);
    let out = carve::to_html_with_options("# H *b*\n\n[see </#H-b> x](/u)\n", &o);
    assert!(
        out.contains("<a href=\"/u\">see Section 1 - H <strong>b</strong> x</a>"),
        "{out}"
    );
}

/// THE CLONE IS THE SAME CLONE, transformations included. A footnote reference
/// in the heading is DROPPED from the label: the label renders inside the
/// referring paragraph, so a second copy of the `fnref` anchor would publish a
/// duplicate id and put an anchor inside an anchor. Cloning the children raw
/// produced exactly that - a second endnote and a nested `<a>`.
#[test]
fn a_footnote_reference_in_the_heading_is_dropped_from_the_label() {
    let ext = HeadingNumbers::new();
    let mut o = Options::new();
    o.extensions.push(&ext);
    let out = carve::to_html_with_options("# H^[note]\n\nSee </#H>\n", &o);
    assert!(
        out.contains("<p>See <a href=\"#H\">Section 1 - H</a></p>"),
        "{out}"
    );
    assert!(
        !out.contains("fnref2"),
        "the label published a second fnref: {out}"
    );
}

/// Resolution is ONE LEVEL: a nested cross-reference in the heading becomes
/// empty text in the label rather than being expanded again. Expanding it here
/// would also drag the `section-number` span of the INNER heading into the outer
/// label, and it makes a crossref cycle structurally impossible to follow.
#[test]
fn a_nested_crossref_in_the_heading_is_not_re_expanded() {
    let ext = HeadingNumbers::new();
    let mut o = Options::new();
    o.extensions.push(&ext);
    let out = carve::to_html_with_options("# A\n\n# H </#A>\n\nSee </#H>\n", &o);
    assert!(
        out.contains("<p>See <a href=\"#H\">Section 2 - H </a></p>"),
        "{out}"
    );
    assert!(
        !out.contains("<a href=\"#H\">Section 2 - H <a"),
        "the label nested an anchor: {out}"
    );
}

/// PART 12 section 1a: no node's children hold two adjacent `text` nodes. The
/// TITLE part of the label very often begins with one, so the manufactured
/// prefix joins it instead of sitting beside it. A split there is bookkeeping
/// rather than the document, and it would publish two runs where a consumer must
/// see one.
#[test]
fn the_manufactured_prefix_joins_the_titles_first_run() {
    use carve::ast::{BlockNode, InlineNode};
    let ext = HeadingNumbers::new();
    let mut o = Options::new();
    o.extensions.push(&ext);
    let src = "# A *b*\n\nSee </#A-b>\n";
    let doc = carve::prepare_document_for_render(
        carve::parse_with_options(src, &o),
        &o,
        carve::Mode::Interactive,
        true,
    )
    .expect("no profile violation");
    let BlockNode::Paragraph(p) = &doc.children[1] else {
        panic!("expected a paragraph, got {:?}", doc.children[1]);
    };
    let InlineNode::Link(l) = &p.children[1] else {
        panic!("expected the rewritten link, got {:?}", p.children[1]);
    };
    let kinds: Vec<&str> = l
        .children
        .iter()
        .map(|n| match n {
            InlineNode::Text(_) => "text",
            InlineNode::Emphasis(_) => "emphasis",
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(kinds, vec!["text", "emphasis"]);
    match &l.children[0] {
        InlineNode::Text(t) => assert_eq!(t.value, "Section 1 - A "),
        other => panic!("{other:?}"),
    }
}
