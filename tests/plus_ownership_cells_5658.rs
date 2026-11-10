//! Ownership cells of carve#5658 ported from the spec model (carve#5716).

#[test]
fn a_refused_run_follower_keeps_an_indented_closer_past_a_quote() {
    // carve#5600
    let source = "{.x\n.y}\n```\nwa\n> q\n  ```\n";
    let html = "<p>{.x\n.y}</p>\n<pre><code>wa\n&gt; q\n  ```\n</code></pre>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_note_keeps_the_line_after_a_refused_marker_ends_its_list_fence() {
    // carve#5601
    let source = "[^a]: +\n- li\n  ```\n+\n  p\n\nsee[^a]\n";
    let html = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <pre><code></code></pre>\n        </li>\n      </ul>\n      <p>p<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_quoted_note_lazy_line_finishes_an_attribute_run() {
    // carve#5619
    let source = "> [^a]: +\n> {#i\n .y}\n\nsee[^a]\n";
    let html = "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{<span class=\"tag\"><strong>#i</strong></span>\n.y}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn an_own_line_item_fence_keeps_its_first_blank_and_tail() {
    // carve#5635
    let source = "- p\n+\n```\ncode\n\n  tail\n\nsee\n";
    let html =
        "<ul>\n  <li>p\n    <pre><code>code\n\n  tail\n\n</code></pre>\n  </li>\n</ul>\n<p>see</p>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_refused_item_marker_keeps_the_follower_in_the_item() {
    // carve#5637
    let source = "- ```\n+\n  x\n";
    let html = "<ul>\n  <li>\n    <pre><code></code></pre>\n    x\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_refused_item_marker_closes_the_open_second_fence() {
    // carve#5637
    let source = "- ```\n  ```\n  ~~~\n+\n  x\n";
    let html = "<ul>\n  <li>\n    <pre><code></code></pre>\n    <pre><code></code></pre>\n    x\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_description_plus_after_a_blank_reaches_its_host() {
    // carve#5639
    let source = ":: t\n: h\n\n+\npp\n";
    let html = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>h</p>\n    <p>pp</p>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source).trim(), html);
}

#[test]
fn a_note_plus_after_a_blank_reaches_its_host() {
    // carve#5639
    let source = "[^n]: h\n\n+\npp\n\nsee[^n]\n";
    let html = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>h</p>\n      <p>pp<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), html);
}
