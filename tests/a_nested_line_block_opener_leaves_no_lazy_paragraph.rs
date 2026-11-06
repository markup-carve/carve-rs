//! A flush-left line after a nested item's `::: |` opener reaches the document
//! (markup-carve/carve-rs#3993): the opener holds no stanza, so nothing open
//! takes the lazy line (PART 0 S4).

use carve::to_html;

const EMPTY_BLOCK: &str = "<div class=\"line-block\">\n\n        </div>";

#[test]
fn the_flush_left_line_leaves_both_items() {
    assert_eq!(
        to_html("- a\n  - u\n    ::: |\nb\n"),
        format!("<ul>\n  <li>a\n    <ul>\n      <li>u\n        {EMPTY_BLOCK}\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>")
    );
}

#[test]
fn the_one_level_and_ordered_hosts_agree() {
    for src in [
        "- u\n  ::: |\nb\n",
        "1. a\n   - u\n     ::: |\nb\n",
        "- a\n  1. u\n     ::: |\nb\n",
    ] {
        let html = to_html(src);
        assert!(
            html.trim_end().ends_with("</li>\n</ul>\n<p>b</p>")
                || html.trim_end().ends_with("</li>\n</ol>\n<p>b</p>"),
            "{src:?}: {html}"
        );
    }
}

#[test]
fn a_stanza_line_still_takes_the_lazy_line() {
    let html = to_html("- a\n  - u\n    ::: |\n      x\nb\n");
    assert!(html.contains("x<br>\nb</p>"), "{html}");
}
