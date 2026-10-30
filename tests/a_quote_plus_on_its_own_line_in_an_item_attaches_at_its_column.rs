//! A `> +` that opens a quote on its own line inside a list item or a
//! description attaches the next block at that line's column to the quote
//! (CARVE-P9-067, markup-carve/carve#3421). Stacked openers keep the innermost,
//! and the marker-line form `- > +` attaches at the quote's column too.

use carve::to_html;

fn compact(html: &str) -> String {
    html.lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .replace(">\n<", "><")
}

#[test]
fn a_paragraph_at_the_quote_column_joins_the_quote() {
    assert_eq!(
        compact(&to_html("- a\n  > +\n  q\n")),
        "<ul><li>a\n<blockquote><p>q</p></blockquote></li></ul>"
    );
}

#[test]
fn a_table_joins_the_quote_in_a_nested_item() {
    assert_eq!(
        compact(&to_html("1. a\n   - b\n     > +\n     | c |\n")),
        "<ol><li>a\n<ul><li>b\n<blockquote><table><tbody><tr><td>c</td></tr></tbody></table>\
         </blockquote></li></ul></li></ol>"
    );
}

#[test]
fn stacked_openers_hand_the_block_to_the_innermost_quote() {
    assert_eq!(
        compact(&to_html("- a\n  > > +\n  q\n")),
        "<ul><li>a\n<blockquote><blockquote><p>+\nq</p></blockquote></blockquote></li></ul>"
    );
}

#[test]
fn a_description_body_attaches_the_same_way() {
    assert_eq!(
        compact(&to_html(":: t\n: d\n  > +\n  q\n")),
        "<dl><dt>t</dt><dd><p>d</p><blockquote><p>q</p></blockquote></dd></dl>"
    );
}

#[test]
fn a_description_body_attaches_a_fence() {
    assert_eq!(
        compact(&to_html(":: t\n: d\n  > +\n  ```\n  x\n  ```\n")),
        "<dl><dt>t</dt><dd><p>d</p><blockquote><pre><code>x\n</code></pre></blockquote></dd></dl>"
    );
}

#[test]
fn the_marker_line_form_attaches_at_the_quote_column() {
    assert_eq!(
        compact(&to_html("- > +\n  x\n")),
        "<ul><li><blockquote><p>x</p></blockquote></li></ul>"
    );
    assert_eq!(
        compact(&to_html("- > +\n   x\n")),
        "<ul><li><blockquote><p>+\nx</p></blockquote></li></ul>"
    );
}

#[test]
fn a_list_marker_at_the_quote_column_is_not_attached() {
    let html = compact(&to_html("- a\n  > +\n  - b\n"));
    assert!(
        html.contains("<blockquote><ul><li>b</li></ul></blockquote>"),
        "{html}"
    );
}
