//! carve#5189: a `{%` run that starts a paragraph continuation line is at block
//! position and follows CARVE-P2-035 in every host.

use carve::{djot_to_carve, to_html};

#[test]
fn an_unindented_continuation_keeps_the_run_literal_in_each_host() {
    for (source, expected) in [
        ("pre\n{% a\nb %}\n", "<p>pre\n{% a\nb %}</p>"),
        (
            "- pre\n  {% a\n  b %}\n",
            "<ul>\n  <li>pre\n{% a\nb %}</li>\n</ul>",
        ),
        (
            "> pre\n> {% a\n> b %}\n",
            "<blockquote><p>pre\n{% a\nb %}</p></blockquote>",
        ),
        (
            "> - pre\n>   {% a\n>   b %}\n",
            "<blockquote>\n  <ul>\n    <li>pre\n{% a\nb %}</li>\n  </ul>\n</blockquote>",
        ),
    ] {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

#[test]
fn an_indented_continuation_is_a_comment() {
    assert_eq!(to_html("pre\n{% a\n  b %}\n"), "<p>pre\n</p>");
    assert_eq!(
        to_html("> pre\n> {% a\n>  b %} c\n"),
        "<blockquote><p>pre\n c</p></blockquote>"
    );
}

#[test]
fn a_nested_items_comment_uses_authored_columns_after_collection() {
    assert_eq!(
        to_html("- a\n  - b\n{% c\n d %}\n  - z\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n</li>\n      <li>z</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html("- a\n  - b\n{% c\nd %}\n  - z\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n{% c\nd %}</li>\n      <li>z</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html("1. a\n  1. b\n  {% c\n    d %}\n  1. z\n"),
        "<ol>\n  <li>a</li>\n  <li>b\n</li>\n  <li>z</li>\n</ol>"
    );
    assert_eq!(
        to_html("- a\n  - b\n\t{% c\n    d %}\n  - z\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n{% c\nd %}</li>\n      <li>z</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html("> - a\n>   - b\n> \t{% c\n>    d %}\n>   - z\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n{% c\nd %}</li>\n        <li>z</li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>"
    );
    assert_eq!(
        to_html("- a\n  - b\n \t{% c\n\td %}\n  - z\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n{% c\nd %}</li>\n      <li>z</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn text_after_the_closer_keeps_an_item_run_in_a_quote_literal() {
    assert_eq!(
        to_html("> - pre\n>   {% a\n>   b %} c\n"),
        "<blockquote>\n  <ul>\n    <li>pre\n{% a\nb %} c</li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn a_lazy_line_in_a_quote_does_not_continue_the_run() {
    assert_eq!(
        to_html("> pre\n> {% a\nb %} c\n"),
        "<blockquote><p>pre\n{% a\nb %} c</p></blockquote>"
    );
    assert_eq!(
        to_html("> - pre\n>   {% a\nb %}\n"),
        "<blockquote>\n  <ul>\n    <li>pre\n{% a\nb %}</li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn the_djot_importer_keeps_a_comment_in_a_quote_an_item_opens() {
    assert_eq!(
        djot_to_carve("- > pre\n  > {% a\n  > b %} c\n"),
        "- > pre\n  > {%%} c\n"
    );
    assert_eq!(
        djot_to_carve("ii. > pre\n    > {% a\n    > b %} c\n"),
        "ii.  > pre\n     > {%%} c\n"
    );
    assert_eq!(
        djot_to_carve("- > pre\n  > {}\n  > more\n"),
        "- > pre\n  > {%%}\n  > more\n"
    );
}
