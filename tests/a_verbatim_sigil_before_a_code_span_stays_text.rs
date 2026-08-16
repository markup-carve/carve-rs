//! A verbatim sigil directly before a code span stays literal TEXT.
//!
//! The stays-text escapes ruled on carve#1130 (corpus-convert 05): Markdown's
//! `a $`x+y` b` says a dollar and then a code span - no Markdown flavour
//! spells math that way - but the bytes are exactly Carve's math span, so the
//! migrated document rendered `x+y` as math and the author's dollar vanished.
//! Same fusion for `$$` (display math) and `!` (a literal span, PART 9 §27).
//!
//! The fix lives in the canonical WRITER, not in the converter: a text node
//! ending in a dollar run or a bang in front of a code span re-parses as the
//! fused construct wherever the tree came from, so PART 11 §2 owes the escape
//! on every path that can produce the adjacency. EVERY dollar of the trailing
//! run is escaped, not just the first - with only the first escaped, the
//! remaining dollar still opens inline math against the backtick.

const MARKDOWN: &str = "a $`x+y` b\n\nc $$`x+y` d\n\ne !`x` f\n";

#[test]
fn the_migrated_carve_escapes_the_sigils() {
    let carve_source = carve::markdown_to_carve(MARKDOWN);
    assert!(carve_source.contains("a \\$`x+y` b"), "{carve_source}");
    assert!(carve_source.contains("c \\$\\$`x+y` d"), "{carve_source}");
    assert!(carve_source.contains("e \\!`x` f"), "{carve_source}");
}

#[test]
fn the_migrated_document_renders_the_sigils_as_text() {
    // The corpus-convert gate's own semantics: convert, render, compare.
    let html = carve::to_html(&carve::markdown_to_carve(MARKDOWN));
    assert_eq!(
        html,
        "<p>a $<code>x+y</code> b</p>\n<p>c $$<code>x+y</code> d</p>\n<p>e !<code>x</code> f</p>"
    );
}

#[test]
fn an_ingested_tree_takes_the_same_escape() {
    // The adjacency without any converter: a hand-built tree holding TEXT
    // that ends in a sigil, then a code span. The writer owes the same escape
    // here, or its own output re-parses as math (PART 11 §2).
    let payload = r#"{"type":"document","children":[{"type":"paragraph","children":[{"type":"text","value":"a $"},{"type":"code","value":"x"}]}],"srcByteLength":0}"#;
    let doc = carve::from_json(payload).expect("decodes");
    let written = carve::render_carve(&doc).expect("writes");
    assert_eq!(written, "a \\$`x`\n");
    assert_eq!(
        carve::to_html(&written),
        carve::render_html(&doc).expect("renders")
    );
}

#[test]
fn a_sigil_not_against_a_code_span_needs_nothing() {
    // `$` is not Carve markup on its own; PART 11 §4 asks for the minimal
    // form when dropping the escape changes nothing.
    let carve_source = carve::markdown_to_carve("cost: $5 and 10$ more\n");
    assert!(!carve_source.contains('\\'), "over-escaped: {carve_source}");
}

#[test]
fn a_dollar_pair_around_plain_text_needs_nothing() {
    // `$x+y$` is not math in CommonMark and not math in Carve either - the
    // math span needs a code span after the sigil - so the dollars pass
    // through as the ordinary text they are, unescaped.
    let carve_source = carve::markdown_to_carve("a $x+y$ b\n");
    assert!(!carve_source.contains('\\'), "over-escaped: {carve_source}");
    assert_eq!(carve::to_html(&carve_source), "<p>a $x+y$ b</p>");
}
