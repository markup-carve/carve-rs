use carve::to_html;

#[test]
fn empty_definition_past_item_column_stays_invisible_before_table() {
    assert_eq!(
        to_html("- a\n    [n]:\n\n     ||\n"),
        "<ul>\n  <li>a\n    <table>\n      <tbody>\n        <tr><td></td></tr>\n      </tbody>\n    </table>\n  </li>\n</ul>"
    );
}

#[test]
fn definition_at_and_past_item_column_resolves() {
    for (source, href) in [
        ("- a\n  [n]:\n\nsee [t][n]\n", ""),
        ("- a\n   [n]:\n\nsee [t][n]\n", ""),
        ("- a\n    [n]:\n\nsee [t][n]\n", ""),
        ("- a\n    [n]: \n\nsee [t][n]\n", ""),
        ("- a\n    [n]: /u\n\nsee [t][n]\n", "/u"),
    ] {
        assert_eq!(
            to_html(source),
            format!("<ul>\n  <li>a</li>\n</ul>\n<p>see <a href=\"{href}\">t</a></p>"),
            "{source:?}"
        );
    }
}

#[test]
fn nested_and_quoted_items_use_the_reached_column() {
    assert_eq!(
        to_html("- - a\n      [n]:\n\nsee [t][n]\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>see <a href=\"\">t</a></p>"
    );
    assert_eq!(
        to_html("> - a\n>     [n]:\n\nsee [t][n]\n"),
        "<blockquote>\n  <ul>\n    <li>a</li>\n  </ul>\n</blockquote>\n<p>see <a href=\"\">t</a></p>"
    );
}

#[test]
fn plain_indented_paragraph_does_not_define_a_link() {
    assert_eq!(
        to_html("    [n]:\n\nsee [t][n]\n"),
        "<p>[n]:</p>\n<p>see [t][n]</p>"
    );
}
