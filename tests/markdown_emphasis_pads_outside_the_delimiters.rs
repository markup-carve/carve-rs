//! A delimiter run only OPENS emphasis while it is left-flanking, which a run
//! followed by whitespace never is (CommonMark 6.2), so `** x**` reads back as
//! literal text and the emphasis is lost on the way out. The padding is
//! content, so it moves outside the delimiters instead of being trimmed. At a
//! paragraph edge Markdown collapses it either way; mid-paragraph it survives
//! (carve-js#1683).

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
