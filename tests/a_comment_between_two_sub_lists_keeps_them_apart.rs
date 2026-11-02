//! An invisible line between two sub-lists of a tight item does not keep them
//! apart on re-parse, so the writer spells the hard boundary below it
//! (CARVE-P2-034 as amended by markup-carve/carve#4981; carve-rs#3774).

use carve::{markdown_to_carve, to_html};

const TWO_LISTS: &str = "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n";

#[test]
fn a_comment_ends_the_inner_list_of_a_compact_item() {
    for markdown in [
        "- - a\n\n  <!-- c -->\n\n  - b\n",
        "- - a\n  <!-- c -->\n  - b\n",
    ] {
        let carve = markdown_to_carve(markdown);
        assert_eq!(carve, "- - a\n\n  %% c\n\n\n\n  - b\n", "{markdown:?}");
        assert_eq!(format!("{}\n", to_html(&carve).trim_end()), TWO_LISTS);
    }
}

#[test]
fn an_ordered_host_keeps_the_boundary_too() {
    let carve = markdown_to_carve("1. - a\n\n   <!-- c -->\n\n   - b\n");
    assert_eq!(carve, "1. - a\n\n   %% c\n\n\n\n   - b\n");
}
