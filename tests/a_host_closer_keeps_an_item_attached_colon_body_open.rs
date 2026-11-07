//! A HOST CLOSER KEEPS AN ITEM-ATTACHED COLON BODY OPEN (markup-carve/carve#5405).
//!
//! A colon body a marker-line `+` attaches ends at its first blank only when it
//! is unclosed (PART 0 S1, §17 L3). When the item's lines at its content column
//! hold the closer, the blank and the lines after it stay in the body, as in a
//! footnote host. A colon opener inside a line block is verse (ruled option 1).
//!
//! Expectations are the executable spec's output at markup-carve/carve#5412.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

#[test]
fn a_later_closer_takes_a_line_block_past_its_blank() {
    assert_eq!(
        both_paths("- +\n::: |\n\n  ::: x\n  :::\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <div class=\"line-block\">\n",
            "      <p>::: x</p>\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_later_closer_takes_a_div_past_its_blank() {
    assert_eq!(
        both_paths("- +\n::: x\n\n  a\n  :::\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <div class=\"x\">\n",
            "      <p>a</p>\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_closer_that_only_closes_a_nested_div_leaves_the_body_unclosed() {
    assert_eq!(
        both_paths("- +\n::: x\n\n  ::: y\n  :::\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <div class=\"x\">\n",
            "\n",
            "    </div>\n",
            "    <div class=\"y\">\n",
            "\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_blank_inside_the_held_body_does_not_loosen_the_list() {
    // PART 9 §17 L1: the blank is the container's, not the item's.
    assert_eq!(
        both_paths("- +\n::: x\n\n  b\n  :::\n- sib\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <div class=\"x\">\n",
            "      <p>b</p>\n",
            "    </div>\n",
            "  </li>\n",
            "  <li>sib</li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn the_footnote_host_reads_the_same() {
    let html = both_paths("[^1]: +\n::: |\n\n  ::: x\n  :::\n\n[^1]\n");
    assert!(
        html.contains("<div class=\"line-block\">\n        <p>::: x</p>\n      </div>"),
        "{html}"
    );
}
