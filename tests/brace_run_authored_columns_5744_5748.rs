use carve::to_html;

#[test]
fn block_comment_continuations_use_authored_columns_after_a_brace_line() {
    for brace in ["{lang=de}", "{.x}", "{#i}", "{k=v}"] {
        for indent in [1, 2] {
            let pad = " ".repeat(indent);
            let source = format!("{{% a\n{pad}{brace}\n{pad}z %}} y\n");
            assert_eq!(to_html(&source).trim(), "<p> y</p>", "{source:?}");
        }
    }
    assert_eq!(to_html("\t{% a\n     z %} y\n").trim(), "<p> y</p>");
    assert_eq!(to_html("    {% a\n     z %} y\n").trim(), "<p> y</p>");
}

#[test]
fn note_marker_comment_continues_only_past_its_opening_brace() {
    let past = "[^n]: {% a\n       {k=v}\n       z %} y\n\nr[^n]\n";
    let html = to_html(past);
    assert!(html.contains("<p> y<a href=\"#fnref1\""), "{html}");
    assert!(!html.contains("{% a"), "{html}");

    // The carve#5748 ruling measures a marker-line run from the body column.
    let body_column = "[^n]: {% a\n    {k=v}\n    z %} y\n\nr[^n]\n";
    let html = to_html(body_column);
    assert!(html.contains("<p> y<a href=\"#fnref1\""), "{html}");
    assert!(!html.contains("{% a"), "{html}");
}

#[test]
fn note_marker_brace_column_does_not_grow_with_its_label_or_quote_prefix() {
    for source in [
        "[^long-label]: {% a\n    {k=v}\n    z %} y\n\nr[^long-label]\n",
        "> [^n]: {% a\n>     {k=v}\n>     z %} y\n\n> r[^n]\n",
    ] {
        let html = to_html(source);
        assert!(
            html.contains("<p> y<a href=\"#fnref1\""),
            "{source:?}\n{html}"
        );
        assert!(!html.contains("{% a"), "{source:?}\n{html}");
    }
}
