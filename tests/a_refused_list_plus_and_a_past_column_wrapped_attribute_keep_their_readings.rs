//! Readings the spec model and carve-js share (markup-carve/carve#3421): a
//! wrapped attribute opened past a description's column is paragraph text, and
//! a nested list item whose first-block `+` is refused leaves no paragraph for a
//! list marker below its column to fold into.

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn a_wrapped_attribute_past_a_description_column_is_paragraph_text() {
    for (source, expected) in [
        (
            ":: t\n: d\n   {.c\n  }\npara\n",
            "<dl><dt>t</dt><dd>d{.c}para</dd></dl>",
        ),
        (
            ":: t\n: d\n\n   {.c\n  }\n- s\n",
            "<dl><dt>t</dt><dd><p>d</p><p>{.c}-s</p></dd></dl>",
        ),
        (
            ":: t\n: d\n   {.c}\npara\n",
            "<dl><dt>t</dt><dd>d</dd></dl><p>para</p>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_band_marker_below_an_empty_plus_item_starts_a_list() {
    for (source, expected) in [
        (
            "- a\n  - +\n - s\n",
            "<ul><li>a<ul><li></li></ul></li><li>s</li></ul>",
        ),
        (
            "- a\n  > - +\n - s\n",
            "<ul><li>a<blockquote><ul><li></li></ul></blockquote></li><li>s</li></ul>",
        ),
        (
            "- > - +\n - s\n",
            "<ul><li><blockquote><ul><li></li></ul></blockquote></li><li>s</li></ul>",
        ),
        (
            "- a\n  - +\n- s\n",
            "<ul><li>a<ul><li></li></ul></li><li>s</li></ul>",
        ),
        (
            "- a\n  - x\n - s\n",
            "<ul><li>a<ul><li>x</li></ul></li><li>s</li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}
