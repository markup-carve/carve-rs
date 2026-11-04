//! markup-carve/carve#5132 (CARVE-P9-067): after a quote's `+` attaches a
//! paragraph written outside the prefix, `> +` continues that quote and its
//! `+` is a letter at the quote's content column.

#[test]
fn the_plus_is_a_letter_whatever_follows() {
    for (source, html) in [
        (
            "> +\nq\n> +\n",
            "<blockquote>\n  <p>q</p>\n  <p>+</p>\n</blockquote>",
        ),
        (
            "> +\nq\n> +\nr\n",
            "<blockquote>\n  <p>q</p>\n  <p>+\nr</p>\n</blockquote>",
        ),
        (
            "> +\nq\n> +\n> x\n",
            "<blockquote>\n  <p>q</p>\n  <p>+\nx</p>\n</blockquote>",
        ),
        (
            "> +\nq\n> +\n# h\n",
            "<blockquote>\n  <p>q</p>\n  <p>+</p>\n</blockquote>\n<section id=\"h\">\n  <h1>h</h1>\n</section>",
        ),
        (
            "> +\nq\n> +\n\nz\n",
            "<blockquote>\n  <p>q</p>\n  <p>+</p>\n</blockquote>\n<p>z</p>",
        ),
    ] {
        assert_eq!(carve::to_html(source), html, "{source:?}");
    }
}
