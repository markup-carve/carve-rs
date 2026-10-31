#[test]
fn a_table_row_shaped_line_in_a_line_block_takes_a_lazy_line() {
    let source = "- a\n  ::: |\n  | x |\ny\n";
    let expected = "<ul>\n  <li>a\n    <div class=\"line-block\">\n      <p>| x |<br>\ny</p>\n    </div>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_lazy_line_keeps_a_line_block_open_for_its_closer() {
    let source = "- ::: |\n  a\nlazy\n  :::\ntail\n";
    let expected = "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>a<br>\nlazy</p>\n    </div>\n  </li>\n</ul>\n<p>tail</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn multiple_lazy_lines_keep_the_bare_closer_at_the_list_boundary() {
    let source = "- ::: |\n  a\nlazy1\nlazy2\n  :::\ntail\n";
    let expected = "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>a<br>\nlazy1<br>\nlazy2</p>\n    </div>\n  </li>\n</ul>\n<p>tail</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_colon_line_past_a_line_blocks_column_keeps_its_stanza_open() {
    let source = "- ::: |\n  a\nlazy\n   :::\ntail\n";
    let expected = "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>a<br>\nlazy<br>\n&nbsp;:::<br>\ntail</p>\n    </div>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_shifted_colon_line_keeps_description_and_note_stanzas_open() {
    let description = ":: t\n: ::: |\n  a\nlazy\n   :::\ntail\n";
    let note = "[^n]: ::: |\n  a\nlazy\n   :::\ntail\n\nref[^n]\n";
    let stanza = "a<br>\nlazy<br>\n&nbsp;:::<br>\ntail";
    let description_html = carve::to_html(description);
    assert!(description_html.contains(stanza), "{description_html}");
    assert!(!description_html.contains("</dl>\n<p>tail</p>"));
    let note_html = carve::to_html(note);
    assert!(note_html.contains(stanza), "{note_html}");
    assert!(note_html.starts_with("<p>ref"), "{note_html}");
}

#[test]
fn a_nested_item_closes_after_its_line_blocks_exact_closer() {
    let source = "- - ::: |\n    a\nlazy\n    :::\ntail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>a<br>\nlazy</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_nested_line_block_keeps_a_shifted_colon_as_payload() {
    // The model still drops the last line here (carve#4661). PART 9 §23
    // keeps the shifted colon as payload, and S4 keeps its stanza open.
    let source = "- - ::: |\n    a\nlazy\n     :::\ntail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>a<br>\nlazy<br>\n&nbsp;:::<br>\ntail</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_lazy_line_keeps_a_line_block_open_in_a_description() {
    let source = ":: t\n: ::: |\n  a\nlazy\n  :::\ntail\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>a<br>\nlazy</p>\n    </div>\n  </dd>\n</dl>\n<p>tail</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_lazy_line_keeps_a_line_block_open_in_a_note() {
    let source = "[^n]: ::: |\n  a\nlazy\n  :::\ntail[^n]\n";
    let expected = "<p>tail<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <div class=\"line-block\">\n        <p>a<br>\nlazy</p>\n      </div>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_line_after_a_closed_nested_line_block_leaves_the_outer_item() {
    let source = "- - ::: |\n    x\n    :::\nafter\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_line_after_a_closed_line_block_under_a_nested_paragraph_leaves_the_list() {
    let source = "- - a\n    ::: |\n    x\n    :::\nafter\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>a\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn an_opener_after_a_lazy_term_line_stays_in_the_marker_line_sub_item() {
    let source = "- * :: t\n   : d\n    > r\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n        <blockquote><p>r</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_heading_after_a_lazy_term_line_stays_in_the_ordered_sub_item() {
    let source = "- 1. :: t\n   : d\n     # h\n";
    let expected = "<ul>\n  <li>\n    <ol>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ol>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_sibling_after_the_sub_item_opener_stays_in_the_outer_list() {
    let source = "- * :: t\n   : d\n    > r\n- x\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n        <blockquote><p>r</p></blockquote>\n      </li>\n    </ul>\n  </li>\n  <li>x</li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}
