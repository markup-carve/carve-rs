//! A reference written inside an inline note's content resolves.
//!
//! PART 9 §16 disables FOOTNOTE recognition inside a note and says nothing about
//! references, so a note's content is ordinary inline content and a reference in
//! it resolves like any other (markup-carve/carve#1203).
//!
//! `resolve_reference_links_inline` had no arm for the note, so `^[see [t][r]]`
//! reached the reader as literal text while `*[t][r]*` one node over resolved.
//! The crossref pass a few hundred lines up already descended into a note, which
//! is why `^[see </#h>]` worked and this did not: one rule, two walks, and only
//! one of them complete.
//!
//! ## The sweep found three hosts, not one
//!
//! Every inline node carrying inline children was measured with each of the
//! three reference kinds inside it, against carve-js `f05f3a7`:
//!
//! | host | before |
//! | --- | --- |
//! | emphasis, span, link text, extension, citation group | resolves |
//! | an inline note | LITERAL |
//! | a critic insertion | LITERAL |
//! | a critic deletion | LITERAL |
//!
//! `CriticSubstitute` and `CriticComment` hold strings rather than children, so
//! there is nothing to descend into and no arm was written for them - a branch
//! that could not fail. The substitute is measured below anyway, because "it has
//! no children" is a claim about the tree that a test can hold to.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source)
}

/// The endnote body of a one-note document, without the backlink.
fn note_body(source: &str) -> String {
    let rendered = html(source);
    let body = rendered
        .split("<li id=\"fn1\">")
        .nth(1)
        .unwrap_or_else(|| panic!("no endnote in:\n{rendered}"));
    let body = body.split("<a href=\"#fnref1\"").next().unwrap_or_default();
    body.trim().trim_start_matches("<p>").trim().to_string()
}

#[test]
fn a_reference_link_inside_a_note_resolves() {
    // corpus 315-an-inline-note-s-content-resolves-after-the-note-3.
    assert_eq!(
        note_body("a ^[see [t][r]] b\n\n[r]: /u\n"),
        "see <a href=\"/u\">t</a>"
    );
}

#[test]
fn an_image_reference_inside_a_note_resolves() {
    // corpus 315-...-4.
    assert_eq!(
        note_body("a ^[see ![z][r]] b\n\n[r]: /i.png\n"),
        "see <img src=\"/i.png\" alt=\"z\">"
    );
}

#[test]
fn a_collapsed_reference_inside_a_note_reaches_the_heading_index() {
    // corpus 315-...-5. The label answers through the heading index rather than
    // through a definition, which is a different branch of the same resolver.
    assert_eq!(
        note_body("a ^[see [h][]] b\n\n# h\n"),
        "see <a href=\"#h\">h</a>"
    );
}

#[test]
fn a_reference_nested_deeper_inside_a_note_resolves_too() {
    // The arm recurses, so the note is not a special case one level down.
    assert_eq!(
        note_body("a ^[see *[t][r]*] b\n\n[r]: /u\n"),
        "see <strong><a href=\"/u\">t</a></strong>"
    );
}

#[test]
fn a_reference_inside_a_critic_range_resolves() {
    // The same gap, in the two other inline nodes that carry children. Found by
    // sweeping the node kinds rather than by reading the ticket.
    assert_eq!(
        html("x {++[t][r]++} y\n\n[r]: /u\n"),
        "<p>x <ins>+<a href=\"/u\">t</a>+</ins> y</p>"
    );
    assert_eq!(
        html("x {--![z][r]--} y\n\n[r]: /i.png\n"),
        "<p>x <del>-<img src=\"/i.png\" alt=\"z\">-</del> y</p>"
    );
    assert_eq!(
        html("x {++[h][]++} y\n\n# h\n"),
        "<p>x <ins>+<a href=\"#h\">h</a>+</ins> y</p>\n<section id=\"h\">\n  <h1>h</h1>\n</section>"
    );
}

#[test]
fn an_unresolved_reference_inside_a_note_stays_its_source() {
    // The resolver reaching the note does not make every bracket pair a link.
    // PART 12 §3a keeps the node and the reader sees the source (PART 9R). The
    // document carries a definition the label does NOT match, so a resolver
    // that stopped keying on the label would be caught here rather than passing
    // for want of anything to resolve to.
    assert_eq!(
        note_body("a ^[see [t][nope]] b\n\n[r]: /u\n"),
        "see [t][nope]"
    );
}

#[test]
fn a_note_s_content_still_recognizes_no_note() {
    // markup-carve/carve#1191 is the rule this one sits beside, and it is
    // untouched: §16 disables FOOTNOTE recognition inside a note, which is a
    // different rule from reference resolution.
    assert_eq!(note_body("x ^[a ^[b] c]\n"), "a ^[b] c");
    assert_eq!(note_body("x ^[a [^1] c]\n\n[^1]: n\n"), "a [^1] c");
}

#[test]
fn a_critic_substitution_has_no_children_to_resolve() {
    // Named because the arm this test would justify is deliberately absent.
    // `CriticSubstitute` carries `old_text` and `new_text` as strings, so its
    // halves never become nodes and no reference in them can resolve - which is
    // also what carve-js does, byte for byte.
    assert_eq!(
        html("x {~~a~>[t][r]~~} y\n\n[r]: /u\n"),
        "<p>x <del>~a</del><ins>[t][r]~</ins> y</p>"
    );
}
