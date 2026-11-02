//! CommonMark and cmark-gfm readings pulldown-cmark misses, found by the
//! Markdown import sweep (markup-carve/carve#4925, #4927, #4930, #4931, #4932,
//! #4936, #4937, #4938, #4939, #4940).

use carve::{markdown_to_carve, to_html};

fn rendered(markdown: &str) -> String {
    format!("{}\n", to_html(&markdown_to_carve(markdown)).trim_end())
}

#[test]
fn fence_closer_may_end_in_a_tab() {
    assert_eq!(
        rendered("~~~\ncode\n~~~\t\nafter\n"),
        "<pre><code>code\n</code></pre>\n<p>after</p>\n"
    );
}

#[test]
fn space_then_tab_is_no_hard_break() {
    assert_eq!(rendered("a \t\nb\n"), "<p>a\nb</p>\n");
}

#[test]
fn tab_indented_empty_item_nests() {
    assert_eq!(
        rendered("- x\n\n\t-\n"),
        "<ul>\n  <li><p>x</p>\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n"
    );
}

#[test]
fn tab_indented_item_nests() {
    assert_eq!(
        rendered("- L1\n\n\t- L2\n\n\t\tx\n"),
        "<ul>\n  <li><p>L1</p>\n    <ul>\n      <li><p>L2</p>\n        <p>x</p>\n      </li>\n    </ul>\n  </li>\n</ul>\n"
    );
}

#[test]
fn task_marker_in_a_setext_heading_stays_text() {
    assert_eq!(
        rendered("- [ ] a\n  -\n"),
        "<ul>\n  <li>\n    <h2 id=\"a\">[ ] a</h2>\n  </li>\n</ul>\n"
    );
}

#[test]
fn a_loose_task_item_keeps_its_box() {
    assert_eq!(
        markdown_to_carve("* [ ] foo\n\n* [ ] bar\n"),
        "* [ ] foo\n\n* [ ] bar\n"
    );
    assert_eq!(
        rendered("* [ ] foo\n\n* [ ] bar\n"),
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled aria-label=\"foo\"> <p>foo</p></li>\n  <li><input type=\"checkbox\" disabled aria-label=\"bar\"> <p>bar</p></li>\n</ul>\n"
    );
    assert_eq!(
        markdown_to_carve("- [x] XYZ\n  Body.\n\n  TAIL\n"),
        "- [x] XYZ\n  Body.\n\n  TAIL\n"
    );
}

#[test]
fn tabs_around_delimiter_cells_keep_the_table() {
    assert_eq!(
        rendered("|\tfoo\t|\tbar\t|\n|\t---\t|\t---\t|\n|\tbaz\t|\tquux\t|\n"),
        "<table>\n  <thead>\n    <tr><th scope=\"col\">foo</th><th scope=\"col\">bar</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>baz</td><td>quux</td></tr>\n  </tbody>\n</table>\n"
    );
}

#[test]
fn an_escaped_bracket_opens_no_label() {
    assert_eq!(
        rendered("[link]\\[id]\n\n[id]: /u\n"),
        "<p>[link][id]</p>\n"
    );
}

#[test]
fn a_title_needs_whitespace_after_an_angle_destination() {
    assert_eq!(rendered("[a](<u>\"t\")\n"), "<p>[a](<u>\"t\")</p>\n");
}

#[test]
fn a_comment_across_quote_lines_drops_the_markers() {
    assert_eq!(
        rendered("> a <!--x\n> y--> b\n"),
        "<blockquote><p>a <!--x\ny--> b</p></blockquote>\n"
    );
}

#[test]
fn a_malformed_attribute_is_text() {
    assert_eq!(
        rendered("a <a h=x\t)> b\n"),
        "<p>a &lt;a h=x\t)&gt; b</p>\n"
    );
}

#[test]
fn an_autolink_in_an_image_description_keeps_its_text() {
    assert_eq!(
        rendered("![<http://example.com>](logo.png)\n"),
        "<img src=\"logo.png\" alt=\"http://example.com\">\n"
    );
}

#[test]
fn an_uppercase_closer_ends_a_script_block() {
    assert_eq!(
        rendered("<script>\nalert(1)\n</SCRIPT>\n# T\n"),
        "<script>\nalert(1)\n</SCRIPT>\n<section id=\"T\">\n  <h1>T</h1>\n</section>\n"
    );
}
