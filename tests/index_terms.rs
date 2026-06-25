use carve::{Index, Options};

fn h(source: &str) -> String {
    let index = Index::new();
    let options = Options::new().with_extension(&index);
    carve::to_html_with_options(source, &options)
        .trim()
        .to_string()
}

fn off(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn emits_invisible_span_per_marker() {
    let out = h("A :index[parser] here.\n\n::: index\n:::");
    assert!(out.contains("<span id=\"idx-parser-1\" class=\"index-term\"></span>"));
    assert!(out.contains("<p>A <span id=\"idx-parser-1\" class=\"index-term\"></span> here.</p>"));
}

#[test]
fn full_golden_matches_carve_js() {
    let out = h("A :index[parser] and :index[lexer], then :index[parser] again.\n\n::: index\n:::");
    assert_eq!(
        out,
        "<p>A <span id=\"idx-parser-1\" class=\"index-term\"></span> and \
<span id=\"idx-lexer-1\" class=\"index-term\"></span>, then \
<span id=\"idx-parser-2\" class=\"index-term\"></span> again.</p>\n\
<ul class=\"index\">\n  <li>lexer <a href=\"#idx-lexer-1\" class=\"index-backref\">\u{21a9}</a></li>\n  \
<li>parser <a href=\"#idx-parser-1\" class=\"index-backref\">\u{21a9}</a> \
<a href=\"#idx-parser-2\" class=\"index-backref\">\u{21a9}</a></li>\n</ul>"
    );
}

#[test]
fn sorted_with_backlinks() {
    let out = h("A :index[parser] and :index[lexer], then :index[parser].\n\n::: index\n:::");
    assert!(out.contains("<ul class=\"index\">"));
    assert!(out.find(">lexer ").unwrap() < out.find(">parser ").unwrap());
}

#[test]
fn numbers_occurrences_in_order() {
    let out = h(":index[a] :index[a] :index[a].\n\n::: index\n:::");
    assert!(out.contains("id=\"idx-a-1\""));
    assert!(out.contains("id=\"idx-a-2\""));
    assert!(out.contains("id=\"idx-a-3\""));
}

#[test]
fn no_markers_keeps_plain_div() {
    let out = h("No terms.\n\n::: index\n:::");
    assert!(out.contains("<div class=\"index\">"));
    assert!(!out.contains("<ul class=\"index\">"));
}

#[test]
fn off_uses_generic_fallback() {
    let out = off("A :index[parser] here.");
    assert!(out.contains("<span class=\"ext-index\">parser</span>"));
}

#[test]
fn marker_in_link_label_uses_span_not_nested_a() {
    let out = h("[see :index[parser]](/x).\n\n::: index\n:::");
    assert!(out.contains("<span id=\"idx-parser-1\" class=\"index-term\"></span>"));
    assert!(!out.contains("</a></a>"));
}

#[test]
fn footnote_def_marker_is_inert_no_dangling() {
    let out = h("Body :index[x].[^a]\n\n[^a]: Note :index[x].\n\n::: index\n:::");
    assert_eq!(out.matches("id=\"idx-x-").count(), 1);
    assert!(out.contains("id=\"idx-x-1\""));
    assert!(!out.contains("id=\"idx-x-2\""));
    assert!(out.contains("<span class=\"index-term\"></span>"));
    assert!(!out.contains("href=\"#idx-x-2\""));
}

#[test]
fn preserves_authored_content_before_list() {
    let out = h("A :index[parser].\n\n::: index\nGenerated below.\n:::");
    assert!(out.contains("Generated below."));
    assert!(out.contains("<ul class=\"index\">"));
    assert!(out.find("Generated below.").unwrap() < out.find("<ul class=\"index\">").unwrap());
}

#[test]
fn carries_block_attrs_on_ul() {
    let out = h("A :index[parser].\n\n{#book-index .two-col}\n::: index\n:::");
    assert!(out.contains("<ul id=\"book-index\" class=\"index two-col\">"));
}

#[test]
fn nested_in_blockquote() {
    let out = h("A :index[parser].\n\n> ::: index\n> :::");
    assert!(out.contains("<ul class=\"index\">"));
    assert!(out.contains(
        "<li>parser <a href=\"#idx-parser-1\" class=\"index-backref\">\u{21a9}</a></li>"
    ));
}
