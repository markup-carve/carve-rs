use carve::to_html;

#[test]
fn a_lazy_line_keeps_the_second_inner_quotes_literal_plus() {
    let source = "> > - > +\n> > - > +\n>  s\n";
    let html = to_html(source);
    assert!(
        html.contains("<blockquote><p>+\ns</p></blockquote>"),
        "{html}"
    );
}

#[test]
fn a_zero_shift_later_quote_supplies_the_first_block() {
    let source = "> > - > +\n> > - > +\n> s\n";
    let html = to_html(source);
    assert!(
        html.contains("<blockquote><p>+\ns</p></blockquote>"),
        "{html}"
    );
}

#[test]
fn a_first_block_at_document_column_attaches_to_the_single_quote() {
    let html = to_html("- a\n  > - > +\nb\n");
    assert!(
        html.contains("<blockquote><p>+\nb</p></blockquote>"),
        "{html}"
    );
}

#[test]
fn a_deeper_quote_keeps_the_literal_plus_under_the_same_ruling() {
    let source = "> > > - > +\n> > > - > +\n>  s\n";
    let html = to_html(source);
    assert!(
        html.contains("<blockquote><p>+\ns</p></blockquote>"),
        "{html}"
    );
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
    assert!(
        term.contains(": a") && !term.contains(":: sibling"),
        "{html}"
    );
    assert!(html.contains("<dt>sibling"), "{html}");
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
    assert!(
        term.contains(": a") && !term.contains(":: sibling"),
        "{html}"
    );
    assert!(html.contains("<dt>sibling"), "{html}");
}
