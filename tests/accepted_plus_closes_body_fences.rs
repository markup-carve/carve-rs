#[test]
fn description_plus_closes_its_colon_container() {
    assert_eq!(
        carve::to_html(":: t\n: :::\n+\n> q\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div>\n\n    </div>\n    <blockquote><p>q</p></blockquote>\n  </dd>\n</dl>"
    );
    assert_eq!(
        carve::to_html(":: t\n: :::: n\n+\n> q\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"n\">\n\n    </div>\n    <blockquote><p>q</p></blockquote>\n  </dd>\n</dl>"
    );
}

#[test]
fn note_plus_closes_its_colon_container() {
    assert_eq!(
        carve::to_html("r[^n]\n\n[^n]: :::\n+\n> q\n"),
        "<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <div>\n\n      </div>\n      <blockquote><p>q</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn note_plus_also_closes_an_open_code_fence() {
    assert_eq!(
        carve::to_html("r[^n]\n\n[^n]: ```\n+\n> q\n"),
        "<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code></code></pre>\n      <blockquote><p>q</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
