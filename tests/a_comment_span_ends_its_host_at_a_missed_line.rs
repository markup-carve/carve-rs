//! With an exact-width `%%%` closer ahead, a line that misses the span's host
//! ends the span and the host (PART 0 recovery table, markup-carve/carve#4770).
//! A quote's search accepts an unmarked closer and stops at a blank line, and a
//! blank line before a closer below an item's column keeps the closer the
//! span's. markup-carve/carve#4888 and #4889.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(facade, authoritative, "{src:?}");
    facade
}

#[test]
fn a_quote_ends_at_a_line_that_misses_it_with_the_closer_unmarked_below() {
    assert_eq!(
        both_paths("> %%%\n> n\nzz\n%%%\n"),
        "<blockquote>\n\n</blockquote>\n<p>zz</p>"
    );
}

#[test]
fn a_nested_quote_ends_both_quotes_at_that_line() {
    assert_eq!(
        both_paths("> > %%%\n> > n\nzz\n%%%\n"),
        "<blockquote>\n  <blockquote>\n\n  </blockquote>\n</blockquote>\n<p>zz</p>"
    );
}

#[test]
fn a_quote_holding_a_list_ends_at_that_line() {
    assert_eq!(
        both_paths("> %%%\n> - a\nzz\n%%%\n"),
        "<blockquote>\n\n</blockquote>\n<p>zz</p>"
    );
}

#[test]
fn a_blank_before_a_closer_below_the_nested_item_keeps_the_span() {
    assert_eq!(
        both_paths(". a\n  . %%%\n    n\n\n  %%%\n"),
        "<ol>\n  <li>a\n    <ol>\n      <li></li>\n    </ol>\n  </li>\n</ol>"
    );
}

#[test]
fn a_blank_before_a_closer_below_the_item_keeps_the_span() {
    assert_eq!(
        both_paths("- x\n  %%%\n  n\n\n%%%\n"),
        "<ul>\n  <li>x</li>\n</ul>"
    );
}

#[test]
fn a_quote_search_stops_at_a_blank_line() {
    assert_eq!(
        both_paths("> %%%\n> n\nzz\n\n%%%\n"),
        "<blockquote>\n\n</blockquote>\n<p>zz</p>"
    );
}

#[test]
fn without_a_closer_the_line_folds_into_the_quote() {
    assert_eq!(
        both_paths("> %%%\n> n\nzz\n"),
        "<blockquote>\n\n</blockquote>\n<p>zz</p>"
    );
}
