//! PART 11 R1: which headings an implicit `[label][]` reference can reach.
//!
//! The index is shared with `</#id>` crossrefs, and that sharing is what this
//! engine got wrong (carve-rs#410): a crossref DOES resolve into quoted
//! material, an implicit reference does not. Quoted text names the quoted
//! document's headings, not this one's, and a quotation is the one container
//! whose wording the author does not control.
//!
//! Found by the combinatorial check in markup-carve/carve#452. The corpus
//! covered implicit references and covered headings in blockquotes, and never
//! put both in one document, so three engines declined and this one resolved
//! with every suite green.

use carve::to_html;

#[test]
fn declines_a_heading_under_a_blockquote() {
    let html = to_html("> # H\n>\n> See [H][].\n");
    assert!(html.contains("<p>See [H][].</p>"), "{html}");
}

#[test]
fn declines_in_either_nesting_order() {
    // A blockquote ANCESTOR declines, however deep and whichever way around.
    let quote_in_div = to_html(":::\n> # H\n:::\n\nSee [H][].\n");
    assert!(quote_in_div.contains("See [H][]."), "{quote_in_div}");

    let div_in_quote = to_html("> :::\n> # H\n> :::\n\nSee [H][].\n");
    assert!(div_in_quote.contains("See [H][]."), "{div_in_quote}");
}

#[test]
fn resolves_into_a_list_item() {
    // Only a blockquote declines. A list item is the author's own grouping in
    // their own document, so the heading and its wording are theirs.
    let html = to_html("- # H\n\nSee [H][].\n");
    assert!(html.contains(r##"<a href="#H">H</a>"##), "{html}");
}

#[test]
fn resolves_into_a_div() {
    let html = to_html(":::\n# H\n:::\n\nSee [H][].\n");
    assert!(html.contains(r##"<a href="#H">H</a>"##), "{html}");
}

#[test]
fn resolves_at_top_level() {
    let html = to_html("# H\n\nSee [H][].\n");
    assert!(html.contains(r##"<a href="#H">H</a>"##), "{html}");
}

#[test]
fn a_crossref_still_reaches_a_quoted_heading() {
    // The non-regression that matters: declining the reference index must not
    // make the heading unreachable. `</#id>` addresses it by id rather than by
    // wording, and still resolves.
    let html = to_html("> # H\n\nSee </#H>.\n");
    assert!(html.contains(r##"<a href="#H">H</a>"##), "{html}");
}

#[test]
fn a_quoted_heading_still_gets_its_id_and_dedupes() {
    // Declined from the reference index, but otherwise an ordinary heading:
    // slugged, and sharing the one document-order dedup namespace.
    let html = to_html("# abc\n\n> # abc\n\n# abc\n");
    assert!(html.contains(r##"id="abc""##), "{html}");
    assert!(html.contains(r##"id="abc-2""##), "{html}");
    assert!(html.contains(r##"id="abc-3""##), "{html}");
}

#[test]
fn a_link_definition_still_wins_over_a_heading() {
    let html = to_html("# H\n\n[H]: /wins\n\nSee [H][].\n");
    assert!(html.contains(r##"<a href="/wins">H</a>"##), "{html}");
}
