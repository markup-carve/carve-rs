//! DERIVED DISPLAY TEXT CLONES THE SAME NODES, past the cross-reference label
//! (PART 9R R4, markup-carve/carve#957; carve-rs#782).
//!
//! The core crossref landed with carve-rs#768 and the numbered label with
//! carve-rs#791. The clause binds EVERY consumer that derives display text from
//! a heading, and it names three; the two that were still building a string are
//! a table-of-contents entry (the injected nav and the `::: toc` placement
//! directive alike) and an index term's display.
//!
//! THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE INJECTION is the half aimed at
//! this engine. It derives display text at RENDER time, after every
//! `before_render` hook has run, so the `section-number` span and the permalink
//! anchor are already in the heading when the clone is taken. The pristine
//! reading has to be recovered rather than obtained by ordering, which is what
//! `strip_non_authored` does - and what the tests below hold it to.

use carve::{
    Citations, CrossrefStyle, HeadingNumbers, HeadingNumbersOptions, HeadingPermalinks, Index,
    Options, SmartTypographyMode, TableOfContents, TableOfContentsOptions, TocPlacement,
};

/// The heading every case below derives from: an emphasis, a code span, and an
/// escape. Flattening destroys all three, and a renderer downstream cannot
/// recover any of them.
const MARKUP: &str = "# A *bold* `c` \\* h\n";

fn toc_html(src: &str, exts: &[&dyn carve::CarveExtension]) -> String {
    let mut o = Options::new();
    for e in exts {
        o.extensions.push(*e);
    }
    carve::to_html_with_options(src, &o)
}

/// The `<li>` line of a generated nav, so an assertion talks about the entry and
/// not about the heading it was derived from.
fn entry_line(html: &str) -> String {
    html.lines()
        .find(|l| l.contains("<li><a href="))
        .unwrap_or_else(|| panic!("no TOC entry in {html}"))
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// A table-of-contents entry is nodes.
// ---------------------------------------------------------------------------

#[test]
fn an_injected_nav_entry_carries_the_headings_markup() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    assert_eq!(
        entry_line(&toc_html(MARKUP, &[&toc])),
        "<li><a href=\"#A-bold-c-h\">A <strong>bold</strong> <code>c</code> * h</a></li>"
    );
}

/// The placement directive is the same derivation at a different site, so it is
/// asserted separately rather than assumed to follow.
#[test]
fn a_toc_placement_entry_carries_the_headings_markup() {
    let toc = TocPlacement::new();
    let src = format!("::: toc\n:::\n\n{MARKUP}");
    assert_eq!(
        entry_line(&toc_html(&src, &[&toc])),
        "<li><a href=\"#A-bold-c-h\">A <strong>bold</strong> <code>c</code> * h</a></li>"
    );
}

/// An index term's display is the third consumer the clause names.
#[test]
fn an_index_term_display_carries_the_terms_markup() {
    let index = Index::new();
    let out = toc_html("p :index[*bold* `c`] q\n\n::: index\n:::\n", &[&index]);
    assert!(
        out.contains("<li><strong>bold</strong> <code>c</code> <a href=\"#idx-bold-c-1\""),
        "{out}"
    );
}

/// `inside_link` is the CALLER's context, not a property of being derived: an
/// index list item is not an anchor - only the backrefs after the display are -
/// so an authored link in the term survives where a TOC entry would unwrap it.
#[test]
fn an_index_term_keeps_an_authored_link_because_the_item_is_not_an_anchor() {
    let index = Index::new();
    let out = toc_html(
        "p :index[see <https://e/>] q\n\n::: index\n:::\n",
        &[&index],
    );
    assert!(
        out.contains("<li>see <a href=\"https://e/\">https://e/</a> <a href=\"#idx-"),
        "{out}"
    );
}

// ---------------------------------------------------------------------------
// THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE INJECTION.
// ---------------------------------------------------------------------------

/// The `section-number` span is in the heading by the time a TOC entry is
/// derived here, and it is not part of the label.
#[test]
fn a_toc_entry_never_carries_the_injected_section_number() {
    let numbers = HeadingNumbers::new();
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let out = toc_html("# H\n", &[&numbers, &toc]);
    assert!(
        out.contains("<h1><span class=\"section-number\">1</span> H</h1>"),
        "the heading was not numbered at all: {out}"
    );
    assert_eq!(entry_line(&out), "<li><a href=\"#H\">H</a></li>");
}

/// The permalink anchor is the other injection R4 names. Left in, the entry
/// published an `<a>` inside its own `<a>` - invalid HTML and a second copy of
/// the target's permalink.
#[test]
fn a_toc_entry_never_carries_the_injected_permalink_anchor() {
    let links = HeadingPermalinks::new();
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let out = toc_html("# H\n", &[&links, &toc]);
    assert!(
        out.contains("<h1>H <a href=\"#H\" class=\"permalink\""),
        "the heading got no permalink at all: {out}"
    );
    assert_eq!(entry_line(&out), "<li><a href=\"#H\">H</a></li>");
}

