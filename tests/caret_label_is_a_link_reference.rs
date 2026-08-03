//! `[^]` has an EMPTY `footnote_label`, and `footnote_label = {character - ']'}+`
//! requires one or more - so it is not a `reference_footnote`. Carve has no
//! shortcut reference either, so `[^]` is literal text.
//!
//! `[^]: /url` IS a valid LINK reference definition: `reference_label` excludes
//! only `]`, and `@` at the first position, so `^` is an ordinary label.
//!
//! This engine had it backwards on both sides - it claimed the definition for
//! the footnote path, which kept the link path from ever seeing it, and it bound
//! a bare `[^]` to that footnote. carve-js and carve-php get both right
//! (carve#552).

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_caret_label_is_a_usable_link_reference() {
    assert_eq!(
        squash(&to_html("[^]: /url\n\nsee [text][^].\n")),
        "<p>see <a href=\"/url\">text</a>.</p>"
    );
}

#[test]
fn a_bare_caret_bracket_is_literal_text() {
    // With a definition present - this is where the divergence showed. Without
    // one, every engine already agreed.
    assert_eq!(squash(&to_html("see [^].\n\n[^]: x\n")), "<p>see [^].</p>");
    assert_eq!(squash(&to_html("see [^].\n")), "<p>see [^].</p>");
}

#[test]
fn a_real_footnote_still_works() {
    let html = to_html("see [^a].\n\n[^a]: n\n");

    assert!(html.contains("doc-noteref"), "{html}");
    assert!(html.contains("<sup>1</sup>"), "{html}");
}

#[test]
fn an_ordinary_link_reference_still_works() {
    assert_eq!(
        squash(&to_html("[r]: /u\n\nsee [t][r].\n")),
        "<p>see <a href=\"/u\">t</a>.</p>"
    );
}
