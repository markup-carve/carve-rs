use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_quoted_plus_can_probe_an_unopened_item_past_a_note_definition() {
    let source = "> - p\n> +\n> \n> {.y}\n> +\n> [^b]: n\n";
    let expected =
        "<blockquote>\n  <ul>\n    <li>p</li>\n  </ul>\n  <p class=\"y\">+</p>\n</blockquote>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_quoted_plus_without_an_item_can_probe_past_a_note_definition() {
    let source = "> +\n> \n> {.y}\n> +\n> [^b]: n\n";
    let expected = "<blockquote><p class=\"y\">+</p></blockquote>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}
