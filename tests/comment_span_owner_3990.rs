#[test]
fn a_column_zero_comment_line_stays_in_an_item_verse_span() {
    let html = carve::to_html("- ::: |\n  x\n  %%%\n%% c\nmore\n");
    assert!(html.contains("<p>x<br>\nmore</p>"), "{html}");
    assert!(!html.contains("<p>more</p>"), "{html}");
}

#[test]
fn a_below_column_comment_line_stays_in_a_description_verse_span() {
    let html = carve::to_html(":: t\n: ::: |\n  x\n  %%%\n %% c\n  %%%\n  more\n");
    assert!(html.contains("<p>x<br>\nmore</p>"), "{html}");
}

#[test]
fn a_below_column_comment_line_keeps_a_note_verse_span() {
    let html = carve::to_html("[^n]: ::: |\n  x\n  %%%\n %% c\n%%%\n\n[^n]\n");
    assert!(html.contains("role=\"doc-noteref\""), "{html}");
    assert!(
        html.contains("<div class=\"line-block\">\n        <p>x</p>"),
        "{html}"
    );
}

#[test]
fn a_quoted_note_verse_takes_no_lazy_heading() {
    let html = carve::to_html("> [^n]: ::: |\n>   x\n>\n>   y\n# h\n\n[^n]\n");
    assert!(html.contains("<h1>h</h1>"), "{html}");
    assert!(!html.contains("# h"), "{html}");
}

#[test]
fn a_span_ended_below_its_marker_owner_leaves_a_later_span_to_the_document() {
    let source = ". a\n  . %%%\n    n\n  zz\n%%%\n";
    assert_eq!(
        carve::to_carve(source),
        ". a\n  . %%%\n    n\n    %%%\n  zz\n\n%%%\n%%%\n"
    );
}

#[test]
fn a_span_ended_below_its_descendant_owner_leaves_a_later_span_to_the_document() {
    let source = "1. a\n   . b\n     %%%\n     n\n    - q\n%%%\n";
    let out = carve::to_carve(source);
    assert!(out.ends_with("\n\n%%%\n%%%\n"), "{out}");
}

#[test]
fn a_column_zero_closer_of_an_indented_span_does_not_swallow_the_container_closer() {
    let html = carve::to_html("::: note\n- b\n  %%%\n  n\n%%%\n:::\n");
    assert!(!html.contains("<div>"), "{html}");
    assert!(html.trim_end().ends_with("</aside>"), "{html}");
}

#[test]
fn a_nested_div_closer_inside_an_indented_span_is_payload() {
    // `::::` reaches the inner div that owns the span, so it is payload and the
    // column-0 `%%%` closes the span; `:::` then opens a div in the inner note.
    let html = carve::to_html("::: note\n:::: note\n  %%%\n::::\n%%%\n:::\nhidden\n%%%\n:::\n");
    assert!(
        html.contains("<div>\n      <p>hidden</p>\n    </div>"),
        "{html}"
    );
}

#[test]
fn an_indented_verse_span_opener_hides_the_line_block_closer() {
    let html = carve::to_html("::: |\n %%%\n:::\n%%%\n");
    assert_eq!(html.trim_end(), "<div class=\"line-block\">\n\n</div>");
    let html = carve::to_html("::: |\n %%%\n:::\nzz\n");
    assert!(!html.contains("zz"), "{html}");
}
