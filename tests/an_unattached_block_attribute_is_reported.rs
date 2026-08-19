//! PART 9 §15 A4: a floating `{…}` that reaches no block is DROPPED, and the
//! drop is REPORTED (ruled in markup-carve/carve#1281).
//!
//! A4 named one way to run out of following blocks - the end of the document -
//! and the ruling names the second: the end of the CONTAINER holding the
//! attribute. A floating attribute is scoped to its container. "Applies to the
//! next block" answers WHICH BLOCK, not which container, and containment bounds
//! everything else in the language, so an attribute written inside a quote, an
//! item, a `dd` or a footnote body does not survive that container's end.
//!
//! DROPPING SILENTLY IS THE ONE THING THIS MAY NOT DO. Both cases are the lint
//! rule `unattached-block-attribute`, in the same family as the other
//! constructs that render nothing and can be written by mistake. The rule is
//! about OUTPUT: nothing is emitted for the attribute, and the processor says
//! so.
//!
//! The AST cannot answer this one. An unattached attribute leaves NOTHING
//! behind - no node, and no attrs on a neighbour - which is exactly why it needs
//! a diagnostic, and why the parser records the drop rather than the linter
//! guessing at it from the source.

use carve::lint::{lint_carve, LintWarning};

fn unattached(src: &str) -> Vec<LintWarning> {
    lint_carve(src)
        .into_iter()
        .filter(|w| w.rule == "unattached-block-attribute")
        .collect()
}

fn html(src: &str) -> String {
    carve::to_html(src)
}

// ---------------------------------------------------------------------------
// End of DOCUMENT - the half A4 already named.
// ---------------------------------------------------------------------------

