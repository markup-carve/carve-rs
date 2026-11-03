// A term is one line (carve#4861): the separator below the item column ends
// the lists, and what follows it is paragraph text.
#[test]
fn a_separator_below_a_nested_term_ends_the_lists() {
    for source in [
        "- - :: t\n: a\n  - sibling",
        "- 1. :: t\n: a\n  2. sibling",
        "1. - :: t\n: a\n   - sibling",
    ] {
        let html = carve::to_html(source);
        let lists = html.matches("<ul>").count() + html.matches("<ol>").count();
        assert_eq!(lists, 2, "{source:?}: {html}");
        assert!(html.contains("<dt>t</dt>"), "{source:?}: {html}");
        assert!(html.contains("<p>: a\n"), "{source:?}: {html}");
        assert!(!html.contains("<li>sibling</li>"), "{source:?}: {html}");
    }
}

#[test]
fn a_term_after_a_separator_below_a_nested_term_opens_a_new_list() {
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
        assert!(html.contains("<p>: a</p>"), "{source:?}: {html}");
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
