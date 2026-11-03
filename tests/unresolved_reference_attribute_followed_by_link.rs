//! A reference's valid attribute run ends before the next independent bracket run.

#[test]
fn a_following_empty_link_pairs_after_unresolved_reference_attributes() {
    for (source, expected) in [
        ("[][]{#i}[#}]()", "<p>[][]{#i}<a href=\"\">#}</a></p>"),
        (
            "[][]{v}{#i}[#}]()",
            "<p>[][]{v}{#i}<a href=\"\">#}</a></p>",
        ),
        (
            "before [][]{#i}[#}]() after",
            "<p>before [][]{#i}<a href=\"\">#}</a> after</p>",
        ),
        (
            "[x](u){#i}[#}]()",
            "<p><a href=\"u\" id=\"i\">x</a><a href=\"\">#}</a></p>",
        ),
        (
            "![a](u){#i}[#}]()",
            "<p><img src=\"u\" alt=\"a\" id=\"i\"><a href=\"\">#}</a></p>",
        ),
        (
            "*x*{.c}{#i}[#}]()",
            "<p><strong class=\"c\" id=\"i\">x</strong><a href=\"\">#}</a></p>",
        ),
        (
            "`x`{.c}{#i}[#}]()",
            "<p><code class=\"c\" id=\"i\">x</code><a href=\"\">#}</a></p>",
        ),
        (
            "{+x+}{.c}{#i}[#}]()",
            "<p><ins class=\"c\" id=\"i\">x</ins><a href=\"\">#}</a></p>",
        ),
        (
            "![a][missing]{v}{#i}[#}]()",
            "<p>![a][missing]{v}{#i}<a href=\"\">#}</a></p>",
        ),
        (
            "::: note\n[][]{#i}[#}]()\n:::",
            "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>[][]{#i}<a href=\"\">#}</a></p>\n</aside>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn an_editorial_comment_inside_a_link_label_stays_opaque() {
    for (source, expected) in [
        (
            "[{#i}x]#}y](/u)",
            "<p><a href=\"/u\"><span class=\"critic-comment\">i}x]</span>y</a></p>",
        ),
        (
            "[x }{#i}x]#}y](/u)",
            "<p><a href=\"/u\">x }<span class=\"critic-comment\">i}x]</span>y</a></p>",
        ),
        (
            "[a {.c}{#i}x]#}y](/u)",
            "<p><a href=\"/u\">a {.c}<span class=\"critic-comment\">i}x]</span>y</a></p>",
        ),
        (
            "[a*{.c}{#i}x]#}y](/u)",
            "<p><a href=\"/u\">a*{.c}<span class=\"critic-comment\">i}x]</span>y</a></p>",
        ),
        (
            "[é*x*{#i}x]#}y](/u)",
            "<p><a href=\"/u\">é*x*<span class=\"critic-comment\">i}x]</span>y</a></p>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn a_comment_inside_image_alt_stays_opaque_after_a_nested_bracket() {
    let source = "![a [b]{#i}x]#}y](u)";
    assert_eq!(
        carve::to_html(source).trim(),
        "<img src=\"u\" alt=\"a [b]{#i}x]#}y\">"
    );
}

#[test]
fn unresolved_image_attributes_keep_logical_table_cell_escapes() {
    let source = "|= H |\n| ![a][missing]{.x}{title=\"a\\|b\"} |";
    assert_eq!(
        carve::to_html(source).trim(),
        "<table>\n  <thead>\n    <tr><th scope=\"col\">H</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>![a][missing]{.x}{title=\"a|b\"}</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn quoted_attribute_bracket_does_not_hide_a_reference_label_close() {
    let source = "a [t[x][r[n]{title=\"]\"}]](/u) b";
    let expected = "<p>a [t[x][r[n]{title=\"]\"}]](/u) b</p>";
    assert_eq!(carve::to_html(source).trim(), expected);
    assert_eq!(carve::to_html(&carve::to_carve(source)).trim(), expected);
}
