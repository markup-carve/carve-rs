//! PART 0 S1: a stanza opened in a description body ends at a line that misses
//! the description's column, so a later column-0 line has no paragraph to join.

use carve::to_html;

#[test]
fn a_heading_below_the_description_ends_its_stanza() {
    for (tail, out) in [("z\n", "</ul>\n<p>z</p>"), (": z\n", "</ul>\n<p>: z</p>")] {
        let src = format!("- a\n  :: t\n  : u\n    ::: |\n      x\n  # H\n{tail}");
        let html = to_html(&src);
        assert!(html.trim_end().ends_with(out), "{src:?}: {html}");
        assert!(html.contains("<h1 id=\"H\">H</h1>"), "{src:?}: {html}");
    }
}

#[test]
fn a_stanza_at_the_item_column_still_takes_the_heading_line() {
    let html = to_html("- a\n  ::: |\n    x\n  # H\n: z\n");
    assert!(html.contains("x<br>\n# H<br>\n: z</p>"), "{html}");
}

#[test]
fn a_blank_after_the_description_stanza_leaves_the_item_paragraph_open() {
    let html = to_html("- a\n  :: t\n  : u\n    ::: |\n      x\n\n  w\n: z\n");
    assert!(html.contains("<p>w\n: z</p>"), "{html}");
}
