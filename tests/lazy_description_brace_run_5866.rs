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
        html(":: t\n: pre\n {% b\n  b %}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>pre\n</dd>\n</dl>"
    );
    assert_eq!(
        html(":: t\n: {% b\n    b %}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>"
    );
}
