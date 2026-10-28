#[test]
fn a_list_sibling_stays_in_the_nested_list_after_term_text_folds() {
    for source in [
        "- - :: t\n: a\n  - sibling",
        "- 1. :: t\n: a\n  2. sibling",
        "1. - :: t\n: a\n   - sibling",
    ] {
        let html = carve::to_html(source);
        let lists = html.matches("<ul>").count() + html.matches("<ol>").count();
        assert_eq!(lists, 2, "{source:?}: {html}");
        assert!(html.contains(": a</dt>"), "{source:?}: {html}");
        assert!(html.contains("<li>sibling</li>"), "{source:?}: {html}");
    }
}

#[test]
fn a_term_at_the_nested_marker_column_returns_to_the_outer_item() {
    for source in [
        "- - :: t\n: a\n  :: sibling",
        "- 1. :: t\n: a\n  :: sibling",
        "1. - :: t\n: a\n   :: sibling",
    ] {
        let html = carve::to_html(source);
        let inner_end = [html.find("</ul>"), html.find("</ol>")]
            .into_iter()
            .flatten()
            .min()
            .unwrap();
        let sibling = html.find("<dt>sibling</dt>").unwrap();
        assert!(inner_end < sibling, "{source:?}: {html}");
        assert!(html.contains(": a</dt>"), "{source:?}: {html}");
    }
}

#[test]
fn a_quote_blank_ends_term_lazy_eligibility() {
    for source in [
        "> :: t\n>\nplain",
        "> - :: t\n>   \nplain",
        "> - - :: t\n>  : a\n>   :: sibling\n>   \nplain",
    ] {
        let html = carve::to_html(source);
        assert!(
            html.ends_with("</blockquote>\n<p>plain</p>"),
            "{source:?}: {html}"
        );
    }
}
