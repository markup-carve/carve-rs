use carve::to_html;

#[test]
fn task_item_keeps_a_line_that_misses_its_attached_quotes_open_span() {
    let source = "- [ ] > +\n      %%%\n  after\n";
    let expected = "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <blockquote>\n\n    </blockquote>\n    after\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(to_html("- [ ] > +\n      %%%\n      k\n  after\n"), expected);
}

#[test]
fn plain_item_releases_a_line_at_the_document_column() {
    let source = "- > +\n  %%%\nafter\n";
    let expected = "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn a_deeper_span_and_a_closed_span_keep_their_existing_owners() {
    let deeper = "- [ ] > +\n       %%%\n  after\n";
    let closed = "- [ ] > +\n      %%%\n      k\n      %%%\n  after\n";
    for source in [deeper, closed] {
        let html = to_html(source);
        assert!(!html.contains("<p>after</p>"), "{html}");
    }
    assert!(!to_html(deeper).contains("    after\n"));
    assert_eq!(to_html(closed).matches("<blockquote>").count(), 1);
}
