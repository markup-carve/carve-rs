//! carve#5740 shape 1: a brace line inside an inline comment that the
//! paragraph closes later is comment text (PART 9 §21a, CARVE-P2-035).
//! Expected HTML is the spec model's.

use carve::to_html;

#[test]
fn the_comment_holds_the_brace_line() {
    for (source, expected) in [
        ("p {% a\n{lang=de}\nz %} y\n", "<p>p  y</p>"),
        ("p {% a\n{lang=de} %} y\n", "<p>p  y</p>"),
        ("p {% a\n{.x\n foo}\nz %} y\n", "<p>p  y</p>"),
        ("p {% a %} {% b\n{lang=de}\nz %} y\n", "<p>p   y</p>"),
        (
            "- p {% a\n  {lang=de}\n  z %} y\n",
            "<ul>\n  <li>p  y</li>\n</ul>",
        ),
        (
            "> p {% a\n> {lang=de}\n> z %} y\n",
            "<blockquote><p>p  y</p></blockquote>",
        ),
        (
            ":: t\n:  p {% a\n   {lang=de}\n   z %} y\n",
            "<dl>\n  <dt>t</dt>\n  <dd>p  y</dd>\n</dl>",
        ),
    ] {
        assert_eq!(to_html(source).trim_end(), expected, "{source}");
    }
}

#[test]
fn edge_shapes_match_the_spec_model() {
    for (source, expected) in [
        (
            "p {% a\n{lang=de}\n# h %}\n",
            "<p>p {% a\n{lang=de}</p>\n<section id=\"h\">\n  <h1>h %}</h1>\n</section>",
        ),
        (
            "p `{%` a\n{lang=de}\nz %} y\n",
            "<p>p <code>{%</code> a</p>\n<p lang=\"de\">z %} y</p>",
        ),
        // A lazy line below the item is held by a code span only.
        (
            "- p {% a\n{lang=de}\nz %} y\n",
            "<ul>\n  <li>p {% a</li>\n</ul>\n<p lang=\"de\">z %} y</p>",
        ),
    ] {
        assert_eq!(to_html(source).trim_end(), expected, "{source}");
    }
}
