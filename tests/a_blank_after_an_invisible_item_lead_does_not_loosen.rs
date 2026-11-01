//! A blank after an invisible item lead separates nothing visible, so the list
//! stays tight (carve#4639, CARVE-P9-023).

use carve::to_html;

const TIGHT: &str = "<ul>\n  <li>Body.</li>\n  <li>x</li>\n</ul>";

#[test]
fn an_invisible_lead_leaves_the_list_tight() {
    for lead in ["%% c", "[r]: /u", "[^n]: x"] {
        let source = format!("- {lead}\n\n  Body.\n- x\n");
        assert_eq!(to_html(&source).trim(), TIGHT, "{source}");
    }
    assert_eq!(to_html("- %%%\n  c\n  %%%\n\n  Body.\n- x\n").trim(), TIGHT);
}

#[test]
fn a_second_paragraph_after_the_first_visible_block_still_loosens() {
    assert_eq!(
        to_html("- %% c\n  Body.\n\n  Two.\n").trim(),
        "<ul>\n  <li><p>Body.</p>\n    <p>Two.</p>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html("- a\n\n  Body.\n- x\n").trim(),
        "<ul>\n  <li><p>a</p>\n    <p>Body.</p>\n  </li>\n  <li><p>x</p></li>\n</ul>"
    );
}
