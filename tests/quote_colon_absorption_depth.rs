fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn lazy_malformed_colon_line_keeps_its_quote_paragraph_open() {
    // carve#4863 ruling: a colon line is read on its own.
    assert_eq!(
        html("> e\n:::e\n> :::\nl\n"),
        "<blockquote>\n  <p>e\n:::e</p>\n  <div>\n\n  </div>\n</blockquote>\n<p>l</p>"
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
    // carve#4863 ruling: a colon line is read on its own.
    assert_eq!(
        html("> > e\n> :::e\n> > :::\nl\n"),
        "<blockquote>\n  <blockquote>\n    <p>e\n:::e</p>\n    <div>\n\n    </div>\n  </blockquote>\n</blockquote>\n<p>l</p>"
    );
}

#[test]
fn a_lazily_framed_colon_line_survives_reparse_at_the_inner_quote() {
    // carve#4863 ruling: a colon line is read on its own.
    assert_eq!(
        html("> > e\n:::e\n> > :::\nl\n"),
        "<blockquote>\n  <blockquote>\n    <p>e\n:::e</p>\n    <div>\n\n    </div>\n  </blockquote>\n</blockquote>\n<p>l</p>"
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
    // carve#4863 ruling: a colon line is read on its own.
    assert_eq!(
        html("> > a:b\n>\n> x\n> :::e\n> :::\nl\n"),
        "<blockquote>\n  <blockquote><p>a:b</p></blockquote>\n  <p>x\n:::e</p>\n  <div>\n\n  </div>\n</blockquote>\n<p>l</p>"
    );
}
