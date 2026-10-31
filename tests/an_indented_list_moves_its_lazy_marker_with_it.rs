//! A line at or past an indented list's column moves with that list (PART 0
//! S3), so a different marker there starts a sibling list at the same indent
//! (PART 9 §11 N1, markup-carve/carve#4363).

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let positioned = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, positioned,
        "the two parse paths disagree on {src:?}"
    );
    facade.trim().to_string()
}

#[test]
fn a_different_marker_at_an_indented_list_column_starts_a_new_list() {
    assert_eq!(
        both_paths(" * a\n - x\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>x</li>\n</ul>"
    );
}

#[test]
fn an_indented_pair_in_an_item_body_reads_as_two_sub_lists() {
    assert_eq!(
        both_paths("- p\n\n   * x\n   - y\n"),
        concat!(
            "<ul>\n  <li>p\n    <ul>\n      <li>x</li>\n    </ul>\n",
            "    <ul>\n      <li>y</li>\n    </ul>\n  </li>\n</ul>"
        )
    );
}

#[test]
fn a_different_delimiter_at_an_indented_column_starts_a_new_list() {
    assert_eq!(
        both_paths("  1. a\n  2) b\n"),
        "<ol>\n  <li>a</li>\n</ol>\n<ol start=\"2\" data-delim=\")\">\n  <li>b</li>\n</ol>"
    );
}

#[test]
fn a_marker_left_of_the_list_column_still_folds() {
    assert_eq!(
        both_paths("  * a\n - x\n"),
        "<ul>\n  <li>a\n- x</li>\n</ul>"
    );
}
