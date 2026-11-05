//! carve#5195: a refused `{%` run (CARVE-P2-035) is visible paragraph text, not a comment-only paragraph.

#[test]
fn a_refused_run_after_a_plus_attaches() {
    assert_eq!(
        carve::to_html("- a\n+\n{% x\n%}\n"),
        "<ul>\n  <li>a\n    {% x\n%}\n  </li>\n</ul>"
    );
}

#[test]
fn the_line_after_the_refused_run_continues_its_paragraph() {
    assert_eq!(
        carve::to_html("- a\n+\n{% x\n%}\n{y}\n more\n"),
        "<ul>\n  <li>a\n    {% x\n%}\n{y}\nmore\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_line_plus_keeps_the_heading_shaped_line_as_text() {
    assert_eq!(
        carve::to_html("- +\n{% x\n%}\n# H\n"),
        "<ul>\n  <li>{% x\n%}\n# H</li>\n</ul>"
    );
}

#[test]
fn a_run_continued_past_its_brace_is_still_a_comment() {
    assert_eq!(
        carve::to_html("- a\n+\n{% x\n %}\n# H\n"),
        "<ul>\n  <li>a\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn a_refused_item_lead_run_is_a_paragraph() {
    assert_eq!(
        carve::to_html("- {% c\n  d %}\n\n  Body.\n"),
        "<ul>\n  <li><p>{% c\nd %}</p>\n    <p>Body.</p>\n  </li>\n</ul>"
    );
}

#[test]
fn a_refused_run_between_items_separates_the_lists() {
    assert_eq!(
        carve::to_html("- a\n\n{% c\nd %}\n\n- b\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>{% c\nd %}</p>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn a_top_level_refused_run_is_a_paragraph_without_positions() {
    let source = "{% c\nd %}\n";
    let expected = "<p>{% c\nd %}</p>";
    assert_eq!(carve::to_html(source), expected);
    let positioned = carve::Options::default().with_positions(true);
    assert_eq!(
        carve::try_to_html_with_options(source, &positioned).unwrap(),
        expected
    );
}
