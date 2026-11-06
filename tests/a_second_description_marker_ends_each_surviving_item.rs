// carve#5320: each `: ` line at a surviving item's marker column ends it.

#[test]
fn nested_description_markers_end_each_item_in_turn() {
    let input = "- - a\n    :: t\n    : u\n      ::: |\n        x\n  : b\n: c\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>a\n        <dl>\n          <dt>t</dt>\n          <dd>\n            <p>u</p>\n            <div class=\"line-block\">\n              <p>&nbsp;&nbsp;x</p>\n            </div>\n          </dd>\n        </dl>\n      </li>\n    </ul>\n    : b\n  </li>\n</ul>\n<p>: c</p>";
    assert_eq!(carve::to_html(input), expected);
}

#[test]
fn ordered_outer_item_ends_at_its_own_marker_column() {
    let input = "1. - a\n     :: t\n     : u\n       ::: |\n         x\n   : b\n: c\n";
    let html = carve::to_html(input);
    assert!(
        html.contains("    : b\n  </li>\n</ol>\n<p>: c</p>"),
        "{html}"
    );
}

#[test]
fn a_bare_stanza_still_takes_a_description_shaped_lazy_line() {
    let input = "- a\n  - b\n    ::: |\n      v\n      %%%\n: d\n";
    let html = carve::to_html(input);
    assert!(html.contains("v<br>\n: d</p>"), "{html}");
}

#[test]
fn a_description_shaped_stanza_line_does_not_end_an_item() {
    let input = "- a\n  - b\n    ::: |\n      : x\n: d\n";
    let html = carve::to_html(input);
    assert!(html.contains(": x<br>\n: d</p>"), "{html}");
}

#[test]
fn an_ended_term_entry_keeps_no_claim_at_the_item_marker() {
    let input = "- :: t\n   a\n  : x\n: z\n";
    let expected = "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n    </dl>\n    a\n: x\n  </li>\n</ul>\n<p>: z</p>";
    assert_eq!(carve::to_html(input), expected);
}

#[test]
fn a_task_items_ended_term_entry_has_the_same_marker_boundary() {
    let input = "- [ ] :: t\n       a\n      : x\n: z\n";
    let html = carve::to_html(input);
    assert!(html.ends_with("  </li>\n</ul>\n<p>: z</p>"), "{html}");
}

#[test]
fn a_marker_column_separator_ends_an_item_with_an_earlier_definition_list() {
    let input = "- a\n  :: t\n\n  filler\n: b\n: c\n";
    let html = carve::to_html(input);
    assert!(html.ends_with("<p>: b\n: c</p>"), "{html}");
}

#[test]
fn a_quoted_definition_list_does_not_end_its_items_open_quote_paragraph() {
    let input = "- a\n  > :: t\n  > prose\n: z\n";
    let html = carve::to_html(input);
    assert!(html.contains("<p>prose\n: z</p>"), "{html}");
}

#[test]
fn an_earlier_definition_list_does_not_end_a_later_open_quote_paragraph() {
    let input = "- a\n  :: t\n\n  > prose\n: z\n";
    let html = carve::to_html(input);
    assert!(html.contains("<p>prose\n: z</p>"), "{html}");
}

#[test]
fn an_earlier_definition_list_still_ends_a_later_div() {
    let input = "- a\n  :: t\n\n  ::: box\n  prose\n: z\n";
    let html = carve::to_html(input);
    assert!(html.ends_with("<p>: z</p>"), "{html}");
}
