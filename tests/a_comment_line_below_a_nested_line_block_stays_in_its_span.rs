//! A comment-shaped line below a nested line block's column inside its open
//! `%%%` span is span payload; the line after the span reads the stanza as it
//! stood before it (PART 0 S1, PART 9 §28, CARVE-P9-047, carve#5021).

#[test]
fn the_line_after_the_span_is_a_document_paragraph_without_verse() {
    for source in [
        "- - ::: |\n    %%%\n%%%%\n    %%%\nz\n",
        "- x\n  - ::: |\n    %%%\n%% c\n    %%%\nz\n",
        "- :: t\n  : ::: |\n    %%%\n%%%%\n    %%%\nz\n",
        "- x\n  - y\n    %%%\n%% c\n    %%%\nz\n",
    ] {
        let html = carve::to_html(source);
        assert!(
            html.trim_end().ends_with("<p>z</p>"),
            "{source:?} -> {html}"
        );
    }
}

#[test]
fn the_line_after_the_span_folds_into_the_verse() {
    for column in 0..=4 {
        let source = format!(
            "- - ::: |\n    a\n    %%%\n{}%%%%\n    %%%\nz\n",
            " ".repeat(column)
        );
        let html = carve::to_html(&source);
        assert!(html.contains("<p>a<br>\nz</p>"), "{source:?} -> {html}");
    }
}
