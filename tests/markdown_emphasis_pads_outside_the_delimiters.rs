//! A delimiter run only OPENS emphasis while it is left-flanking, which a run
//! followed by whitespace never is (CommonMark 6.2), so `** x**` reads back as
//! literal text and the emphasis is lost on the way out. The padding is
//! content, so it moves outside the delimiters instead of being trimmed. At a
//! paragraph edge Markdown collapses it either way; mid-paragraph it survives
//! (carve-js#1683).
//!
//! The rows below read the Markdown BACK with pulldown-cmark, through this
//! crate's own importer, because the defect is never in the bytes: it is what a
//! CommonMark reader makes of them.

#[test]
fn moves_leading_padding_outside_a_strong_run() {
    let out = carve::to_markdown("a{* b*}c\n");
    assert!(out.contains("a **b**c"), "got: {out}");
}

#[test]
fn moves_trailing_padding_outside_a_strong_run() {
    let out = carve::to_markdown("a{*b *}c\n");
    assert!(out.contains("a**b** c"), "got: {out}");
}

#[test]
fn moves_padding_outside_an_italic_run() {
    let out = carve::to_markdown("a{/ i /}b\n");
    assert!(out.contains("a *i* b"), "got: {out}");
}

#[test]
fn moves_padding_outside_a_strike_run() {
    let out = carve::to_markdown("a{~ s ~}b\n");
    assert!(out.contains("a ~~s~~ b"), "got: {out}");
}

#[test]
fn never_emits_a_run_that_cannot_open_emphasis() {
    let out = carve::to_markdown("a{* b*}c\n");
    assert!(!out.contains("** b"), "got: {out}");
}

#[test]
fn falls_back_to_inline_html_when_the_content_is_only_padding() {
    let out = carve::to_markdown("a{* *}b\n");
    assert!(out.contains("a<strong> </strong>b"), "got: {out}");
}

#[test]
fn leaves_an_unpadded_run_exactly_as_it_was() {
    let out = carve::to_markdown("x {*y*} z\n");
    assert!(out.contains("x **y** z"), "got: {out}");
}

/// What a CommonMark reader makes of the Markdown this renderer wrote. The
/// importer is pulldown-cmark, so this is the ecosystem's own reading, not a
/// second opinion from the engine that produced the bytes.
fn readback(source: &str) -> String {
    let md = carve::to_markdown(source);
    carve::render_html(&carve::markdown_to_ast(&md)).expect("the reader renders")
}

// ---------------------------------------------------------------------------
// The five wrappers that spelled the delimiter run at the call site, so none of
// them got the repair above.
// ---------------------------------------------------------------------------

#[test]
fn a_whitespace_only_admonition_title_writes_no_thematic_break() {
    // `** **` is four `*` separated by a space, which CommonMark reads as a
    // THEMATIC BREAK: the title vanishes and a rule the author never wrote
    // stands in the document.
    let back = readback("::: note \" \"\nbody\n:::\n");
    assert!(!back.contains("<hr"), "a fabricated rule: {back}");
    assert!(back.contains("<strong> </strong>"), "got: {back}");
}

#[test]
fn a_padded_admonition_title_keeps_its_strong() {
    let back = readback("::: note \" x\"\nbody\n:::\n");
    assert!(back.contains("<strong>x</strong>"), "got: {back}");
    assert!(!back.contains("** x**"), "got: {back}");
}

#[test]
fn a_padded_definition_term_keeps_its_strong() {
    // Plain padded text, NOT an authored `{* *}` run: the term's own `**`
    // wrapper is the site under test, and an authored run would reach the
    // inline arm instead and pass whatever the wrapper did.
    let back = readback(":: \\ term\n: body\n");
    assert!(back.contains("<strong>term</strong>"), "got: {back}");
}

#[test]
fn a_padded_figure_panel_caption_keeps_its_emphasis() {
    let back = readback("::: figure\n![one](a.png)\n^ \\ cap\n:::\n");
    assert!(back.contains("<em>cap</em>"), "got: {back}");
}

#[test]
fn a_padded_figure_group_caption_keeps_its_strong() {
    let back = readback("::: figure\n![one](a.png)\n^ (a) One\n:::\n^ \\ G\n");
    assert!(back.contains("<strong>G</strong>"), "got: {back}");
}

#[test]
fn a_padded_container_label_keeps_its_strong() {
    let back = readback("::: note [ L]\nbody\n:::\n");
    assert!(back.contains("<strong>L</strong>"), "got: {back}");
}

// ---------------------------------------------------------------------------
// Consequences of moving the padding.
// ---------------------------------------------------------------------------

#[test]
fn a_hard_break_at_the_trailing_edge_does_not_escape_the_closing_delimiter() {
    // A hard break is a backslash then a newline. Moving the newline out on
    // its own leaves the backslash escaping the `*` behind it, so the run
    // closes nowhere and the strong is lost along with the break.
    let back = readback("a{*x\\\n*}b\n");
    assert!(back.contains("<strong>x<br>"), "got: {back}");
    assert!(
        !back.contains("a**x"),
        "the closing run was escaped: {back}"
    );
}

#[test]
fn the_padding_class_covers_what_the_reader_counts_as_whitespace() {
    // U+2028 is NOT CommonMark whitespace but IS Rust `White_Space`, and the
    // reader's flanking test counts the wider class. Narrowing this class to
    // CommonMark 2.1 leaves the run stranded against a character the reader
    // will not let it open across.
    let back = readback("a{*\u{2028}x*}b\n");
    assert!(back.contains("<strong>x</strong>"), "got: {back}");
}
