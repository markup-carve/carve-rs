//! A nesting of the SAME strength has no run spelling: `*` around `*x*` is a
//! run of two, which is one strong rather than an italic inside an italic. The
//! parent takes the inline-HTML form and the child keeps its delimiters
//! (markup-carve/carve-rs#1639). Different strengths commute under PART 11's
//! normalization list and keep their runs.

fn html(src: &str) -> String {
    carve::to_markdown(src)
}

/// A same-kind nesting has no Carve source (PART 9 §9 E3), so its tree is
/// built through the HTML importer's AST exit.
fn from_html(html: &str) -> String {
    let doc = carve::html_to_ast(html, &carve::HtmlImportOptions::default())
        .unwrap()
        .value;
    carve::render_markdown(&doc).unwrap()
}

#[test]
fn an_italic_inside_an_italic_no_longer_collapses_to_a_strong() {
    assert_eq!(from_html("<p><em><em>x</em></em></p>"), "<em>*x*</em>\n");
}

#[test]
fn it_answers_the_same_way_mid_paragraph() {
    assert_eq!(
        from_html("<p>a <em><em>x</em></em> b</p>"),
        "a <em>*x*</em> b\n"
    );
}

#[test]
fn a_child_at_the_leading_edge_only_is_enough() {
    assert_eq!(from_html("<p><em><em>x</em>y</em></p>"), "<em>*x*y</em>\n");
}

#[test]
fn a_child_at_the_trailing_edge_only_is_enough() {
    assert_eq!(from_html("<p><em>y<em>x</em></em></p>"), "<em>y*x*</em>\n");
}

#[test]
fn a_strong_inside_a_strong_keeps_its_runs() {
    // Four asterisks read back as a strong inside a strong, so nothing moves.
    assert_eq!(
        from_html("<p><strong><strong>x</strong></strong></p>"),
        "****x****\n"
    );
}

#[test]
fn a_strong_inside_an_italic_keeps_its_runs() {
    // Different strengths, and the pair commutes under the normalization list.
    assert_eq!(html("/{*x*}/\n"), "***x***\n");
}

#[test]
fn a_bold_italic_keeps_its_triple() {
    assert_eq!(html("/*x*/\n"), "***x***\n");
}

#[test]
fn a_child_spelled_with_another_marker_keeps_the_parents_run() {
    assert_eq!(html("/{~x~}/\n"), "*~~x~~*\n");
}

#[test]
fn an_escaped_trailing_marker_grows_no_run() {
    assert_eq!(html("{*x\\**}\n"), "**x\\***\n");
}

#[test]
fn an_escaped_leading_marker_grows_no_run() {
    assert_eq!(html("{*\\*x*}\n"), "**\\*x**\n");
}

/// What a CommonMark reader makes of the Markdown this renderer wrote.
fn readback(html: &str) -> String {
    let md = from_html(html);
    carve::render_html(&carve::markdown_to_ast(&md)).expect("the reader renders")
}

#[test]
fn the_reader_keeps_both_levels_of_a_same_strength_nesting() {
    let back = readback("<p><em><em>x</em></em></p>");
    assert!(back.contains("<em><em>x</em></em>"), "got: {back}");
}
