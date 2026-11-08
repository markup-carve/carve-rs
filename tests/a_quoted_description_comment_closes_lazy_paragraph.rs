fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn erased_description_body_does_not_leave_a_quoted_paragraph_open() {
    for source in [
        "> :: t\n> : %% c\npara\n",
        "> > :: t\n> > : %% c\npara\n",
        "> :: longer term\n> : %% another comment\npara\n",
    ] {
        let got = html(source);
        assert!(got.contains("</dl></blockquote>"), "{source:?}: {got}");
        assert!(got.ends_with("<p>para</p>"), "{source:?}: {got}");
    }
}

#[test]
fn a_visible_description_body_still_admits_a_lazy_follower() {
    assert_eq!(
        html("> :: t\n> : body\npara\n"),
        "<blockquote><dl><dt>t</dt><dd>body para</dd></dl></blockquote>"
    );
}
