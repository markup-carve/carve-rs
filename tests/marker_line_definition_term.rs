//! PART 9 §24 C3 names the block openers a list item's marker line can carry:
//!
//!   "The block-opener set is UNIFORM and closed: block quote, heading,
//!   thematic break, fenced code, colon fence / admonition, TABLE, and
//!   DEFINITION LIST (a `:: term` opener …)"
//!
//! `marker_content_starts_block` implemented every member of that set except
//! the last, so `* :: t` kept the term as literal item text where carve-js and
//! carve-php open a definition list inside the item.
//!
//! Found by the differential fuzzer on the markdown target (carve#545 made
//! non-HTML targets reachable), though the divergence is visible in HTML too.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_definition_term_on_the_marker_line_opens_a_list() {
    assert_eq!(
        squash(&to_html("* :: t\n")),
        "<ul> <li> <dl> <dt>t</dt> </dl> </li> </ul>"
    );
}

#[test]
fn its_definition_attaches_at_the_content_column() {
    let html = squash(&to_html("* :: t\n  :  d\n"));

    assert!(html.contains("<dt>t</dt>"), "{html}");
    assert!(html.contains("<dd>d</dd>"), "{html}");
}

#[test]
fn an_ordered_marker_behaves_the_same() {
    assert_eq!(
        squash(&to_html("1. :: t\n")),
        "<ol> <li> <dl> <dt>t</dt> </dl> </li> </ol>"
    );
}

#[test]
fn a_colon_fence_on_the_marker_line_is_unchanged() {
    // `:::` is a div, not a term - the two must not be confused by a looser
    // prefix test.
    let html = squash(&to_html("* ::: note\n  body\n  :::\n"));

    assert!(html.contains("admonition"), "{html}");
    assert!(!html.contains("<dt>"), "{html}");
}

#[test]
fn a_content_less_term_marker_stays_literal() {
    // MARKER REQUIRES CONTENT applies to `::` as well (carve#512), so this is
    // item text, not a definition list.
    assert_eq!(squash(&to_html("* :: \n")), "<ul> <li>::</li> </ul>");
}

#[test]
fn a_definition_in_a_description_nested_under_a_marker_line_term_is_collected() {
    for marker in ["- [ ]", "-"] {
        let source = format!(":: t\n: {marker} :: t\n    : [r]:\n         /u\n");
        let html = to_html(&source);
        assert!(html.contains("<dd>/u</dd>"), "{marker}: {html}");
        assert!(!html.contains("[r]:"), "{marker}: {html}");
    }
}

#[test]
fn a_separator_before_the_nested_item_content_column_stays_in_the_term() {
    let source = ":: t\n: - [ ] :: t\n   : [r]:\n         /u\n";
    let html = to_html(source);
    assert!(html.contains("<dt>t\n: [r]:\n     /u</dt>"), "{html}");
}

#[test]
fn the_nested_description_boundary_survives_a_quote_or_an_outer_item() {
    for source in [
        "> :: t\n> : - [ ] :: t\n>     : [r]:\n>          /u\n",
        "- :: t\n  : - [ ] :: t\n      : [r]:\n           /u\n",
    ] {
        let html = to_html(source);
        assert!(html.contains("<dd>/u</dd>"), "{source:?}: {html}");
    }
    let below = to_html("- :: t\n  : - [ ] :: t\n     : [r]:\n           /u\n");
    assert!(below.contains(": [r]:"), "{below}");
}

#[test]
fn the_separator_must_reach_the_innermost_marker_line_item() {
    for indent in [4, 5] {
        let source = format!(
            ":: t\n: - - :: t\n{}: [r]:\n         /u\n",
            " ".repeat(indent)
        );
        let html = to_html(&source);
        assert!(html.contains(": [r]:"), "{indent}: {html}");
    }
    let at = to_html(":: t\n: - - :: t\n      : [r]:\n         /u\n");
    assert!(at.contains("<dd>/u</dd>"), "{at}");
}
