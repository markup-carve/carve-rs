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
    let source = "[{#i}x]#}y](/u)";
    assert_eq!(
        carve::to_html(source).trim(),
        "<p><a href=\"/u\"><span class=\"critic-comment\">i}x]</span>y</a></p>"
    );
}
