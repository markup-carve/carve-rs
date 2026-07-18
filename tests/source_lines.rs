//! Opt-in `data-source-line` stamping for editor preview scroll-sync.

use carve::{to_html, to_html_with_options, Options};

#[test]
fn source_lines_disabled_by_default() {
    let html = to_html("# Heading\n\nPara one.\n");
    assert!(!html.contains("data-source-line"), "got: {html}");
}

#[test]
fn source_lines_stamps_top_level_blocks_one_based() {
    let opts = Options::new().with_source_lines(true);
    // 1-based source lines: 1 "# Heading", 3 "Para one.", 5 "Para two."
    let html = to_html_with_options("# Heading\n\nPara one.\n\nPara two.\n", &opts);
    assert!(html.contains("data-source-line=\"1\""), "got: {html}");
    assert!(html.contains("data-source-line=\"3\""), "got: {html}");
    assert!(html.contains("data-source-line=\"5\""), "got: {html}");
}

#[test]
fn source_lines_stamp_nested_blockquotes_and_lazy_continuations() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options("> Quote\nlazy\n> \n> ## Nested\n> \n> Para\n", &opts);

    assert!(
        html.contains("<blockquote data-source-line=\"1\">"),
        "got: {html}"
    );
    assert!(
        html.contains("<p data-source-line=\"1\">Quote\nlazy</p>"),
        "got: {html}"
    );
    assert!(
        html.contains("data-source-line=\"4\">Nested</h2>"),
        "got: {html}"
    );
    assert!(
        html.contains("<p data-source-line=\"6\">Para</p>"),
        "got: {html}"
    );
}

#[test]
fn source_lines_stamp_nested_div_content_and_code_fence() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options(
        "{.box}\n:::\nPara\n\n```rs\nfn main() {}\n```\n:::\n",
        &opts,
    );

    assert!(
        html.contains("<div class=\"box\" data-source-line=\"2\">"),
        "got: {html}"
    );
    assert!(
        html.contains("<p data-source-line=\"3\">Para</p>"),
        "got: {html}"
    );
    assert!(html.contains("<pre data-source-line=\"5\""), "got: {html}");
}

#[test]
fn source_lines_stamp_list_items_loose_paragraphs_and_nested_sublists() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options("- One\n\n  second paragraph\n\n  - Nested\n- Two\n", &opts);

    assert!(html.contains("<ul data-source-line=\"1\">"), "got: {html}");
    assert!(
        html.contains("<li data-source-line=\"1\"><p data-source-line=\"1\">One</p>"),
        "got: {html}"
    );
    assert!(
        html.contains("<p data-source-line=\"3\">second paragraph</p>"),
        "got: {html}"
    );
    assert!(html.contains("<ul data-source-line=\"5\">"), "got: {html}");
    assert!(
        html.contains("<li data-source-line=\"5\">Nested</li>"),
        "got: {html}"
    );
    assert!(
        html.contains("<li data-source-line=\"6\"><p data-source-line=\"6\">Two</p></li>"),
        "got: {html}"
    );
}

#[test]
fn source_lines_stamp_blockquote_paragraph_nested_in_list_item_body() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options(
        "- Item one\n\n  Para in item.\n\n  > quote\n\n- Item two\n",
        &opts,
    );

    assert!(
        html.contains("<blockquote data-source-line=\"5\"><p data-source-line=\"5\">quote</p>"),
        "got: {html}"
    );
}

#[test]
fn source_lines_stamp_blockquote_attached_by_list_continuation_marker() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options("- first\n\n+\n> quote\n", &opts);

    assert!(
        html.contains("<blockquote data-source-line=\"4\"><p data-source-line=\"4\">quote</p>"),
        "got: {html}"
    );
}

#[test]
fn source_lines_stamp_list_inside_quote() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options("> - Quoted\n> - List\n", &opts);

    assert!(
        html.contains("<blockquote data-source-line=\"1\">"),
        "got: {html}"
    );
    assert!(html.contains("<ul data-source-line=\"1\">"), "got: {html}");
    assert!(
        html.contains("<li data-source-line=\"1\">Quoted</li>"),
        "got: {html}"
    );
    assert!(
        html.contains("<li data-source-line=\"2\">List</li>"),
        "got: {html}"
    );
}

#[test]
fn source_lines_stamp_footnote_content() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options(
        "Use note.[^a]\n\n[^a]: Footnote paragraph\n\n  - Item\n",
        &opts,
    );

    assert!(
        html.contains("<section role=\"doc-endnotes\">"),
        "got: {html}"
    );
    assert!(
        html.contains("<p data-source-line=\"3\">Footnote paragraph</p>"),
        "got: {html}"
    );
    assert!(
        html.contains("<li data-source-line=\"5\">Item</li>"),
        "got: {html}"
    );
}

#[test]
fn source_lines_stamp_definition_terms_and_definitions() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options(":: Term\n:  Definition\n\n:: Empty\n", &opts);

    assert!(html.contains("<dl data-source-line=\"1\">"), "got: {html}");
    assert!(
        html.contains("<dt data-source-line=\"1\">Term</dt>"),
        "got: {html}"
    );
    assert!(
        html.contains("<dd data-source-line=\"2\">Definition</dd>"),
        "got: {html}"
    );
    assert!(
        html.contains("<dt data-source-line=\"4\">Empty</dt>"),
        "got: {html}"
    );
    assert!(!html.contains("<dd data-source-line=\"4\">"), "got: {html}");
}

#[test]
fn source_lines_preserve_author_data_source_line() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options(
        "-{data-source-line=99} Item\n\n{data-source-line=42}\n> Quote\n",
        &opts,
    );

    assert!(
        html.contains("<li data-source-line=\"99\">Item</li>"),
        "got: {html}"
    );
    assert!(
        html.contains("<blockquote data-source-line=\"42\">"),
        "got: {html}"
    );
}

#[test]
fn source_lines_option_off_is_byte_identical() {
    let source = "> - Quoted\n> - List\n\n:: Term\n:  Definition\n";
    let default_html = to_html(source);
    let disabled_html = to_html_with_options(source, &Options::new().with_source_lines(false));

    assert_eq!(disabled_html, default_html);
}

#[test]
fn source_lines_handles_crlf_input() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options("> Quote\r\n> \r\n> Para\r\n", &opts);

    assert!(
        html.contains("<blockquote data-source-line=\"1\">"),
        "got: {html}"
    );
    assert!(
        html.contains("<p data-source-line=\"1\">Quote</p>"),
        "got: {html}"
    );
    assert!(
        html.contains("<p data-source-line=\"3\">Para</p>"),
        "got: {html}"
    );
}

#[test]
fn source_lines_preserve_frontmatter_offset_before_trailing_link_definition() {
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options("---\ntitle: x\n---\nPara\n\n[x]: /url\n", &opts);

    assert!(
        html.contains("<p data-source-line=\"4\">Para</p>"),
        "got: {html}"
    );
    assert!(!html.contains("data-source-line=\"1\""), "got: {html}");
}
