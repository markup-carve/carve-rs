//! A resolved cross-reference renders the target heading's inline NODES,
//! cloned - not the heading flattened to a string (PART 9R R4, carve#915).
//!
//! The distinction is invisible until a renderer asks for something only a node
//! still has. A node carries the SOURCE RUN the author typed; a string carries
//! whichever glyphs some earlier pass chose. This engine built its cross-
//! reference index by flattening every heading to text (`crossref_index_for_
//! document`), which destroyed the run before any renderer was invoked - so
//! smart typography's SOURCE mode could not recover it on ANY target, and no
//! renderer change could have reached the loss (carve-rs#767).
//!
//! Typography is the measurable face of it, and the same flattening lost every
//! other run a renderer may want back: a code span, an emphasis, an escape.
//! Those are asserted here too, because a fix that only re-derived the source
//! SPELLING would leave the label a string and leave them broken.

use carve::{Options, SmartTypographyMode};

/// The document the four optional-corpus cases in carve#952 are built on
/// (`36`..`39-crossref-label-typography-*`).
const TYPOGRAPHY: &str = "# The \"quoted\" -- heading\n\nSee </#The-quoted-heading>\n";

fn options(mode: SmartTypographyMode) -> Options<'static> {
    Options {
        smart_typography: mode,
        ..Options::default()
    }
}

