fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn lazy_malformed_colon_line_keeps_its_quote_paragraph_open() {
    assert_eq!(
        html("> e\n:::e\n> :::\nl\n"),
        "<blockquote><p>e\n:::e\n:::\nl</p></blockquote>"
    );
}

#[test]
fn an_outer_quote_line_cannot_absorb_for_the_inner_paragraph() {
    assert_eq!(
        html("> > e\n> :::e\n> :::\nl\n"),
        "<blockquote>\n  <blockquote><p>e\n:::e</p></blockquote>\n  <div>\n\n  </div>\n</blockquote>\n<p>l</p>"
    );
}

#[test]
fn an_inner_quote_line_can_absorb_for_the_inner_paragraph() {
    assert_eq!(
        html("> > e\n> :::e\n> > :::\nl\n"),
        "<blockquote>\n  <blockquote><p>e\n:::e\n:::\nl</p></blockquote>\n</blockquote>"
    );
}

#[test]
fn a_lazily_framed_colon_line_survives_reparse_at_the_inner_quote() {
    assert_eq!(
        html("> > e\n:::e\n> > :::\nl\n"),
        "<blockquote>\n  <blockquote><p>e\n:::e\n:::\nl</p></blockquote>\n</blockquote>"
    );
}

#[test]
fn an_intervening_fence_ends_the_absorbing_paragraph() {
    assert_eq!(
        html("> e\n> :::e\n> ```\n> code\n> ```\n> :::\nl\n"),
        "<blockquote>\n  <p>e\n:::e</p>\n  <pre><code>code\n</code></pre>\n  <div>\n\n  </div>\n</blockquote>\n<p>l</p>"
    );
}

#[test]
fn a_blank_ends_the_old_paragraph_depth_before_a_new_outer_paragraph() {
    assert_eq!(
        html("> > a:b\n>\n> x\n> :::e\n> :::\nl\n"),
        "<blockquote>\n  <blockquote><p>a:b</p></blockquote>\n  <p>x\n:::e\n:::\nl</p>\n</blockquote>"
    );
}
