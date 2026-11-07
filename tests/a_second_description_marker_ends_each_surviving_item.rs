// carve#5320: each `: ` line at a surviving item's marker column ends it while
// its description is open; once closed, the line folds (ruling on carve#5323).

#[test]
fn nested_description_markers_end_each_item_in_turn() {
    let input = "- - a\n    :: t\n    : u\n      ::: |\n        x\n  : b\n: c\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>a\n        <dl>\n          <dt>t</dt>\n          <dd>\n            <p>u</p>\n            <div class=\"line-block\">\n              <p>&nbsp;&nbsp;x</p>\n            </div>\n          </dd>\n        </dl>\n      </li>\n    </ul>\n    : b\n: c\n  </li>\n</ul>";
    assert_eq!(carve::to_html(input), expected);
}

#[test]
fn ordered_outer_item_ends_at_its_own_marker_column() {
    let input = "1. - a\n     :: t\n     : u\n       ::: |\n         x\n   : b\n: c\n";
    let html = carve::to_html(input);
    assert!(html.contains("    : b\n: c\n  </li>\n</ol>"), "{html}");
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
    let expected =
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n    </dl>\n    a\n: x\n: z\n  </li>\n</ul>";
    assert_eq!(carve::to_html(input), expected);
}

#[test]
fn a_task_items_ended_term_entry_has_the_same_marker_boundary() {
    let input = "- [ ] :: t\n       a\n      : x\n: z\n";
    let html = carve::to_html(input);
    assert!(html.ends_with("a\n: x\n: z\n  </li>\n</ul>"), "{html}");
}

#[test]
fn a_later_paragraph_releases_an_earlier_definition_boundary() {
    let input = "- a\n  :: t\n\n  filler\n: b\n: c\n";
    let html = carve::to_html(input);
    assert!(html.contains("<p>filler\n: b\n: c</p>"), "{html}");
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
fn an_earlier_definition_list_releases_a_later_div_paragraph() {
    let input = "- a\n  :: t\n\n  ::: box\n  prose\n: z\n";
    let html = carve::to_html(input);
    assert!(html.contains("<p>prose\n: z</p>"), "{html}");
}

#[test]
fn an_earlier_definition_list_releases_a_later_stanza_line() {
    let input = "- a\n  :: t\n\n  ::: |\n    prose\n: z\n";
    let html = carve::to_html(input);
    assert!(html.contains("prose<br>\n: z</p>"), "{html}");
}

#[test]
fn a_later_div_quote_keeps_its_open_paragraph() {
    let input = "- a\n  :: t\n\n  ::: box\n  > prose\n: z\n";
    let html = carve::to_html(input);
    assert!(html.contains("<p>prose\n: z</p>"), "{html}");
}

#[test]
fn a_closed_div_definition_list_releases_a_later_item_paragraph() {
    let input = "- a\n  ::: box\n  :: t\n  :::\n\n  prose\n: z\n";
    let html = carve::to_html(input);
    assert!(html.contains("<p>prose\n: z</p>"), "{html}");
}

#[test]
fn a_later_block_releases_a_term_only_marker_boundary() {
    for (input, tail) in [
        (
            "- a\n  :: t\n  # H\n  w\n: z\n",
            "    w\n: z\n  </li>\n</ul>",
        ),
        ("- a\n  :: t\n  x\n: d\n", "    x\n: d\n  </li>\n</ul>"),
        (
            "- a\n  > +\n  :: t\n  x\n: d\n",
            "    x\n: d\n  </li>\n</ul>",
        ),
        (
            "- a\n  > +\n  :: t\n  x\n: d\n: e\n",
            "    x\n: d\n: e\n  </li>\n</ul>",
        ),
        (
            "- a\n  :: m\n  - i\n: l\n",
            "      <li>i\n: l</li>\n    </ul>\n  </li>\n</ul>",
        ),
        (
            "- a\n  :: m\n  : d\n\n  p\n\n  : x\n: l\n",
            "    <p>: x\n: l</p>\n  </li>\n</ul>",
        ),
    ] {
        let html = carve::to_html(input);
        assert!(html.ends_with(tail), "input: {input:?}\n{html}");
    }
}
