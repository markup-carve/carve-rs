// carve#4956: a `%%%` span in a line block leaves nothing. A line that misses the
// span's host ends it (S1) and then folds only if a stanza with content was open
// before the span (S4).

#[test]
fn a_missed_span_folds_the_line_into_the_open_stanza() {
    let folded = "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>x<br>\nb</p>\n    </div>\n  </li>\n</ul>";
    for source in [
        "- ::: |\n  x\n  %%%\nb\n",
        "- ::: |\n  x\n  %%%\n  y\nb\n",
        "- ::: |\n  x\n  %%%\n  y\n  %%%\nb\n",
    ] {
        assert_eq!(carve::to_html(source).trim_end(), folded, "{source:?}");
    }
    assert_eq!(
        carve::to_html("- - ::: |\n    x\n    %%%\nb\n").trim_end(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x<br>\nb</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_span_after_a_blank_opens_no_stanza() {
    assert_eq!(
        carve::to_html("- ::: |\n  x\n\n  %%%\nb\n").trim_end(),
        "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>x</p>\n    </div>\n  </li>\n</ul>\n<p>b</p>"
    );
}

#[test]
fn a_note_body_span_ends_at_the_missed_line() {
    let html = carve::to_html("[^a]: ::: |\n  x\n  %%%\n  y\nb\n\n[^a]\n");
    assert!(html.contains("<p>x<br>\nb</p>"), "{html}");
}
