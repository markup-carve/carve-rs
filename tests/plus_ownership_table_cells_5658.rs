//! Generated ownership-table cells of carve#5658 that carve-rs read differently from the spec model.

#[test]
fn a_description_plus_after_a_blank_reaches_its_host_past_an_attached_paragraph() {
    let source = ":: t\n: p\n+\na\n\n+\n  zq\n";
    let html =
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>p</p>\n    <p>a</p>\n    <p>zq</p>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_description_plus_after_a_blank_reaches_its_host_past_an_attached_table() {
    let source = ":: t\n: p\n+\n| a |\n| b |\n\n+\n  zq\n";
    let html = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>p</p>\n    <table>\n      <tbody>\n        <tr><td>a</td></tr>\n        <tr><td>b</td></tr>\n      </tbody>\n    </table>\n    <p>zq</p>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_note_plus_after_a_blank_reaches_its_host_past_an_attached_paragraph() {
    let source = "see[^a]\n\n[^a]: p\n+\na\n\n+\n  zq\n";
    let html = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>p</p>\n      <p>a</p>\n      <p>zq<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_note_plus_after_a_blank_reaches_its_host_past_a_closed_fence() {
    let source = "see[^a]\n\n[^a]: p\n+\n```\na\n```\n\n+\n  zq\n";
    let html = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>p</p>\n      <pre><code>a\n</code></pre>\n      <p>zq<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_quote_line_after_a_stacked_attached_paragraph_resumes_the_quote() {
    let source = "- > +\n  a\n  >   zq\n";
    let html = "<ul>\n  <li>\n    <blockquote>\n      <p>a</p>\n      <p>zq</p>\n    </blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_quote_line_after_a_stacked_attached_fence_resumes_the_quote() {
    let source = "- > +\n  ```\n  a\n  ```\n  >   zq\n";
    let html = "<ul>\n  <li>\n    <blockquote>\n      <pre><code>a\n</code></pre>\n      <p>zq</p>\n    </blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_quote_line_after_a_stacked_attached_table_resumes_the_quote() {
    let source = "- > +\n  | a |\n  | b |\n  >   zq\n";
    let html = "<ul>\n  <li>\n    <blockquote>\n      <table>\n        <tbody>\n          <tr><td>a</td></tr>\n          <tr><td>b</td></tr>\n        </tbody>\n      </table>\n      <p>zq</p>\n    </blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_quote_line_after_a_stacked_refused_brace_run_resumes_the_quote() {
    let source = "- > +\n  {.x\n  .y}\n  >   zq\n";
    let html = "<ul>\n  <li>\n    <blockquote>\n      <p>{.x\n.y}</p>\n      <p>zq</p>\n    </blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), html);
}
