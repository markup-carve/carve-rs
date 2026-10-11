//! Markdown import preserves same-kind emphasis with explicit native braces.

fn migrate(markdown: &str) -> String {
    carve::try_markdown_to_carve(markdown).expect("the importer converts rather than refusing")
}

#[test]
fn an_italic_inside_an_italic_keeps_the_outer_kind_and_the_text() {
    assert_eq!(migrate("*(*foo*)*\n"), "{/({/foo/})/}\n");
}

#[test]
fn the_underscore_spelling_answers_the_same_way() {
    assert_eq!(migrate("_(_foo_)_\n"), "{/({/foo/})/}\n");
}

#[test]
fn a_strong_inside_a_strong_keeps_every_character() {
    assert_eq!(migrate("__foo, __bar__, baz__\n"), "{*foo, {*bar*}, baz*}\n");
}

#[test]
fn an_unnested_document_is_untouched() {
    assert_eq!(migrate("*em* and **strong**\n"), "/em/ and *strong*\n");
}

#[test]
fn what_it_wrote_reads_back_as_one_span_of_the_outer_kind() {
    let source = migrate("*(*foo*)*\n");
    assert_eq!(carve::to_html(&source), "<p><em>(<em>foo</em>)</em></p>");
}

#[test]
fn native_nesting_has_no_loss_diagnostic() {
    let report = carve::migrate_markdown("*(*foo*)*\n").report;
    assert!(!report.diagnostics.iter().any(|diagnostic| diagnostic.code == "structure-unspellable"));
}

#[test]
fn the_ast_path_keeps_the_nesting() {
    let document = carve::markdown_to_ast("*(*foo*)*\n");
    let html = carve::render_html(&document).expect("renders");
    assert!(html.contains("<em>(<em>foo</em>)</em>"), "got: {html}");
}
