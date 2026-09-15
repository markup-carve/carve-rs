//! Edge cases of inline HTML on Markdown import, past the well-formed tags that
//! `markdown_html_block_in_container` and `markdown_tight_item_inline_run` cover.
//!
//! Only a BARE, properly paired native tag converts to a Carve construct. An
//! attributed tag, an unpaired or self-closing tag, and any non-native tag are
//! kept verbatim as an inline raw span, so nothing is dropped and an unclosed
//! tag never swallows the text after it. Every expectation matches carve-js
//! `markdownToCarve`.

fn carve(markdown: &str) -> String {
    carve::markdown_to_carve(markdown)
}

#[test]
fn an_attributed_native_tag_is_kept_raw_not_converted() {
    // The class would be lost if `<b>` were converted to `*...*`; keeping the
    // run raw preserves it, the way carve-js does.
    assert_eq!(
        carve("a <b class=\"x\">y</b> c\n"),
        "a `<b class=\"x\">y</b>`{=html} c\n",
    );
    assert_eq!(
        carve("a <span data-x=\"1\">y</span> c\n"),
        "a `<span data-x=\"1\">y</span>`{=html} c\n",
    );
}

#[test]
fn an_unclosed_native_tag_does_not_swallow_the_line() {
    // Regression: an unclosed `<b>` used to open an emphasis that consumed the
    // rest of the line. It is a raw span of just the tag now.
    assert_eq!(carve("a <b> c\n"), "a `<b>`{=html} c\n");
    assert_eq!(carve("a <b>x c\n"), "a `<b>`{=html}x c\n");
}

#[test]
fn an_unpaired_close_tag_is_raw() {
    assert_eq!(carve("a </b> c\n"), "a `</b>`{=html} c\n");
}

#[test]
fn a_self_closing_native_tag_is_raw() {
    assert_eq!(carve("a <b/> c\n"), "a `<b/>`{=html} c\n");
}

#[test]
fn a_bare_native_tag_still_converts() {
    // The fix above must not stop the ordinary case from converting.
    assert_eq!(carve("a <b>y</b> c\n"), "a *y* c\n");
    assert_eq!(carve("a <code>y</code> c\n"), "a `y` c\n");
}