/// Drop the styling runs so an assertion can talk about the text.
fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        for c in chars.by_ref() {
            if c == 'm' {
                break;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The source run reaches every target.
// ---------------------------------------------------------------------------

/// Corpus `36-crossref-label-typography-source`.
#[test]
fn plain_source_mode_gives_the_crossref_the_source_run() {
    let out = carve::to_plain_text_with_options(TYPOGRAPHY, &options(SmartTypographyMode::Source));
    assert_eq!(
        out.trim(),
        "The \"quoted\" -- heading\n\nSee The \"quoted\" -- heading"
    );
}

/// Corpus `37-crossref-label-typography-source-markdown`.
#[test]
fn markdown_source_mode_gives_the_crossref_the_source_run() {
    let out = carve::to_markdown_with_options(TYPOGRAPHY, &options(SmartTypographyMode::Source));
    assert_eq!(
        out.trim(),
        "# The \"quoted\" -- heading {#The-quoted-heading}\n\n\
         See [The \"quoted\" -- heading](#The-quoted-heading)"
    );
}

/// Corpus `38-crossref-label-typography-source-ansi`. The rule under the
/// heading is a COLUMN count of the rendered heading, so its width moves with
/// the mode: 23 for the source spelling against 22 for the glyphs.
#[test]
fn ansi_source_mode_gives_the_crossref_the_source_run() {
    let out = carve::to_ansi_with_options(TYPOGRAPHY, &options(SmartTypographyMode::Source));
    assert_eq!(
        strip_ansi(&out).trim(),
        "The \"quoted\" -- heading\n\
         \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\
         \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\
         \u{2550}\n\n\
         See The \"quoted\" -- heading"
    );
}

#[test]
fn html_source_mode_gives_the_crossref_the_source_run() {
    let out = carve::to_html_with_options(TYPOGRAPHY, &options(SmartTypographyMode::Source));
    assert!(
        out.contains("<a href=\"#The-quoted-heading\">The \"quoted\" -- heading</a>"),
        "{out}"
    );
}

/// Corpus `39-crossref-label-typography-glyphs`. THE CONTROL, and load-bearing
/// rather than decoration: without it the four cases above also pass an engine
/// that never applies typography to a cross-reference label in EITHER mode,
/// which is the neighbouring way to be wrong.
#[test]
fn default_mode_gives_the_crossref_the_glyphs() {
    let out = carve::to_plain_text_with_options(TYPOGRAPHY, &options(SmartTypographyMode::Glyph));
    assert_eq!(
        out.trim(),
        "The \u{201c}quoted\u{201d} \u{2013} heading\n\nSee The \u{201c}quoted\u{201d} \u{2013} heading"
    );
    assert_eq!(
        carve::to_plain_text(TYPOGRAPHY).trim(),
        out.trim(),
        "glyph is the default"
    );
}

// ---------------------------------------------------------------------------
// The run is not the only thing a string had thrown away.
// ---------------------------------------------------------------------------

#[test]
fn the_label_keeps_the_headings_markup() {
    let input = "# A *bold* heading\n\nSee </#A-bold-heading>\n";
    assert!(
        carve::to_html(input)
            .contains("<a href=\"#A-bold-heading\">A <strong>bold</strong> heading</a>"),
        "{}",
        carve::to_html(input)
    );
    assert_eq!(
        carve::to_markdown(input).trim(),
        "# A **bold** heading {#A-bold-heading}\n\nSee [A **bold** heading](#A-bold-heading)"
    );
}

#[test]
fn the_label_keeps_a_code_span() {
    let input = "# A `code` heading\n\nSee </#A-code-heading>\n";
    assert!(
        carve::to_html(input)
            .contains("<a href=\"#A-code-heading\">A <code>code</code> heading</a>"),
        "{}",
        carve::to_html(input)
    );
}

/// The flattening did not merely un-escape this one - it dropped the character
/// entirely, on every target and in BOTH modes, because the walk that built the
/// index had no arm for an escaped-text node.
#[test]
fn the_label_keeps_an_escaped_character() {
    let input = "# A \\*star heading\n\nSee </#A-star-heading>\n";
    assert_eq!(
        carve::to_plain_text(input).trim(),
        "A *star heading\n\nSee A *star heading"
    );
    assert_eq!(
        carve::to_markdown(input).trim(),
        "# A \\*star heading {#A-star-heading}\n\nSee [A \\*star heading](#A-star-heading)"
    );
}

// ---------------------------------------------------------------------------
// Cloning nodes must not import the hazards a string could not carry.
// ---------------------------------------------------------------------------

/// Resolution is ONE LEVEL: a cloned label is never re-expanded, so a
/// cross-reference cycle terminates. Both documents are corpus cases
/// (`118-cyclic-cross-reference-resolves-to-one-level`), pinned here against
/// the clone specifically - a naive clone would splice a label into itself.
#[test]
fn a_nested_crossref_in_the_label_is_not_re_expanded() {
    assert_eq!(
        carve::to_html("# A </#a>\n").trim(),
        "<section id=\"A\">\n  <h1>A <a href=\"#A\">A </a></h1>\n</section>"
    );
    assert_eq!(
        carve::to_html("# A </#b>\n\n# B </#a>\n").trim(),
        "<section id=\"A\">\n  <h1>A <a href=\"#B\">B </a></h1>\n</section>\n\
         <section id=\"B\">\n  <h1>B <a href=\"#A\">A </a></h1>\n</section>"
    );
}

/// Links never nest, and the label is placed inside the anchor the reference
/// itself opens - so a link in the heading is unwrapped to its text in the
/// label while the heading keeps it. Corpus `03-links-13` pins the mirror case
/// (a reference inside a link); this pins the heading side.
#[test]
fn a_link_in_the_heading_is_unwrapped_in_the_label() {
    let out = carve::to_html("# A [link](https://x.example) heading\n\nSee </#A-link-heading>\n");
    assert!(
        out.contains("<h1>A <a href=\"https://x.example\">link</a> heading</h1>"),
        "{out}"
    );
    assert!(
        out.contains("<a href=\"#A-link-heading\">A link heading</a>"),
        "{out}"
    );
}

/// A footnote reference is dropped from the label rather than cloned: the
/// label renders inside the referring paragraph, so a second copy of the
/// `fnref` anchor would publish a duplicate id.
#[test]
fn a_footnote_reference_in_the_heading_is_dropped_from_the_label() {
    let out = carve::to_html("# A heading[^n]\n\n[^n]: note\n\nSee </#A-heading>\n");
    assert_eq!(out.matches("id=\"fnref1\"").count(), 1, "{out}");
    assert!(
        out.contains("<a href=\"#A-heading\">A heading</a>"),
        "{out}"
    );
}

/// A CAPTION target has a title but no nodes to clone - its label is
/// LABEL + NUMBER, text no node of the document ever held - so it keeps the
/// string path. Corpus `47-numbered-cross-references-3`.
#[test]
fn a_caption_target_still_renders_label_and_number() {
    let input =
        "{#fig-sun}\n![A sunset](sun.jpg)\n^ Figure #: A *bold* -- sunset\n\nSee </#fig-sun>.\n";
    for mode in [SmartTypographyMode::Glyph, SmartTypographyMode::Source] {
        let out = carve::to_html_with_options(input, &options(mode));
        assert!(out.contains("<a href=\"#fig-sun\">Figure 1</a>"), "{out}");
    }
}
