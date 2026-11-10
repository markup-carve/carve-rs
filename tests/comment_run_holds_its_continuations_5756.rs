use carve::to_html;

#[test]
fn item_comment_run_keeps_a_brace_line_past_its_opener() {
    for brace in ["{lang=de}", "{.x}", "{#i}"] {
        for (marker, pad) in [("- ", "   "), ("1. ", "    "), ("> - ", "    ")] {
            let quote = if marker.starts_with('>') { "> " } else { "" };
            let source = format!("{marker}{{% a\n{quote}{pad}{brace}\n{quote}{pad}z %}} y\n");
            let html = to_html(&source);
            assert!(html.contains("<li> y</li>"), "{source:?}\n{html}");
        }
        let source = format!("- p\n  {{% a\n   {brace}\n   z %}} y\n");
        assert_eq!(
            to_html(&source).trim(),
            "<ul>\n  <li>p\n y</li>\n</ul>",
            "{source:?}"
        );
    }
}

#[test]
fn item_comment_run_at_its_opener_column_stays_refused() {
    let html = to_html("- {% a\n  {lang=de}\n  z %} y\n");
    assert!(html.contains("<li>{% a\n{lang=de}\nz %} y</li>"), "{html}");
}

#[test]
fn block_comment_run_holds_block_shaped_continuations() {
    for middle in ["# h", "```", "- m"] {
        for source in [
            format!("{{% a\n {middle}\n z %}} y\n"),
            format!("p\n{{% a\n {middle}\n z %}} y\n"),
            format!("> p\n> {{% a\n>  {middle}\n>  z %}} y\n"),
        ] {
            let html = to_html(&source);
            assert!(html.contains(" y</p>"), "{source:?}\n{html}");
            assert!(!html.contains("<h1") && !html.contains("<pre"), "{source:?}\n{html}");
            assert!(!html.contains("z %}"), "{source:?}\n{html}");
        }
    }
}

#[test]
fn item_comment_run_holds_a_heading_or_fence() {
    for middle in ["# h", "```"] {
        let source = format!("- {{% a\n   {middle}\n   z %}} y\n");
        assert_eq!(to_html(&source).trim(), "<ul>\n  <li> y</li>\n</ul>", "{source:?}");
        let source = format!("- p\n  {{% a\n   {middle}\n   z %}} y\n");
        assert_eq!(
            to_html(&source).trim(),
            "<ul>\n  <li>p\n y</li>\n</ul>",
            "{source:?}"
        );
    }
}

#[test]
fn a_list_marker_still_opens_its_item_inside_the_run() {
    let html = to_html("- p\n  {% a\n   - m\n   z %} y\n");
    assert!(html.contains("<li>m\nz %} y</li>"), "{html}");
}

#[test]
fn a_comment_line_inside_a_refused_run_stays_invisible() {
    let html = to_html(":: t\n: p\n  {% a\n  %% c\n  z %} y\n");
    assert!(html.contains("<dd>p\n{% a\nz %} y</dd>"), "{html}");
}

#[test]
fn inline_comment_does_not_hold_a_heading() {
    let html = to_html("p {% a\n# h\nz %} y\n");
    assert!(html.contains("<h1>h</h1>"), "{html}");
}
