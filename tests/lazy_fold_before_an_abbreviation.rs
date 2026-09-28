//! A line folding into a container's inline content is not a place §7
//! recognizes an abbreviation definition (markup-carve/carve-rs#2096).
//!
//! Expectations are the executable spec's reading at markup-carve/carve
//! 38829a97. The last two cases are the controls: at the document level the
//! definition still interrupts a paragraph and still ends a quote.

#[test]
fn an_abbreviation_folds_into_a_quote_in_an_item() {
    assert_eq!(
        carve::to_html("- >~\n*[A]: b\n"),
        "<ul>\n  <li>&gt;~\n*[A]: b</li>\n</ul>"
    );
}

#[test]
fn an_abbreviation_folds_into_a_term_on_a_marker_line() {
    assert_eq!(
        carve::to_html(". :: t\n*[A]: b\n"),
        "<ol>\n  <li>\n    <dl>\n      <dt>t\n*[A]: b</dt>\n    </dl>\n  </li>\n</ol>"
    );
}

#[test]
fn an_abbreviation_folds_into_a_top_level_term() {
    assert_eq!(
        carve::to_html(":: t\n*[A]: b\n\nA\n"),
        "<dl>\n  <dt>t\n*[A]: b</dt>\n</dl>\n<p>A</p>"
    );
}

#[test]
fn an_abbreviation_folds_into_a_real_quote_in_an_item() {
    assert_eq!(
        carve::to_html("- > q\n*[A]: b\n"),
        "<ul>\n  <li>\n    <blockquote><p>q\n*[A]: b</p></blockquote>\n  </li>\n</ul>"
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
