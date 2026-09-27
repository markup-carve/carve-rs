//! A span inside a heading contributes its text to the heading's id and to the
//! Markdown target's GFM slug (carve-rs#2038).
//!
//! Neither `plain_inlines_typography_at` nor `plain_inlines_parse` had an arm for
//! `InlineNode::Span`, so the trailing catch-all swallowed the span and its
//! children: `## [Flavored]{#g} text` published `id="text"`, and a heading whose
//! only child was such a span published the empty fallback. PART 11 §11 G1 takes
//! the heading's whole text content, and carve-js includes the span's text.
//!
//! The cross-reference case is the one that proves the two functions agree: the
//! parse-time index builds the reference, the renderer builds the id, and a
//! heading whose id disagreed between them publishes an anchor nothing resolves.
//!
//! Only an AUTHORED span contributes. PART 9R R4 takes a derived text before any
//! render-stage injection, so the `section-number` span the heading-numbers
//! extension prepends stays out; `derived_display_text_clones_the_nodes` pins
//! that side.

fn options() -> carve::Options<'static> {
    carve::Options::default().with_lowercase_heading_ids(true)
}

/// Both attribute kinds, because the defect is the span rather than the id it
/// happens to carry.
const ATTRS: [&str; 2] = ["#g", ".flavor"];

#[test]
fn a_heading_id_takes_the_span_s_text() {
    for attrs in ATTRS {
        let source = format!("## [Flavored]{{{attrs}}} text\n");
        let html = carve::to_html_with_options(&source, &options());
        assert!(html.contains("<section id=\"flavored-text\">"), "{html}");
    }
}

#[test]
fn a_crossref_resolves_against_the_id_the_span_fed() {
    for attrs in ATTRS {
        let source = format!("## [Flavored]{{{attrs}}} text\n\n</#flavored-text>\n");
        let html = carve::to_html_with_options(&source, &options());
        assert!(html.contains("<section id=\"flavored-text\">"), "{html}");
        assert!(html.contains("<a href=\"#flavored-text\">"), "{html}");
    }
}

#[test]
fn the_markdown_target_writes_the_slug_the_span_fed() {
    for attrs in ATTRS {
        let source = format!("{{#h}}\n## [Flavored]{{{attrs}}} text\n\n[go](#h)\n");
        let markdown = carve::to_markdown(&source);
        assert!(markdown.contains("[go](#flavored-text)"), "{markdown}");
    }
}

/// A heading whose only child is such a span slugged to the empty fallback, which
/// is how the botmonster page linked a section as `#-1`.
#[test]
fn a_heading_that_is_only_a_span_still_slugs() {
    let html = carve::to_html_with_options("## [Flavored]{#g}\n", &options());
    assert!(html.contains("<section id=\"flavored\">"), "{html}");
}
