//! Markdown import gives up the nesting, not the document
//! (markup-carve/carve-rs#2098).
//!
//! An opener is text while a span of its kind is open (PART 9 §9 E3) and the
//! forced form shares the stack (markup-carve/carve#2078), so `*(*foo*)*` has no
//! Carve spelling. Handing that refusal to the caller converted NOTHING.
//! markup-carve/carve#2066 already ruled the answer and the HTML importer
//! already follows it: the inner span is unwrapped and its content is written in
//! the outer one, which keeps the outer kind and every character of the text.

fn migrate(markdown: &str) -> String {
    carve::try_markdown_to_carve(markdown).expect("the importer converts rather than refusing")
}

#[test]
fn an_italic_inside_an_italic_keeps_the_outer_kind_and_the_text() {
    assert_eq!(migrate("*(*foo*)*\n"), "/(foo)/\n");
}

#[test]
fn the_underscore_spelling_answers_the_same_way() {
    assert_eq!(migrate("_(_foo_)_\n"), "/(foo)/\n");
}

#[test]
fn a_strong_inside_a_strong_keeps_every_character() {
    assert_eq!(migrate("__foo, __bar__, baz__\n"), "*foo, bar, baz*\n");
}

#[test]
fn an_unnested_document_is_untouched() {
    assert_eq!(migrate("*em* and **strong**\n"), "/em/ and *strong*\n");
}

#[test]
fn what_it_wrote_reads_back_as_one_span_of_the_outer_kind() {
    let source = migrate("*(*foo*)*\n");
    assert_eq!(carve::to_html(&source), "<p><em>(foo)</em></p>");
}

#[test]
fn the_loss_is_reported_rather_than_silent() {
    let report = carve::migrate_markdown("*(*foo*)*\n").report;
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "structure-unspellable"
                && matches!(diagnostic.fidelity, carve::MigrationFidelity::Dropped)
        }),
        "no unspellable diagnostic: {:?}",
        report.diagnostics
    );
}

/// THE AST PATH KEEPS THE NESTING. Only a Carve SPELLING has to give it up, the
/// same split the HTML importer draws with its `writing` flag, so a consumer
/// reading the tree still sees what the source carried.
#[test]
fn the_ast_path_keeps_the_nesting() {
    let document = carve::markdown_to_ast("*(*foo*)*\n");
    let html = carve::render_html(&document).expect("renders");
    assert!(html.contains("<em>(<em>foo</em>)</em>"), "got: {html}");
}
