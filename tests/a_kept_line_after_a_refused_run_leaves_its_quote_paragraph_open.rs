// CARVE-P2-035: the line after a refused brace run is paragraph text whatever it
// opens, so the paragraph stays open for a line that misses the quote (carve#5191).
#[test]
fn a_line_missing_the_quote_folds_after_a_kept_line() {
    for (source, html) in [
        (
            "> a\n> {x\n> y}\n> # c\nmore\n",
            "<blockquote>\n  <p>a\n{x\ny}</p>\n  <h1 id=\"c\">c</h1>\n</blockquote>\n<p>more</p>",
        ),
        (
            "> > a\n> > {.a\n> > #b}\n> > - c\nmore\n",
            "<blockquote>\n  <blockquote><p>a\n{.a\n<span class=\"tag\"><strong>#b</strong></span>}\n- c\nmore</p></blockquote>\n</blockquote>",
        ),
        (
            "> a\n> {% c\n> d %}\n> ::: d\nmore\n",
            "<blockquote>\n  <p>a\n{% c\nd %}</p>\n  <div class=\"d\">\n\n  </div>\n</blockquote>\n<p>more</p>",
        ),
        (
            "> a\n> {x\n> y}\n> ```\nmore\n",
            "<blockquote>\n  <p>a\n{x\ny}</p>\n  <pre><code></code></pre>\n</blockquote>\n<p>more</p>",
        ),
        (
            "> a\n> {x\n> y}\n> ~~~ js\n> z\nmore\n",
            "<blockquote>\n  <p>a\n{x\ny}</p>\n  <pre><code class=\"language-js\">z\n</code></pre>\n</blockquote>\n<p>more</p>",
        ),
        (
            "> a\n> {x\n> y}\n>\nmore\n",
            "<blockquote><p>a\n{x\ny}</p></blockquote>\n<p>more</p>",
        ),
        (
            "- a\n  {x\n  y}\n  # c\nmore\n",
            "<ul>\n  <li>a\n{x\ny}\n    <h1 id=\"c\">c</h1>\n  </li>\n</ul>\n<p>more</p>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim(), html, "{source}");
    }
}
