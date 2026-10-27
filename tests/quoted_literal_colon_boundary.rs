#[test]
fn literal_inner_colon_run_does_not_extend_outer_div() {
    assert_eq!(
        carve::to_html("> > :::note\n> :::\ntail"),
        "<blockquote>\n  <blockquote><p>:::note</p></blockquote>\n  <div>\n\n  </div>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn depth_drop_resets_literal_colon_absorption() {
    assert_eq!(
        carve::to_html("> > > :::note\n> > :::\n> tail"),
        "<blockquote>\n  <blockquote>\n    <blockquote><p>:::note</p></blockquote>\n    <div>\n\n    </div>\n  </blockquote>\n  <p>tail</p>\n</blockquote>"
    );
}
