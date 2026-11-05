// CARVE-P2-035: the line after a refused brace run is paragraph text whatever it
// opens, so the paragraph stays open for a line that misses the quote (carve#5191).
#[test]
fn a_line_missing_the_quote_folds_after_a_kept_line() {
    for (source, html) in [
        (
            "> a\n> {x\n> y}\n> # c\nmore\n",
            "<blockquote><p>a\n{x\ny}\n# c\nmore</p></blockquote>",
        ),
        (
            "> > a\n> > {.a\n> > #b}\n> > - c\nmore\n",
            "<blockquote>\n  <blockquote><p>a\n{.a\n#b}\n- c\nmore</p></blockquote>\n</blockquote>",
        ),
        (
            "> a\n> {% c\n> d %}\n> ::: d\nmore\n",
            "<blockquote><p>a\n{% c\nd %}\n::: d\nmore</p></blockquote>",
        ),
        (
            "> a\n> {x\n> y}\n> ```\nmore\n",
            "<blockquote><p>a\n{x\ny}\n<code>\nmore</code></p></blockquote>",
        ),
        (
            "> a\n> {x\n> y}\n> ~~~ js\n> z\nmore\n",
            "<blockquote><p>a\n{x\ny}\n~~~ js\nz\nmore</p></blockquote>",
        ),
        (
            "- a\n  {x\n  y}\n  # c\nmore\n",
            "<ul>\n  <li>a\n{x\ny}\n# c\nmore</li>\n</ul>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim(), html, "{source}");
    }
}