/// The CORE cross-reference is derived at render time too, so it carried the
/// permalink anchor: a resolved `</#id>` published an `<a>` inside the `<a>` it
/// was opening. Nothing about it is specific to the table of contents, which is
/// why the strip lives in the shared derivation rather than in the extension.
#[test]
fn a_resolved_crossref_never_carries_the_injected_permalink_anchor() {
    let links = HeadingPermalinks::new();
    let out = toc_html("# H\n\nSee </#H>\n", &[&links]);
    assert!(out.contains("<p>See <a href=\"#H\">H</a></p>"), "{out}");
    assert!(
        !out.contains("<a href=\"#H\">H <a "),
        "the label nested the permalink anchor: {out}"
    );
}

/// CONTROL, and the reason the strip keys on the injected node rather than on
/// the class it carries: `[v1]{.section-number}` is valid source. An author's
/// own span is authored content and stays in every derived label; a class-keyed
/// strip would delete it.
#[test]
fn control_an_authored_section_number_span_survives_the_derivation() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let out = toc_html("# [v1]{.section-number} API\n", &[&toc]);
    assert_eq!(
        entry_line(&out),
        "<li><a href=\"#API\"><span class=\"section-number\">v1</span> API</a></li>"
    );
}

// ---------------------------------------------------------------------------
// What a resolution stage added is not the label either.
// ---------------------------------------------------------------------------

/// A citation renders as an anchor into the references list, and with a
/// bibliography pool active it also carries a per-use `cite-…` id. A second copy
/// in a derived label nests an anchor and publishes a duplicate DOM id, so the
/// author's raw `[@key]` run goes back in its place - which is what the flatten
/// this replaces produced.
#[test]
fn a_citation_in_a_heading_is_not_republished_in_a_derived_label() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let citations = Citations::default();
    let src = "# Research [@doe]\n\nSee </#Research-doe>\n\n[@doe]: John Doe, 2020\n";
    let out = toc_html(src, &[&citations, &toc]);
    assert_eq!(
        entry_line(&out),
        "<li><a href=\"#Research-doe\">Research [@doe]</a></li>"
    );
    assert!(
        out.contains("<p>See <a href=\"#Research-doe\">Research [@doe]</a></p>"),
        "{out}"
    );
    assert_eq!(
        out.matches("href=\"#ref-doe\"").count(),
        1,
        "the citation anchor was published more than once: {out}"
    );
}

/// An abbreviation is an R3 resolution result. Republishing it would emit the
/// whole `<abbr title="…">` once per derived site, an amplification the body
/// renderer bounds with a budget this path cannot reach - so the author's short
/// form goes back, again exactly what the flatten produced.
#[test]
fn an_abbreviation_in_a_heading_contributes_its_short_form() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let src = "*[HTML]: HyperText Markup Language\n\n# The HTML h\n\nSee </#The-HTML-h>\n";
    let out = toc_html(src, &[&toc]);
    assert_eq!(
        entry_line(&out),
        "<li><a href=\"#The-HTML-h\">The HTML h</a></li>"
    );
    assert_eq!(
        out.matches("<abbr title=").count(),
        1,
        "the expansion was republished: {out}"
    );
}

/// An `:index[term]` marker the extension has COUNTED is invisible (PART 9
/// §8.1): it emits no visible text anywhere, and its `idx-…` anchor id is
/// published exactly once. Left in, a derived label rendered a SECOND element
/// carrying that id.
#[test]
fn an_index_marker_in_a_heading_contributes_nothing_to_a_derived_label() {
    let index = Index::new();
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let src = "# The :index[t] h\n\nSee </#The-t-h>\n\n::: index\n:::\n";
    let out = toc_html(src, &[&index, &toc]);
    assert_eq!(entry_line(&out), "<li><a href=\"#The-t-h\">The  h</a></li>");
    assert_eq!(
        out.matches("id=\"idx-t-1\"").count(),
        1,
        "the marker's anchor id was published twice: {out}"
    );
}

/// The strip is the COUNTED CARRIER, not the authored `index` node. With the
/// extension off the marker degrades to the visible generic fallback (PART 9
/// §8.3, "the marker cannot hide without its handler"), so a derived label that
/// dropped it would disagree with the heading it came from and lose authored
/// text (raised by codex review, and a regression against `main` had this
/// keyed on the node's NAME).
#[test]
fn control_an_index_marker_stays_visible_where_the_extension_is_off() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let out = toc_html("# The :index[t] h\n\nSee </#The-t-h>\n", &[&toc]);
    assert!(
        out.contains("<h1>The <span class=\"ext-index\">t</span> h</h1>"),
        "the fallback is not what the heading rendered: {out}"
    );
    assert_eq!(
        entry_line(&out),
        "<li><a href=\"#The-t-h\">The <span class=\"ext-index\">t</span> h</a></li>"
    );
    assert!(
        out.contains(
            "<p>See <a href=\"#The-t-h\">The <span class=\"ext-index\">t</span> h</a></p>"
        ),
        "{out}"
    );
}

