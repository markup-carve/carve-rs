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

    // Shape B of carve#5748 awaits a ruling. Its continuation is at the
    // note body column, left of the opening brace's source column.
    let parked = "[^n]: {% a\n    {k=v}\n    z %} y\n\nr[^n]\n";
    let html = to_html(parked);
    assert!(html.contains("<p>{% a\n{k=v}\nz %} y"), "{html}");
}
