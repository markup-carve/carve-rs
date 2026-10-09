//! CommonMark strips only spaces and tabs from the end of a line (4.8, 6.7)
//! and needs two spaces for a hard break (6.6), so a form feed or vertical tab
//! ending a Markdown line is content. pulldown-cmark strips both, and the
//! importer puts them back (markup-carve/carve-rs#2394). carve-js and
//! carve-php import every case below to the same bytes.

use carve::{markdown_to_ast, markdown_to_carve, render_html, to_html};

macro_rules! cases {
    ($($name:ident: $markdown:expr => $carve:expr,)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn imports_as_the_expected_bytes() {
                    assert_eq!(markdown_to_carve($markdown), $carve);
                }

                #[test]
                fn both_exits_render_alike() {
                    assert_eq!(
                        render_html(&markdown_to_ast($markdown)).unwrap(),
                        to_html(&markdown_to_carve($markdown))
                    );
                }
            }
        )*
    };
}

cases! {
    form_feed_before_a_soft_break: "a\x0c\nb\n" => "a\x0c\nb\n",
    vertical_tab_before_a_soft_break: "a\x0b\nb\n" => "a\x0b\nb\n",
    form_feed_ending_the_document: "a\x0c" => "a\x0c\n",
    form_feed_with_crlf: "a\x0c\r\nb\r\n" => "a\x0c\nb\n",
    two_form_feeds: "a\x0c\x0c\nb\n" => "a\x0c\x0c\nb\n",
    space_before_a_form_feed_is_interior: "a \x0c\nb\n" => "a \x0c\nb\n",
    tab_before_a_form_feed_is_interior: "a\t\x0c\nb\n" => "a\t\x0c\nb\n",
    one_space_after_a_form_feed_is_no_hard_break: "a\x0c \nb\n" => "a\x0c\nb\n",
    two_spaces_after_a_form_feed_are_a_hard_break: "a\x0c  \nb\n" => "a\x0c\\\nb\n",
    setext_heading: "a\x0c\n===\n" => "# a\x0c\n",
    block_quote: "> a\x0c\n> b\x0c\n" => "> a\x0c\n> b\x0c\n",
    tight_list: "- a\x0c\n- b\x0c\n" => "- a\x0c\n- b\x0c\n",
    after_emphasis: "*a*\x0c\nb\n" => "/a/\x0c\nb\n",
    after_code: "`c`\x0c\nb\n" => "`c`\x0c\nb\n",
    after_a_link: "[l](/u)\x0c\nb\n" => "[l](/u)\x0c\nb\n",
    no_break_space_was_already_kept: "a\u{a0}\nb\n" => "a\u{a0}\nb\n",
}

/// A GFM table trims the whitespace around a cell's content; that rule is not
/// the paragraph line end and stays as it was.
#[test]
fn a_table_cell_still_trims_its_form_feed() {
    assert_eq!(
        markdown_to_carve("| a | b\x0c\n|---|---|\n| c | d\x0c\n"),
        markdown_to_carve("| a | b\n|---|---|\n| c | d\n")
    );
}
