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
fn a_matched_span_before_any_verse_opens_no_stanza() {
    assert_eq!(
        carve::to_html(":: u\n: ::: |\n   %%%\n   %%%\nz\n").trim_end(),
        "<dl>\n  <dt>u</dt>\n  <dd>\n    <div class=\"line-block\">\n\n    </div>\n  </dd>\n</dl>\n<p>z</p>"
    );
}

#[test]
fn a_note_body_span_ends_at_the_missed_line() {
    let html = carve::to_html("[^a]: ::: |\n  x\n  %%%\n  y\nb\n\n[^a]\n");
    assert!(html.contains("<p>x<br>\nb</p>"), "{html}");
}

#[test]
fn a_description_or_quote_stanza_takes_the_missed_line() {
    assert_eq!(
        carve::to_html(":: t\n: ::: |\n  x\n  %%%\nb\n").trim_end(),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>x<br>\nb</p>\n    </div>\n  </dd>\n</dl>"
    );
    assert_eq!(
        carve::to_html("> ::: |\n> x\n> %%%\nb\n").trim_end(),
        "<blockquote>\n  <div class=\"line-block\">\n    <p>x<br>\nb</p>\n  </div>\n</blockquote>"
    );
}

#[test]
fn a_marker_after_a_quoted_span_still_ends_the_quote() {
    let html = carve::to_html("> - ::: |\n>   x\n>   %%%\n- b\n");
    assert!(
        html.trim_end()
            .ends_with("</blockquote>\n<ul>\n  <li>b</li>\n</ul>"),
        "{html}"
    );
}

#[test]
fn a_blank_ended_nested_stanza_takes_no_lazy_line() {
    let html = carve::to_html("- - ::: |\n    x\n\n    %%%\nb\n");
    assert!(html.trim_end().ends_with("</ul>\n<p>b</p>"), "{html}");
}

#[test]
fn a_closed_span_in_a_nested_stanza_keeps_the_lazy_line() {
    let html = carve::to_html("- - ::: |\n    x\n    %%%\n    y\n    %%%\nb\n");
    assert!(html.contains("<p>x<br>\nb</p>"), "{html}");
}

#[test]
fn the_block_closer_after_the_missed_line_still_closes() {
    for source in [
        ":: t\n: ::: |\n  x\n  %%%\nb\n  :::\n  q\n",
        "- ::: |\n  x\n  %%%\nb\n  :::\n  q\n",
        "> ::: |\n> x\n> %%%\nb\n> :::\n> q\n",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("<p>x<br>\nb</p>\n"), "{source:?}\n{html}");
        assert!(
            !html.contains(":::") && !html.contains("<br>\nq"),
            "{source:?}\n{html}"
        );
    }
}

#[test]
fn a_description_stanza_a_blank_ended_takes_no_line_after_a_span() {
    assert_eq!(
        carve::to_html(":: t\n: ::: |\n  x\n\n  %%%\n  y\nb\n").trim_end(),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>x</p>\n    </div>\n  </dd>\n</dl>\n<p>b</p>"
    );
}

#[test]
fn a_closed_quoted_span_opens_no_stanza() {
    for source in [
        "> ::: |\n> %%%\n> y\n> %%%\nb\n",
        "> ::: |\n> %%%\n> %%%\nb\n",
        "> ::: |\n> x\n>\n> %%%\n> y\n> %%%\nb\n",
    ] {
        let html = carve::to_html(source);
        assert!(
            html.trim_end().ends_with("</blockquote>\n<p>b</p>"),
            "{source:?}\n{html}"
        );
    }
}

#[test]
fn a_nested_item_stanza_keeps_the_line_after_any_span() {
    for source in [
        "- a\n  - ::: |\n    x\n    %%%\n    y\n    %%%\nb\n",
        "- a\n  - ::: |\n    x\n    %%%\n    %%%\nb\n",
        "- a\n  - ::: |\n    x\n    %%%\nb\n",
        "- a\n  - ::: |\n    x\n    %%%\n    y\n    %%%\n  b\n",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("<p>x<br>\nb</p>"), "{source:?}\n{html}");
    }
}

#[test]
fn an_item_in_a_description_keeps_the_body_line_after_a_blank_ended_stanza() {
    let html = carve::to_html(":: t\n: - ::: |\n    x\n\n    %%%\n    y\n  b\n");
    assert!(html.contains("</ul>\n    <p>b</p>\n  </dd>"), "{html}");
}
