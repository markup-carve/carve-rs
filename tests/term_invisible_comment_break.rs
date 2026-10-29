#[test]
fn below_column_comment_keeps_the_term_break() {
    assert_eq!(
        carve::to_html("- :: a\n %%"),
        "<ul>\n  <li>\n    <dl>\n      <dt>a</dt>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn term_continuation_after_comment_keeps_both_breaks() {
    assert_eq!(
        carve::to_html("- :: a\n %%\n  b"),
        "<ul>\n  <li>\n    <dl>\n      <dt>a</dt>\n    </dl>\n  </li>\n</ul>\n<p>b</p>"
    );
}
