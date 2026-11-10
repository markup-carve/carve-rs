use carve::to_html;

#[test]
fn a_quoted_plus_after_a_refused_run_is_paragraph_text_in_each_host() {
    let cases = [
        (
            "> - {.x\n> }\n> +\n> z\n",
            "<blockquote>\n  <ul>\n    <li>{.x\n}\n+\nz</li>\n  </ul>\n</blockquote>",
        ),
        (
            "> :: t\n> : {.x\n> }\n> +\n> z\n",
            "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>{.x\n}\n+\nz</dd>\n  </dl>\n</blockquote>",
        ),
        (
            "> [^1]: {.x\n> }\n> +\n> z\n\nx[^1]\n",
            "<blockquote>\n\n</blockquote>\n<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n}\n+\nz<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
        (
            "> - {% a\n>  %}\n> +\nz\n",
            "<blockquote>\n  <ul>\n    <li>{% a\n%}\n+\nz</li>\n  </ul>\n</blockquote>",
        ),
        (
            "> - {.x\n}\n> +\nz\n",
            "<blockquote>\n  <ul>\n    <li>{.x\n}\n+\nz</li>\n  </ul>\n</blockquote>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

#[test]
fn a_brace_line_inside_an_open_inline_span_does_not_refuse_a_block_run() {
    let cases = [
        (
            "> - a\n> {.x\n> }\n> +\n> z\n",
            "<blockquote>\n  <ul>\n    <li>a\n{.x\n}\n      z\n    </li>\n  </ul>\n</blockquote>",
        ),
        (
            "> - `a\n> {.x\n> }\n> +\n> z\n",
            "<blockquote>\n  <ul>\n    <li><code>a\n{.x\n}</code>\n      z\n    </li>\n  </ul>\n</blockquote>",
        ),
        (
            "> - [a\n> {.x\n> }\n> +\n> z\n",
            "<blockquote>\n  <ul>\n    <li>[a\n{.x\n}\n      z\n    </li>\n  </ul>\n</blockquote>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

#[test]
fn an_outdented_quoted_heading_closes_the_refused_runs_item_paragraph() {
    for (opener, rendered) in [("{.x\n> }", "{.x\n}"), ("{% a\n>  %}", "{% a\n%}")] {
        let source = format!("> - {opener}\n> # h\nz\n");
        let expected = format!(
            "<blockquote>\n  <ul>\n    <li>{}</li>\n  </ul>\n  <h1 id=\"h\">h</h1>\n</blockquote>\n<p>z</p>",
            rendered
        );
        assert_eq!(to_html(&source), expected, "{source:?}");
    }
}
