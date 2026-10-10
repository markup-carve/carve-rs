//! A Markdown importer writing an inline raw span chooses which bytes become
//! the code span's verbatim content. PART 2 spells the construct
//! `raw_inline = backtick_run, code_span_content, backtick_run, '{=',
//! format_name, '}'`, so the payload holds a code span's content and nothing
//! else; the only bytes with grounds to be there are the recognized
//! construct's, and an HTML start tag ends at its `>`.
//!
//! carve-rs swallowed the rest of the paragraph into the payload instead. The
//! payload is opaque, so the widened form renders the same and hides document
//! text where no inline rule reaches it (markup-carve/carve-rs#2409).

use carve::{markdown_to_carve, migrate_markdown, to_html};

fn raw_span_extent(markdown: &str) -> String {
    markdown_to_carve(markdown)
}

#[test]
fn a_start_tag_payload_stops_at_its_closing_angle_bracket() {
    assert_eq!(
        raw_span_extent("x <a href=\"foo  bar\"> y\n"),
        "x `<a href=\"foo  bar\">`{=html} y\n"
    );
}

#[test]
fn trailing_inline_markup_outside_the_payload_still_renders() {
    let carve = raw_span_extent("x <a href=\"foo\"> *y*\n");
    assert_eq!(carve, "x `<a href=\"foo\">`{=html} /y/\n");
    assert_eq!(to_html(&carve), "<p>x <a href=\"foo\"> <em>y</em></p>");
}

#[test]
fn a_start_tag_at_a_line_end_needs_no_trailing_text() {
    assert_eq!(
        raw_span_extent("x <a href=\"foo\">\n"),
        "x `<a href=\"foo\">`{=html}\n"
    );
}

#[test]
fn two_start_tags_on_one_line_each_bound_their_own_payload() {
    assert_eq!(
        raw_span_extent("x <a href=\"a\"> y <b> z\n"),
        "x `<a href=\"a\">`{=html} y `<b>`{=html} z\n"
    );
}

#[test]
fn an_angle_bracket_inside_an_attribute_value_stays_in_the_payload() {
    assert_eq!(
        raw_span_extent("x <a title=\"a > b\"> y\n"),
        "x `<a title=\"a > b\">`{=html} y\n"
    );
}

#[test]
fn punctuation_right_after_the_tag_stays_in_the_document() {
    assert_eq!(
        raw_span_extent("x <a href=\"foo\">, y\n"),
        "x `<a href=\"foo\">`{=html}, y\n"
    );
}

#[test]
fn a_paired_element_writes_two_payloads_and_keeps_its_text() {
    assert_eq!(
        raw_span_extent("x <a href=\"u\">*y*</a> z\n"),
        "x `<a href=\"u\">`{=html}/y/`</a>`{=html} z\n"
    );
    assert_eq!(
        raw_span_extent("x <span>y</span> z\n"),
        "x `<span>`{=html}y`</span>`{=html} z\n"
    );
}

/// markup-carve/carve#2804's `raw-span-whitespace-trimmed` is about a payload
/// losing trailing whitespace at a LINE END. Extent is a different axis, and
/// none of these inputs puts whitespace there, so no engine reports it.
#[test]
fn bounding_the_payload_emits_no_whitespace_trimmed_diagnostic() {
    for markdown in [
        "x <a href=\"foo  bar\"> y\n",
        "x <a href=\"foo\"> *y*\n",
        "x <a href=\"foo\">\n",
        "x <a href=\"a\"> y <b> z\n",
        "x <a title=\"a > b\"> y\n",
        "x <a href=\"foo\">, y\n",
        "x <a href=\"u\">*y*</a> z\n",
    ] {
        let codes: Vec<String> = migrate_markdown(markdown)
            .report
            .diagnostics
            .into_iter()
            .map(|row| row.code)
            .collect();
        assert!(
            !codes
                .iter()
                .any(|code| code == "raw-span-whitespace-trimmed"),
            "{markdown:?} reported it: {codes:?}"
        );
    }
}
