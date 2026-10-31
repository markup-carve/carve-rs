#[test]
fn a_plus_after_a_blank_ends_a_description_fence() {
    let source = ":: t\n: ```\n\n+\nx\n";
    let expected =
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<p>+\nx</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_plus_ends_an_unclosed_description_fence_before_its_block() {
    let source = ":: t\n: ```\n+\nx\n";
    let expected =
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code></code></pre>\n    <p>x</p>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_second_lazy_line_continues_a_term_two_quotes_deep() {
    let source = "> > :: m\nl\nt\n";
    let expected = "<blockquote>\n  <blockquote>\n    <dl>\n      <dt>m\nl\nt</dt>\n    </dl>\n  </blockquote>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_lazy_run_continues_a_term_three_quotes_deep() {
    let source = "> > > :: m\nl\nt\nu\n";
    let expected = "<blockquote>\n  <blockquote>\n    <blockquote>\n      <dl>\n        <dt>m\nl\nt\nu</dt>\n      </dl>\n    </blockquote>\n  </blockquote>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_comment_at_the_item_column_closes_a_marker_line_note() {
    let source = "[^c]\n\n- [^c]: para\n  %% c\n    tail\n";
    let expected = "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<ul>\n  <li>tail</li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_comment_at_a_quoted_item_column_closes_a_marker_line_note() {
    let source = "> [^c]\n>\n> - [^c]: para\n>   %% c\n>     tail\n";
    let expected = "<blockquote>\n  <p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n  <ul>\n    <li>tail</li>\n  </ul>\n</blockquote>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}
