//! An opener with no closer ahead is a `%%` line for every layout question
//! (PART 9 §28), in a note body and between a term and its description.

use carve::to_html;

#[test]
fn a_degraded_opener_below_the_column_closes_a_note() {
    for opener in ["%%%", "%%%%"] {
        let src = format!("x[^f]\n\n[^f]: t\n{opener}\n\n  tail\n");
        // carve#4907: the opener opens a span that hides `tail`.
        assert_ne!(
            to_html(&src),
            to_html("x[^f]\n\n[^f]: t\n%%\n\n  tail\n"),
            "{src:?}"
        );
        assert!(!to_html(&src).contains("tail"), "{src:?}");
    }
}

#[test]
fn an_opener_with_a_closer_ahead_stays_a_span_in_the_note() {
    let html = to_html("x[^f]\n\n[^f]: t\n%%%\nhid\n%%%\n");
    assert!(!html.contains("hid"), "{html}");
}

#[test]
fn a_degraded_opener_keeps_a_term_and_its_description_together() {
    for opener in ["%%%", "    %%%", "%%%tail"] {
        let src = format!(":: t\n{opener}\n:  d\n");
        // carve#4907: the opener opens a span that hides the description.
        assert_eq!(to_html(&src), "<dl>\n  <dt>t</dt>\n</dl>", "{src:?}");
    }
}

#[test]
fn a_closed_span_still_keeps_them_together() {
    assert_eq!(
        to_html(":: t\n%%%\nhid\n%%%\n:  d\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>"
    );
}
