//! PART 11 R1's implicit heading fallback keys the index by heading TEXT:
//!
//!   "the document's headings keyed by their rendered plain text -- and
//!   resolves to `#{slug}` of the FIRST heading with that text"
//!
//! This engine looked the label up among the heading IDS instead. That agrees
//! with the text index whenever the id is the slug of the text, which is the
//! usual case - and stops agreeing the moment an author sets the id explicitly,
//! since `[H][]` then has no id `H` to find and reverts to literal source
//! (carve-rs#477).
//!
//! carve-js and carve-php key by text and resolve it.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn an_id_from_a_preceding_attribute_block_still_resolves() {
    let html = squash(&to_html("{#x}\n# H\n\nSee </#x> and [H][].\n"));

    assert_eq!(
        html,
        "<section id=\"x\"> <h1>H</h1> <p>See <a href=\"#x\">H</a> and <a href=\"#x\">H</a>.</p> </section>"
    );
}

#[test]
fn the_slug_case_is_unchanged() {
    let html = squash(&to_html("# H\n\nSee [H][].\n"));

    assert!(html.contains("<a href=\"#H\">H</a>"), "{html}");
}

#[test]
fn matching_stays_case_insensitive_and_whitespace_collapsed() {
    // R1: "the label and the heading text are both trimmed, their internal
    // whitespace runs collapsed to one space, and then compared
    // case-INSENSITIVELY".
    assert!(to_html("{#g}\n# Getting Started\n\n[getting started][]\n").contains("href=\"#g\""));
    assert!(to_html("{#ab}\n# A   B\n\n[A B][]\n").contains("href=\"#ab\""));
}

#[test]
fn the_first_heading_with_that_text_wins() {
    // Two headings share the text; the reference takes the FIRST.
    let html = to_html("{#one}\n# H\n\n{#two}\n# H\n\n[H][]\n");

    assert!(html.contains("href=\"#one\""), "{html}");
    assert!(!html.contains("href=\"#two\""), "{html}");
}

#[test]
fn an_explicit_link_definition_still_wins_over_a_heading() {
    // R1: "linkDefs WINS on a tie".
    let html = to_html("{#x}\n# H\n\n[H]: /elsewhere\n\n[H][]\n");

    assert!(html.contains("href=\"/elsewhere\""), "{html}");
}

#[test]
fn a_quoted_heading_is_still_declined() {
    // R1 declines a heading with a blockquote ancestor, explicit id or not.
    let html = to_html("> {#q}\n> # H\n\n[H][]\n");

    assert!(!html.contains("href=\"#q\""), "{html}");
}
