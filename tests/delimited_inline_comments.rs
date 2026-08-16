use carve::{
    from_json, lint_carve, parse, to_ansi, to_carve, to_html, to_json, to_markdown, to_plain_text,
};

#[test]
fn delimiters_close_at_the_first_closer_and_unterminated_openers_stay_literal() {
    assert_eq!(to_html("foo {% bar %} baz"), "<p>foo  baz</p>");
    assert_eq!(to_html("foo {%bar%} baz"), "<p>foo  baz</p>");
    assert_eq!(to_html("a {% one {% two %} b"), "<p>a  b</p>");
    assert_eq!(to_html("a {% oops"), "<p>a {% oops</p>");
}

#[test]
fn verbatim_escaping_and_inline_structure_keep_their_meaning() {
    assert_eq!(
        to_html("Run `a {% x %} b` then done."),
        "<p>Run <code>a {% x %} b</code> then done.</p>"
    );
    assert_eq!(to_html("a `x`{=html} {% c %} b"), "<p>a x  b</p>");
    assert_eq!(
        to_html(r"a \{% not a comment %} b"),
        "<p>a {% not a comment %} b</p>"
    );
    assert_eq!(
        to_html("*bo{% c %}ld* text"),
        "<p><strong>bold</strong> text</p>"
    );
    assert_eq!(to_html("[li{% c %}nk](u)"), "<p><a href=\"u\">link</a></p>");
}

#[test]
fn comments_cross_soft_breaks_but_not_paragraphs() {
    assert_eq!(to_html("a {% one\ntwo %} b"), "<p>a  b</p>");
    assert_eq!(
        to_html("a {% one\n\ntwo %} b"),
        "<p>a {% one</p>\n<p>two %} b</p>"
    );
}

#[test]
fn every_presentation_target_drops_the_delimited_form() {
    let source = "foo {% bar %} baz";
    assert_eq!(to_markdown(source).trim(), "foo  baz");
    assert_eq!(to_plain_text(source).trim(), "foo  baz");
    assert_eq!(to_ansi(source).trim(), "foo  baz");
}

#[test]
fn both_comment_forms_work_in_one_table_cell() {
    let html = to_html("| a {% one %} b %% two | c |\n|---|---|\n| d | e |");
    assert!(html.contains(">a  b</th>"), "{html}");
    assert!(!html.contains("one") && !html.contains("two"), "{html}");
}

#[test]
fn ast_and_canonical_writer_preserve_the_author_choice() {
    let source = "foo {% bar %} baz";
    let json = to_json(&parse(source));
    assert!(json.contains("\"delimited\":true"), "{json}");
    assert!(json.contains("\"content\":\"bar\""), "{json}");
    assert!(!to_json(&parse("foo %% bar")).contains("delimited"));

    let decoded = from_json(&json).unwrap();
    assert_eq!(carve::render_carve(&decoded).unwrap().trim(), source);
    assert_eq!(to_carve(source).trim(), source);
    assert_eq!(to_carve(&to_carve(source)), to_carve(source));
    assert_eq!(to_html(&to_carve(source)), to_html(source));
}

#[test]
fn template_source_shape_reports_without_rewriting() {
    // ONE WARNING PER TAG-SHAPED COMMENT: the opener and the closer both
    // vanish into comments, and both are reported. `{% note %}` in the same
    // document is an ordinary comment and is not (carve validation.md).
    let source = "{% raw %} {{ value }} {% endraw %}\n\nText {% note %}.";
    let warnings = lint_carve(source);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings
        .iter()
        .all(|w| w.rule == "braced-comment-in-a-template-source"));
    assert_eq!(warnings.iter().map(|w| w.line).collect::<Vec<_>>(), [1, 1]);
    assert_eq!(
        to_carve(source),
        "{% raw %} {{ value }} {% endraw %}\n\nText {% note %}.\n"
    );
    assert!(lint_carve("Text {% ordinary note %}.").is_empty());
    assert!(lint_carve("`{% raw %}`").is_empty());
    assert!(lint_carve(r"\{% raw %}").is_empty());
}
