use carve::to_html;

#[test]
fn a_list_marker_ends_a_quoted_description_after_a_blank_inside_its_stanza() {
    assert_eq!(
        to_html("> :: t\n> : ::: |\n>   x\n>\n>   y\n- b\n"),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <div class=\"line-block\">\n        <p>x</p>\n        <p>y</p>\n      </div>\n    </dd>\n  </dl>\n</blockquote>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn a_list_marker_folds_into_a_quoted_stanza_after_an_open_span() {
    assert_eq!(
        to_html("> ::: |\n> x\n> %%%\n- b\n"),
        "<blockquote>\n  <div class=\"line-block\">\n    <p>x<br>\n- b</p>\n  </div>\n</blockquote>"
    );
}

#[test]
fn a_list_marker_after_an_open_span_still_ends_a_quoted_description() {
    assert_eq!(
        to_html("> :: t\n> : ::: |\n>   x\n>   %%%\n- b\n"),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <div class=\"line-block\">\n        <p>x</p>\n      </div>\n    </dd>\n  </dl>\n</blockquote>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn a_description_after_a_closed_span_and_more_verse_is_a_sibling() {
    assert_eq!(
        to_html(":: t\n: ::: |\n  x\n\n  %%% c\n  %%%\n  y\n: d\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>x</p>\n      <p>y</p>\n    </div>\n  </dd>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn an_unquoted_description_after_an_open_span_is_a_sibling() {
    assert_eq!(
        to_html(":: t\n: ::: |\n  x\n  %%%\n: d\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>x</p>\n    </div>\n  </dd>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn a_lazy_description_keeps_its_stanza_when_the_quote_resumes() {
    assert_eq!(
        to_html("> :: t\n> : ::: |\n>   x\n>   %%%\n: d\n> after\n"),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <div class=\"line-block\">\n        <p>x<br>\n: d<br>\nafter</p>\n      </div>\n    </dd>\n  </dl>\n</blockquote>"
    );
}

#[test]
fn a_lazy_line_ends_the_span_before_a_blank() {
    assert_eq!(
        to_html(":: t\n: ::: |\n  x\n  %%%\nb\n\nafter\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>x<br>\nb</p>\n    </div>\n  </dd>\n</dl>\n<p>after</p>"
    );
}

#[test]
fn a_footnote_definition_ends_an_item_stanza() {
    assert_eq!(
        to_html("- ::: |\n  x\n[^m]: n\n\n[^m]\n"),
        "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>x</p>\n    </div>\n  </li>\n</ul>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_footnote_definition_ends_a_quoted_stanza() {
    assert_eq!(
        to_html("> ::: |\n> x\n[^m]: n\n\n[^m]\n"),
        "<blockquote>\n  <div class=\"line-block\">\n    <p>x</p>\n  </div>\n</blockquote>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_footnote_definition_inside_a_root_line_block_is_verse() {
    assert_eq!(
        to_html("::: |\nx\n[^m]: n\n:::\n"),
        "<div class=\"line-block\">\n  <p>x<br>\n[^m]: n</p>\n</div>"
    );
}
