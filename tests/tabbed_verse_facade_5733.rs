use carve::{to_html, to_html_with_options, Options};

#[test]
fn nested_quoted_two_tab_verse_matches_positioned_render() {
    let source = "> - a\n>   - a\n>   \t::: |\n> \t\tz\n>   \t:::\n";
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert!(positioned.contains("<p>z</p>"), "{positioned}");
    assert_eq!(to_html(source), positioned);
}

#[test]
fn ticket_reproductions_match_positioned_render() {
    for (source, expected_fragment) in [
        (
            "> * a\n>   * a\n>     * a\n>   \t::: |\n> \t z\n>   \t:::\n",
            "<div>\n\n          </div>",
        ),
        ("* a\n  * a\n   \t::: |\n  \ty\n   \t:::\n", "<p>y</p>"),
        (
            "> - a\n>   - a\n>   \t::: |\n>     \t z\n>   \t:::\n",
            "<p>&nbsp;z</p>",
        ),
    ] {
        let facade = to_html(source);
        let positioned = to_html_with_options(source, &Options::default().with_positions(true));
        assert_eq!(
            facade, positioned,
            "{source:?}\nfacade:\n{facade}\npositioned:\n{positioned}"
        );
        assert!(facade.contains(expected_fragment), "{source:?}\n{facade}");
    }
}
