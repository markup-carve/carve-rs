//! A `%%%` after a list marker that folded into a quote paragraph opens no
//! span, and a span an item holds ends at a line that misses that item, also
//! when the item sits in a quote or holds a footnote (PART 0 S1, carve#4907).

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_folded_marker_in_a_quote_leaves_its_paragraph_open() {
    for opener in ["%%%", "%%%%", "%%% c"] {
        assert_eq!(
            flat(&format!("> a\n> - {opener}\nx\n\nx\n")),
            "<blockquote><p>a-x</p></blockquote><p>x</p>",
            "{opener:?}"
        );
        assert_eq!(
            flat(&format!("> > a\n> > - {opener}\nx\n")),
            "<blockquote><blockquote><p>a-x</p></blockquote></blockquote>",
            "{opener:?}"
        );
    }
}

#[test]
fn a_quoted_line_left_of_the_item_ends_its_span() {
    assert_eq!(
        flat("> - %%%\n> y\nx\n"),
        "<blockquote><ul><li></li></ul><p>yx</p></blockquote>"
    );
    assert_eq!(
        flat("> - b\n> - %%%\n> y\nx\n"),
        "<blockquote><ul><li>b</li><li></li></ul><p>yx</p></blockquote>"
    );
}

#[test]
fn a_line_left_of_the_item_ends_a_span_below_its_footnote() {
    let html = flat("- a\n  - b\n%%%\n%%%\n  [^f]: n\n  %%%\n\n[r][] y[^f]\n");
    assert!(html.contains("<p>[r][] y<a id=\"fnref1\""), "{html}");
    let html = flat("- a\n  [^f]: n\n  %%%\ny[^f]\n");
    assert!(html.starts_with("<ul><li>a</li></ul><p>y<a"), "{html}");
}

#[test]
fn a_span_below_a_nested_footnote_belongs_to_the_item_at_its_column() {
    let html = flat("- a\n  - b\n    [^f]: n\n   %%%\n    y\n%%%\nz[^f]\n");
    assert!(
        html.starts_with("<ul><li>a<ul><li>b</li></ul></li></ul><p>z<a"),
        "{html}"
    );
}
