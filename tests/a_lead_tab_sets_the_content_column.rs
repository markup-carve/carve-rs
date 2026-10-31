//! CARVE-P2-010: the content column of an item is where its content starts,
//! past the whole whitespace run a lead tab begins.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_line_below_the_tab_stop_leaves_the_item() {
    assert_eq!(html("- \te\n\n  f\n"), "<ul> <li>e</li> </ul> <p>f</p>");
    assert_eq!(html("1. \te\n\n   f\n"), "<ol> <li>e</li> </ol> <p>f</p>");
}

#[test]
fn the_whole_whitespace_run_counts() {
    assert_eq!(html("- \t e\n\n\tf\n"), "<ul> <li>e</li> </ul> <p>f</p>");
}

#[test]
fn a_block_at_the_tab_stop_stays_in_the_item() {
    assert_eq!(
        html("- \te\n\t> q\n"),
        "<ul> <li>e <blockquote><p>q</p></blockquote> </li> </ul>"
    );
    assert_eq!(
        html("- \te\n    > q\n"),
        "<ul> <li>e <blockquote><p>q</p></blockquote> </li> </ul>"
    );
}

#[test]
fn content_behind_a_lead_tab_stays_paragraph_text() {
    assert_eq!(html("- \t# h\n"), "<ul> <li># h</li> </ul>");
    assert_eq!(html("- \t> q\n"), "<ul> <li>&gt; q</li> </ul>");
}