#[test]
fn an_attribute_at_the_end_of_the_document_is_reported() {
    let found = unattached("para\n\n{.k}\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 3, "{found:?}");
    assert_eq!(found[0].column, 1, "{found:?}");
    // And nothing was emitted for it.
    assert_eq!(html("para\n\n{.k}\n"), "<p>para</p>");
}

#[test]
fn a_stacked_run_is_one_finding_at_the_run_s_first_block() {
    // Attribute blocks STACK into one set (§15 A3), so the run is one thing that
    // reached nothing, reported once and located where the author started it.
    let found = unattached("para\n\n{.k}\n{#i}\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 3, "{found:?}");
}

#[test]
fn the_span_covers_the_attribute_block() {
    let src = "para\n\n{.k}\n";
    let found = unattached(src);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(&src[found[0].start..found[0].end], "{.k}", "{found:?}");
}

#[test]
fn a_multi_line_block_is_reported_whole() {
    let src = "para\n\n{#id\n.foo}\n";
    let found = unattached(src);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        &src[found[0].start..found[0].end],
        "{#id\n.foo}",
        "{found:?}"
    );
}

// ---------------------------------------------------------------------------
// End of CONTAINER - the half the ruling added.
// ---------------------------------------------------------------------------

#[test]
fn an_attribute_on_a_quote_s_last_line_is_reported() {
    // The shape the ticket opened on. carve-php carried this class OUT of the
    // quote and over a blank line onto a document-level paragraph; here nothing
    // is emitted for it, and now the processor says so.
    let found = unattached("> q\n> {.k}\n\ntail\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 2, "{found:?}");
    assert_eq!(
        html("> q\n> {.k}\n\ntail\n"),
        "<blockquote><p>q</p></blockquote>\n<p>tail</p>"
    );
}

#[test]
fn an_attribute_at_a_list_item_s_end_is_reported() {
    let found = unattached("- a\n  {.k}\n\ntail\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 2, "{found:?}");
    assert_eq!(
        html("- a\n  {.k}\n\ntail\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn an_attribute_at_a_sibling_marker_is_reported() {
    // The other way a list item runs out: the next MARKER opens a sibling, which
    // is not the block the attributes were written for either.
    let found = unattached("- a\n  {.k}\n- b\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 2, "{found:?}");
    assert_eq!(
        html("- a\n  {.k}\n- b\n"),
        "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn an_attribute_at_a_definition_body_s_end_is_reported() {
    // And the body ENDS there, rather than reaching forward for a flush-left
    // line to attribute. An attribute line interrupts the open paragraph (§15
    // A1) and leaves no node, so the block-level test could not see that the
    // body had nothing open left - `tail` folded into the `dd` wearing a class
    // written for a block that never came.
    let found = unattached(":: t\n:  d\n   {.k}\ntail\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 3, "{found:?}");
    assert_eq!(
        html(":: t\n:  d\n   {.k}\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<p>tail</p>"
    );
}

#[test]
fn an_attribute_a_continuation_marker_could_not_place_is_reported() {
    // §17 L3 bounds the attachment at the blank line, so the marker's ONE block
    // is empty and this set reaches nothing. It does not travel past the
    // boundary to the quote below, and it does not come back as text.
    let found = unattached("- a\n+\n{.x}\n\n> q\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        html("- a\n+\n{.x}\n\n> q\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS. Scoped is not disabled: wherever the attribute DOES reach a block,
// it attaches and there is no finding. A rule that fires on these would be
// worse than no rule.
// ---------------------------------------------------------------------------

#[test]
fn control_an_attribute_that_reaches_a_block_is_not_reported() {
    assert!(unattached("{.k}\n\npara\n").is_empty());
    assert_eq!(html("{.k}\n\npara\n"), "<p class=\"k\">para</p>");
}

#[test]
fn control_an_attribute_that_reaches_a_block_inside_its_container_is_not_reported() {
    assert!(unattached("> {.k}\n>\n> tail\n").is_empty());
    assert_eq!(
        html("> {.k}\n>\n> tail\n"),
        "<blockquote><p class=\"k\">tail</p></blockquote>"
    );

    assert!(unattached("- a\n  {.k}\n  # H\n").is_empty());
    assert_eq!(
        html("- a\n  {.k}\n  # H\n"),
        "<ul>\n  <li>a\n    <h1 class=\"k\" id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn control_an_attribute_floating_past_an_invisible_construct_is_not_reported() {
    // §15 A2a: pending floats PAST anything that renders nothing and attaches to
    // the next VISIBLE block. It reached one, so there is nothing to report.
    assert!(unattached("{#i}\n[^f]: note\n\ne\n").is_empty());
    assert!(html("{#i}\n[^f]: note\n\ne\n").contains("<p id=\"i\">e</p>"));
}

#[test]
fn control_a_continuation_marker_that_places_its_attributes_is_not_reported() {
    assert!(unattached("- a\n+\n{.x}\n> q\n").is_empty());
    assert_eq!(
        html("- a\n+\n{.x}\n> q\n"),
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn control_an_indented_brace_line_is_text_and_not_an_attribute_at_all() {
    // The strict column-0 rule: one column in and the braces are ordinary text,
    // so there is no attribute to be unattached.
    assert!(unattached("- a\n+\n  {.x}\n> q\n").is_empty());
}

#[test]
fn control_a_document_with_no_attributes_reports_nothing() {
    assert!(unattached("para\n\n> q\n\n- a\n").is_empty());
}

#[test]
fn a_probe_parse_does_not_report_what_the_real_parse_answered() {
    // The collectors parse a candidate source to ASK A QUESTION about it - does
    // this body end in an open paragraph, does it end in a heading - and throw
    // the result away. Those parses see a FRAGMENT: the chunk `{.k}` without the
    // line the attribute might still reach, and without the lift that carries it
    // to the block it finally lands on.
    //
    // Counting them turned one finding into THREE, two of them located at 1:1
    // because a probe's cursor carries no map back to the document. A diagnostic
    // that fires twice at the wrong place for one construct is worse than none.
    let found = unattached("- a\n  {.k}\nlazy\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].line, found[0].column), (2, 3), "{found:?}");
}

// ---------------------------------------------------------------------------
// The two the diagnostic itself could get wrong. Found by review.
// ---------------------------------------------------------------------------

#[test]
fn a_block_that_takes_no_attributes_is_not_a_block_they_reached() {
    // `apply_attrs_to_block` ends in `_ => {}`, so handing it a COMMENT discards
    // the set exactly as having no block at all would - and §15 A2a's "float
    // past what renders nothing" cannot save it, because a `+` attaches ONE
    // block and there is no next one to float to. This dropped the attributes
    // with nothing reporting it, while its document-level twin reported them.
    let found = unattached("- a\n+\n{.x}\n%% c\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 3, "{found:?}");
    assert_eq!(html("- a\n+\n{.x}\n%% c\n"), "<ul>\n  <li>a</li>\n</ul>");
    // The twin, which always reported.
    assert_eq!(unattached("{.x}\n\n%% c\n").len(), 1);
}

#[test]
fn the_span_survives_the_normalization_the_parser_did_first() {
    // The positions arrive as (line, column) taken during the parse, so turning
    // them back into offsets is a question about NORMALIZATION: a leading BOM is
    // stripped and both CRLF and a lone CR collapse to LF before the parser sees
    // a line. A table that counts only `\n` and keeps the BOM disagrees, and the
    // disagreement is silent - an empty span at end of input in the first case,
    // and the mark highlighted with the closing brace lost in the second.
    for src in [
        "para\r\r{.k}\r",
        "para\r\n\r\n{.k}\r\n",
        "\u{feff}para\n\n{.k}\n",
    ] {
        let found = unattached(src);

        assert_eq!(found.len(), 1, "{src:?} {found:?}");
        assert_eq!(
            &src[found[0].start..found[0].end],
            "{.k}",
            "{src:?} {found:?}"
        );
    }
    // A leading BOM with nothing in front of the attribute, which is where the
    // off-by-one was widest.
    let src = "\u{feff}{.k}\n";
    let found = unattached(src);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(&src[found[0].start..found[0].end], "{.k}", "{found:?}");
}
