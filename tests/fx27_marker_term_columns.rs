#[test]
fn a_separator_at_a_nested_marker_column_returns_to_the_outer_item() {
    for source in [
        "- - :: t\n  : a",
        "- 1. :: t\n  : a",
        "1. - :: t\n   : a",
        "* - a.  :: t\n    : a",
        "> - - :: t\n>   : a",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("<dt>t</dt>"), "{source:?}: {html}");
        assert!(!html.contains("<dd>a</dd>"), "{source:?}: {html}");
        let term_end = html.find("</dt>").unwrap();
        let follower = html.find(": a").unwrap();
        assert!(follower > term_end, "{source:?}: {html}");
        assert!(
            html[term_end..follower].contains("</ul>")
                || html[term_end..follower].contains("</ol>"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn a_past_column_footnote_definition_leaves_the_description() {
    let source = "- a\n  :: outer\n  :  detail\n       :: inner\n      :  tail\n        [^n]: note\n         q";
    let html = carve::to_html(source);
    assert!(!html.contains("[^n]: note"), "{html}");
    assert!(html.contains("<dd>tail</dd>"), "{html}");
    assert!(!html.contains("q</"), "{html}");
    assert!(!html.contains("class=\"footnotes\""), "{html}");
}

#[test]
fn a_past_column_link_definition_registers() {
    let source = "[x][r]\n\n- a\n  :: outer\n  :  detail\n       :: inner\n      :  tail\n        [r]: /u\n         q";
    let html = carve::to_html(source);
    assert!(!html.contains("[r]: /u"), "{html}");
    assert!(html.contains("<p><a href=\"/u\">x</a></p>"), "{html}");
    assert!(html.contains("<dd>tail\nq</dd>"), "{html}");
}
