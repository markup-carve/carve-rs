//! More CommonMark and cmark-gfm readings pulldown-cmark misses
//! (markup-carve/carve#4927, #4928, #4937, #4939).

use carve::{markdown_to_carve, to_html};

fn rendered(markdown: &str) -> String {
    format!("{}\n", to_html(&markdown_to_carve(markdown)).trim_end())
}

const FOO_BAR: &str = "<table>\n  <thead>\n    <tr><th scope=\"col\">foo</th><th scope=\"col\">foo</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>bar</td><td>bar</td></tr>\n  </tbody>\n</table>\n";

#[test]
fn a_table_interrupts_a_paragraph() {
    assert_eq!(
        rendered("paragraph\nfoo|foo\n---|---\nbar|bar\n"),
        format!("<p>paragraph</p>\n{FOO_BAR}")
    );
    assert_eq!(
        markdown_to_carve("> paragraph\n> foo|foo\n> ---|---\n> bar|bar\n"),
        "> paragraph\n>\n> |= foo |= foo |\n> | bar | bar |\n"
    );
}

#[test]
fn a_table_interrupting_an_items_paragraph_keeps_the_list_tight() {
    assert_eq!(
        markdown_to_carve("- paragraph\n  a|b\n  -|-\n  c|d\n- x\n"),
        "- paragraph\n  |= a |= b |\n  | c | d |\n- x\n"
    );
}

#[test]
fn a_delimiter_cell_outside_the_gfm_shape_is_no_table() {
    for cell in ["-::-", "-:-", "- -", "::-"] {
        let markdown = format!("foo|foo\n-----|{cell}\nbar|bar\n");
        assert_eq!(
            rendered(&markdown),
            format!("<p>foo|foo\n-----|{cell}\nbar|bar</p>\n"),
            "{cell}"
        );
    }
}

#[test]
fn an_indented_line_after_a_table_is_code() {
    let table = "<table>\n  <thead>\n    <tr><th scope=\"col\">a</th><th scope=\"col\">b</th></tr>\n  </thead>\n</table>\n";
    assert_eq!(
        rendered("  | a | b |\n  | - | - |\n    | c | d |\n"),
        format!("{table}<pre><code>| c | d |\n</code></pre>\n")
    );
    assert_eq!(
        rendered("a | b\n--|--\n    c | d\n"),
        format!("{table}<pre><code>c | d\n</code></pre>\n")
    );
}

#[test]
fn a_multi_line_parenthesized_title_keeps_its_escaped_parenthesis() {
    assert_eq!(
        rendered("[a](b (\n\\)))\n"),
        "<p><a href=\"b\" title=\"\n)\">a</a></p>\n"
    );
    assert_eq!(
        rendered("> [a](b (\n> \\(\"))\n"),
        "<blockquote><p><a href=\"b\" title=\"\n(&quot;\">a</a></p></blockquote>\n"
    );
}

#[test]
fn a_cdata_section_may_hold_a_bracket() {
    assert_eq!(
        markdown_to_carve("a <![CDATA[x]y]]> b\n"),
        "a `<![CDATA[x]y]]>`{=html} b\n"
    );
    assert_eq!(
        rendered("a <![CDATA[x]y]]> b\n"),
        "<p>a <![CDATA[x]y]]> b</p>\n"
    );
}

#[test]
fn a_closing_tag_may_break_before_its_bracket() {
    assert_eq!(rendered("a </t\n\t> b\n"), "<p>a </t\n> b</p>\n");
    assert_eq!(
        rendered("> a </div\n>     > b\n"),
        "<blockquote><p>a </div\n> b</p></blockquote>\n"
    );
    // A quote marker is not the tag's bracket.
    assert_eq!(
        rendered("a </t\n> b\n"),
        "<p>a &lt;/t</p>\n<blockquote><p>b</p></blockquote>\n"
    );
}

#[test]
fn a_tab_in_the_delimiter_row_of_an_interrupting_table() {
    assert_eq!(
        rendered("paragraph\nfoo|foo\n---|\t---\nbar|bar\n"),
        format!("<p>paragraph</p>\n{FOO_BAR}")
    );
}

#[test]
fn a_second_bracket_after_a_broken_closing_tag_is_text() {
    assert_eq!(rendered("a </t\n    >> b\n"), "<p>a </t\n>&gt; b</p>\n");
}

#[test]
fn an_email_autolink_in_an_earlier_table_keeps_the_list_tight() {
    let carve = markdown_to_carve(
        "| <a\\|b@c> | x |\n|---|---|\n| 1 | 2 |\n\n- paragraph\n  a|b\n  -|-\n  c|d\n- x\n",
    );
    assert!(
        carve.ends_with("- paragraph\n  |= a |= b |\n  | c | d |\n- x\n"),
        "{carve}"
    );
}
