//! An abbreviation line that reaches a container stays text. Under the later
//! carve#4412 ruling, a line that misses every frame defines in the document.

#[test]
fn an_abbreviation_below_an_item_ends_its_paragraph() {
    assert_eq!(
        carve::to_html("- >~\n*[A]: b\n"),
        "<ul>\n  <li>&gt;~</li>\n</ul>"
    );
}

#[test]
fn an_abbreviation_below_a_marker_line_term_ends_it() {
    assert_eq!(
        carve::to_html(". :: t\n*[A]: b\n"),
        "<ol>\n  <li>\n    <dl>\n      <dt>t</dt>\n    </dl>\n  </li>\n</ol>"
    );
}

#[test]
fn an_abbreviation_below_a_top_level_term_defines() {
    assert_eq!(
        carve::to_html(":: t\n*[A]: b\n\nA\n"),
        "<dl>\n  <dt>t</dt>\n</dl>\n<p><abbr title=\"b\">A</abbr></p>"
    );
}

#[test]
fn an_abbreviation_below_a_quote_in_an_item_ends_it() {
    assert_eq!(
        carve::to_html("- > q\n*[A]: b\n"),
        "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn an_abbreviation_still_interrupts_a_document_paragraph() {
    assert_eq!(
        carve::to_html("a\n*[A]: b\n\nA\n"),
        "<p>a</p>\n<p><abbr title=\"b\">A</abbr></p>"
    );
}

#[test]
fn an_abbreviation_still_ends_a_top_level_quote() {
    assert_eq!(
        carve::to_html("> a\n*[A]: b\n\nA\n"),
        "<blockquote><p>a</p></blockquote>\n<p><abbr title=\"b\">A</abbr></p>"
    );
}
