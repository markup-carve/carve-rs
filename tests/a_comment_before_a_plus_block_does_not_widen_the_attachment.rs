//! A comment between an accepted `+` and its block does not let a column-0
//! line after that block join the item (carve-rs#3821, PART 9 §17 L3).

use carve::to_html;

const TIGHT_HEADING: &str = "<ul>\n  <li>a\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>\n<p>tail</p>";

#[test]
fn a_heading_after_a_commented_plus_ends_the_item() {
    assert_eq!(to_html("- a\n+\n%% c\n# h\ntail\n"), TIGHT_HEADING);
    assert_eq!(to_html("- a\n+\n%% c\n%% d\n# h\ntail\n"), TIGHT_HEADING);
}

#[test]
fn the_comment_changes_nothing_against_the_bare_plus() {
    for block in ["# h", "```\nx\n```", "| t |", "***"] {
        let bare = to_html(&format!("- a\n+\n{block}\ntail\n"));
        let commented = to_html(&format!("- a\n+\n%% c\n{block}\ntail\n"));
        assert_eq!(commented, bare, "{block}");
        assert!(bare.ends_with("<p>tail</p>"), "{block}: {bare}");
    }
}

#[test]
fn a_first_block_plus_after_a_comment_ends_the_item_too() {
    let html = to_html("- +\n%% c\n# h\ntail\n");
    assert!(html.ends_with("</ul>\n<p>tail</p>"), "{html}");
}
