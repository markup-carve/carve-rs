use carve::to_html;

#[test]
fn an_erased_span_leaves_the_quoted_description_stanza_open_for_lazy_text() {
    assert_eq!(
        to_html("> - :: t\n>   : ::: |\n>     x\n>     %%%\n: b\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>t</dt>\n        <dd>\n          <div class=\"line-block\">\n            <p>x<br>\n: b</p>\n          </div>\n        </dd>\n      </dl>\n    </li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn a_marked_quote_follower_still_starts_a_quote_paragraph() {
    assert_eq!(
        to_html("> - ::: |\n>   x\n>   %%%\n>\n> c\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <div class=\"line-block\">\n        <p>x</p>\n      </div>\n    </li>\n  </ul>\n  <p>c</p>\n</blockquote>"
    );
}