/// LINKS NEVER NEST (PART 12 section 3a) reaches every construct that opens its
/// own anchor, not links and autolinks alone. A mention and a tag do so once a
/// URL template is configured, so inside a derived label - which the caller
/// writes into an `<a>` - they render their template-less form.
#[test]
fn a_mention_and_a_tag_in_a_derived_label_open_no_anchor() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let mut o = Options::new();
    o.extensions.push(&toc);
    let o = o
        .with_mention_url("https://e/{name}")
        .with_tag_url("https://e/t/{name}");
    let out = carve::to_html_with_options("# A @bob #t h\n\nSee </#A-bob-t-h>\n", &o);
    assert_eq!(
        entry_line(&out),
        "<li><a href=\"#A-bob-t-h\">A <span class=\"mention\"><strong>@bob</strong></span> \
         <span class=\"tag\"><strong>#t</strong></span> h</a></li>"
    );
    assert!(
        out.contains(
            "<p>See <a href=\"#A-bob-t-h\">A <span class=\"mention\"><strong>@bob</strong></span> \
             <span class=\"tag\"><strong>#t</strong></span> h</a></p>"
        ),
        "{out}"
    );
    // The heading itself is not inside an anchor, so it keeps both links.
    assert!(
        out.contains("<h1>A <a class=\"mention\" href=\"https://e/bob\">@bob</a>"),
        "{out}"
    );
}

// ---------------------------------------------------------------------------
// The entry is rendered by the RENDER the caller asked for.
// ---------------------------------------------------------------------------

/// Deriving a string settled the raw-HTML policy in a pre-render pass. The nodes
/// are handed to the renderer instead, so `with_raw_html(false)` escapes a
/// heading's raw inline HTML in the entry exactly as it does in the heading.
#[test]
fn a_toc_entry_obeys_the_raw_html_policy() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let src = "# A =html`<b>x</b>`= h\n";
    let mut safe = Options::new();
    safe.extensions.push(&toc);
    let safe = safe.with_raw_html(false);
    let out = carve::to_html_with_options(src, &safe);
    assert!(
        !out.contains("<b>x</b>"),
        "raw HTML reached the output under with_raw_html(false): {out}"
    );
}

/// Smart typography is DOCUMENT-GLOBAL and applies to every target (PART 9 §19).
/// The entry used to be spelled at derivation time; the nodes carry the author's
/// run and the renderer spells it, so both modes still answer - CONTROL for the
/// half of carve-rs#773 that must not regress.
#[test]
fn control_a_toc_entry_still_follows_the_smart_typography_mode() {
    let src = "# The \"q\" -- h\n";
    for (mode, expected) in [
        (SmartTypographyMode::Source, ">The \"q\" -- h<"),
        (
            SmartTypographyMode::Glyph,
            ">The \u{201c}q\u{201d} \u{2013} h<",
        ),
    ] {
        let toc = TableOfContents::with_options(TableOfContentsOptions::default());
        let mut o = Options::new();
        o.smart_typography = mode;
        o.extensions.push(&toc);
        let out = carve::to_html_with_options(src, &o);
        assert!(entry_line(&out).contains(expected), "{mode:?}: {out}");
    }
}

/// §26: a bidi override in a heading must not reach a TOC link, where it could
/// visually spoof the target. The strip moved from the flattened string to the
/// rendered bytes, and the controls are bare codepoints that never form part of
/// a tag, so it still reaches every one of them.
#[test]
fn a_toc_entry_still_strips_bidi_controls() {
    let toc = TableOfContents::with_options(TableOfContentsOptions::default());
    let out = toc_html("# A\u{202e}B h\n", &[&toc]);
    assert!(
        !entry_line(&out).contains('\u{202e}'),
        "a bidi override reached the entry: {out}"
    );
}

/// CONTROL: the id a derived entry links to is NOT derived display text. It
/// keeps flattening in glyph mode, because an identifier may not depend on
/// presentational typography and PART 9 §19 pins heading ids byte-identical in
/// both modes. No change to the clone may move it.
#[test]
fn control_the_entry_links_to_the_id_the_core_emits() {
    for mode in [SmartTypographyMode::Source, SmartTypographyMode::Glyph] {
        let toc = TableOfContents::with_options(TableOfContentsOptions::default());
        let mut o = Options::new();
        o.smart_typography = mode;
        o.extensions.push(&toc);
        let out = carve::to_html_with_options("# The \"q\" -- h\n", &o);
        assert!(out.contains("<section id=\"The-q-h\">"), "{mode:?}: {out}");
        assert!(
            entry_line(&out).starts_with("<li><a href=\"#The-q-h\">"),
            "{mode:?}: {out}"
        );
    }
}

/// CONTROL: `crossref: number` is manufactured text no node of the document ever
/// held, so R4 does not reach it and no change to the derivation moves it.
#[test]
fn control_a_number_only_crossref_label_is_unmoved() {
    let ext = HeadingNumbers::with_options(HeadingNumbersOptions {
        crossref: CrossrefStyle::Number,
        ..Default::default()
    });
    let out = toc_html("# A *b* h\n\nSee </#A-b-h>\n", &[&ext]);
    assert!(
        out.contains("<p>See <a href=\"#A-b-h\">Section 1</a></p>"),
        "{out}"
    );
}
