//! A nonblank line that misses an item ends the item's comment span and the
//! item, even with an exact-width closer ahead (PART 0 S1 and the recovery
//! table, markup-carve/carve#4770 option B, corpus 929). Comment-shaped lines
//! do not miss; they stay payload.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(facade, authoritative, "{src:?}");
    facade
}

#[test]
fn the_line_ends_an_item_body_span() {
    assert_eq!(
        both_paths("- a\n  %%%\n  n\nzz\n%%%\n"),
        "<ul>\n  <li>a\n    n\n  </li>\n</ul>\n<p>zz</p>"
    );
}

#[test]
fn the_line_ends_a_marker_line_span_and_both_items() {
    assert_eq!(
        both_paths(". a\n  . %%%\n    n\n>>\n%%%\n"),
        "<ol>\n  <li>a\n    <ol>\n      <li>n</li>\n    </ol>\n  </li>\n</ol>\n<p>&gt;&gt;</p>"
    );
}

#[test]
fn a_line_that_misses_only_the_inner_item_lands_in_the_outer_one() {
    assert_eq!(
        both_paths(". a\n  . %%%\n    n\n  zz\n%%%\n"),
        "<ol>\n  <li>a\n    <ol>\n      <li>n</li>\n    </ol>\n    zz\n  </li>\n</ol>"
    );
}

#[test]
fn without_a_closer_the_line_folds_as_after_a_line_comment() {
    for (src, plain) in [
        ("- a\n  %%%\n  n\nzz\n", "- a\n  %%\n  n\nzz\n"),
        (". a\n  . %%%\n    n\n>>\n", ". a\n  . %%\n    n\n>>\n"),
    ] {
        assert_eq!(both_paths(src), both_paths(plain), "{src:?}");
    }
}

#[test]
fn comment_shaped_lines_below_the_column_stay_payload() {
    for (src, html) in [
        (
            "- a\n  %%%\n  n\n%% c\n%%%\nafter\n",
            "<ul>\n  <li>a</li>\n</ul>\n<p>after</p>",
        ),
        (
            "- a\n  - %%%\n    n\n%% c\n%%%\nafter\n",
            "<ul>\n  <li>a\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>",
        ),
        (
            "- head\n\n    %%%%\n    n\n%%%\n    b\n%%%%\n\n  tail\n",
            "<ul>\n  <li><p>head</p>\n    <p>tail</p>\n  </li>\n</ul>",
        ),
    ] {
        assert_eq!(both_paths(src), html, "{src:?}");
    }
}

#[test]
fn a_sibling_marker_after_the_span_stays_in_the_list() {
    assert_eq!(
        both_paths("- a\n  %%%\n  n\n- m\n%%%\nafter\n"),
        "<ul>\n  <li>a\n    n\n  </li>\n  <li>m</li>\n</ul>\n<p>after</p>"
    );
}
