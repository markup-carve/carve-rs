//! A description that closed before an item paragraph does not keep its
//! marker-column boundary active (PART 0 S4, markup-carve/carve#5323).

use carve::to_html;

#[test]
fn a_closed_description_leaves_the_item_paragraph_open() {
    for block in ["# H", "---", "```\nx\n```"] {
        let source = format!(
            "- a\n  :: t\n  : u\n  {}\n  w\n: z\n",
            block.replace('\n', "\n  ")
        );
        let html = to_html(&source);
        assert!(html.contains("w\n: z\n  </li>"), "{source:?}: {html}");
        assert!(!html.contains("<p>: z</p>"), "{source:?}: {html}");
    }
}

#[test]
fn a_later_heading_releases_a_term_without_a_description() {
    let html = to_html("- a\n  :: t\n  # H\n  w\n: z\n");
    assert!(html.contains("w\n: z\n  </li>"), "{html}");
}
