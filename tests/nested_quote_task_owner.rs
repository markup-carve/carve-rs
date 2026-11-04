use carve::to_html;

#[test]
fn a_lazy_line_keeps_the_second_inner_quotes_literal_plus() {
    // The refused first-block markers render nothing (carve#5013).
    assert_eq!(to_html("> > - > +\n> > - > +\n>  s\n").trim_end(), "<blockquote>\n  <blockquote>\n    <ul>\n      <li>\n        <blockquote>\n\n        </blockquote>\n      </li>\n      <li>\n        <blockquote>\n\n        </blockquote>\n      </li>\n    </ul>\n  </blockquote>\n  <p>s</p>\n</blockquote>");
}

#[test]
fn a_zero_shift_later_quote_supplies_the_first_block() {
    // The refused first-block markers render nothing (carve#5013).
    assert_eq!(to_html("> > - > +\n> > - > +\n> s\n").trim_end(), "<blockquote>\n  <blockquote>\n    <ul>\n      <li>\n        <blockquote>\n\n        </blockquote>\n      </li>\n      <li>\n        <blockquote>\n\n        </blockquote>\n      </li>\n    </ul>\n  </blockquote>\n  <p>s</p>\n</blockquote>");
}

#[test]
fn a_first_block_at_document_column_attaches_to_the_single_quote() {
    // The refused first-block markers render nothing (carve#5013).
    assert_eq!(to_html("- a\n  > - > +\nb\n").trim_end(), "<ul>\n  <li>a\n    <blockquote>\n      <ul>\n        <li>\n          <blockquote>\n\n          </blockquote>\n        </li>\n      </ul>\n    </blockquote>\n  </li>\n</ul>\n<p>b</p>");
}

#[test]
fn a_deeper_quote_keeps_the_literal_plus_under_the_same_ruling() {
    // The refused first-block markers render nothing (carve#5013).
    assert_eq!(to_html("> > > - > +\n> > > - > +\n>  s\n").trim_end(), "<blockquote>\n  <blockquote>\n    <blockquote>\n      <ul>\n        <li>\n          <blockquote>\n\n          </blockquote>\n        </li>\n        <li>\n          <blockquote>\n\n          </blockquote>\n        </li>\n      </ul>\n    </blockquote>\n  </blockquote>\n  <p>s</p>\n</blockquote>");
}

#[test]
fn a_task_item_column_releases_the_nested_term() {
    let source = "* [ ] - :: t\n      : a\n      :: sibling\n      tail\n";
    let html = to_html(source);
    assert!(html.contains("<dt>t</dt>"), "{html}");
    assert!(!html.contains("<dt>t\n"), "{html}");
    assert!(html.contains(": a"), "{html}");
}

#[test]
fn a_line_one_column_short_releases_the_nested_term() {
    let source = "* [ ] - :: t\n     : a\n     :: sibling\n     tail\n";
    let html = to_html(source);
    let term = html
        .split("<dt>")
        .nth(1)
        .unwrap()
        .split("</dt>")
        .next()
        .unwrap();
    // A term is one line (carve#4861): `: a` is the task item's text.
    assert_eq!(term, "t", "{html}");
    assert!(html.contains("</ul>\n    : a\n    <dl>"), "{html}");
    assert!(html.contains("<dt>sibling</dt>"), "{html}");
}

#[test]
fn a_line_one_column_past_releases_the_nested_term() {
    let source = "* [ ] - :: t\n       : a\n       :: sibling\n       tail\n";
    let html = to_html(source);
    let term = html
        .split("<dt>")
        .nth(1)
        .unwrap()
        .split("</dt>")
        .next()
        .unwrap();
    // A term is one line (carve#4861): `: a` is the task item's text.
    assert_eq!(term, "t", "{html}");
    assert!(html.contains("</ul>\n    : a\n    <dl>"), "{html}");
    assert!(html.contains("<dt>sibling</dt>"), "{html}");
}
