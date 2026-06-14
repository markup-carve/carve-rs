//! A thematic break is 3+ of the same `-`/`*`/`_`, optionally separated by
//! spaces/tabs, with nothing else on the line. Matches carve-js, carve-php,
//! and canonical djot. A mixed run (`-*-`) or fewer than three is not a break.

#[test]
fn spaced_and_consecutive_runs_are_breaks() {
    for src in [
        "---", "***", "___", "- - -", "* * *", "_ _ _", "- - - -", "-  -  -",
    ] {
        assert_eq!(
            carve::to_html(src),
            "<hr>",
            "{src:?} should be a thematic break"
        );
    }
}

#[test]
fn mixed_or_short_runs_are_not_breaks() {
    assert_eq!(carve::to_html("-*-"), "<p>-*-</p>");
    assert_eq!(carve::to_html("- x"), "<ul>\n  <li>x</li>\n</ul>");
}

#[test]
fn a_spaced_break_interrupts_a_paragraph_and_heading() {
    assert_eq!(carve::to_html("para\n- - -"), "<p>para</p>\n<hr>");
    assert_eq!(
        carve::to_html("# H\n- - -"),
        "<section id=\"h\">\n  <h1>H</h1>\n  <hr>\n</section>"
    );
}
