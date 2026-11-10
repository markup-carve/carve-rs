use carve::{to_html, to_html_with_options, Options};

#[test]
fn nested_quoted_two_tab_verse_uses_each_owner_frame() {
    let source = "> - a\n>   - a\n>   \t::: |\n> \t\tz\n>   \t:::\n";
    let html = to_html(source);
    assert!(html.contains("<p>&nbsp;&nbsp;z</p>"), "{html}");
}

#[test]
fn ticket_reproductions_match_positioned_render() {
    for (source, expected_fragment) in [
        (
            "> * a\n>   * a\n>     * a\n>   \t::: |\n> \t z\n>   \t:::\n",
            "<div>\n\n          </div>",
        ),
        ("* a\n  * a\n   \t::: |\n  \ty\n   \t:::\n", "<p>y</p>"),
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

#[test]
fn nested_quoted_one_tab_verse_uses_each_owner_frame() {
    let source = "> - a\n>   - a\n>   \t::: |\n>     \t z\n>   \t:::\n";
    let html = to_html(source);
    assert!(html.contains("<p>&nbsp;&nbsp;&nbsp;z</p>"), "{html}");
}

#[test]
fn quoted_verse_tab_restarts_at_intermediate_item_column() {
    let source = "> - a\n>   - a\n>   \t::: |\n>    \t z\n>   \t:::\n";
    let html = to_html(source);
    assert!(html.contains("<p>&nbsp;z</p>"), "{html}");
}

#[test]
fn quoted_tabbed_closer_uses_the_same_frame_as_its_opener() {
    let closed = "> - a\n>   - a\n>   \t::: |\n>     \t z\n>    \t:::\n";
    let held = "> - a\n>   - a\n>   \t::: |\n>     \t z\n>     \t:::\n";
    let closed_html = to_html(closed);
    let held_html = to_html(held);
    assert!(
        closed_html.contains("<p>&nbsp;&nbsp;&nbsp;z</p>"),
        "{closed_html}"
    );
    assert!(
        held_html.contains("<p>&nbsp;&nbsp;&nbsp;z<br>\n&nbsp;&nbsp;:::</p>"),
        "{held_html}"
    );
}

#[test]
fn below_opener_tab_does_not_indent_the_verse() {
    let source = "\t\t::: |\n\tx\n\t:::\n";
    let html = to_html(source);
    assert_eq!(html, "<div class=\"line-block\">\n  <p>x</p>\n</div>");
}

#[test]
fn tab_only_opener_keeps_nested_verse_and_closer() {
    let source = "- a\n  - a\n\t::: |\n  \tz\n  \t:::\nend\n";
    let html = to_html(source);
    assert!(
        html.contains("<p>&nbsp;&nbsp;z<br>\n&nbsp;&nbsp;:::<br>\nend</p>"),
        "{html}"
    );
}
