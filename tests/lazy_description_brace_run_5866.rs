// CARVE-P2-035 and carve#5830: a multi-line block brace run takes no lazy line.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_owned()
}

#[test]
fn a_lazy_comment_opener_in_a_description_is_literal() {
    assert_eq!(
        html(":: t\n: pre\n{% b\n b %}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>pre\n{% b\nb %}</dd>\n</dl>"
    );
    assert_eq!(
        html(":: t\n: pre\n{% b\n b %}\nnext\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>pre\n{% b\nb %}\nnext</dd>\n</dl>"
    );
}

#[test]
fn an_owned_comment_run_still_hides_its_lines() {
    assert_eq!(
        html(":: t\n: pre\n{% b\n  b %}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>pre\n</dd>\n</dl>"
    );
    assert_eq!(
        html(":: t\n: pre\n {% b\n  b %}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>pre\n</dd>\n</dl>"
    );
    assert_eq!(
        html(":: t\n: {% b\n    b %}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>"
    );
}

#[test]
fn a_nested_item_uses_its_content_column_for_the_run() {
    assert_eq!(
        html("- a\n  - pre\n  {% b\n   b %}\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>pre\n{% b\nb %}</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("- a\n  - pre\n  {% b\n    b %}\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>pre\n</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_quoted_description_uses_its_body_column_after_the_prefix() {
    assert_eq!(
        html("> :: t\n> : pre\n> {% b\n>  b %}\n"),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>pre\n{% b\nb %}</dd>\n  </dl>\n</blockquote>"
    );
    assert_eq!(
        html("> :: t\n> : pre\n> {% b\n>   b %}\n"),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>pre\n</dd>\n  </dl>\n</blockquote>"
    );
}
