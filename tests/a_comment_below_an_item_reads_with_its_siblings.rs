//! A below-column comment between sibling items keeps them in one list even
//! when the item ends in a block, and a closed code block does not end an
//! item at the closer of a comment span it holds (carve-rs#3522).

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let plain = to_html(src);
    let positioned = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(plain, positioned, "{src:?}");
    plain
}

#[test]
fn a_comment_after_a_block_sub_item_keeps_the_list() {
    for sub in ["# h", "---", "| a |", "```\n    x\n    ```"] {
        for comment in ["%% c", " %% c", "  %% c"] {
            for (marker, tail) in [("-", "- tail"), ("1.", "2. tail")] {
                let src = format!("{marker} x\n   - {sub}\n{comment}\n{tail}\n");
                let html = both_paths(&src);
                let lists = html.matches("<ul>").count() + html.matches("<ol>").count();
                assert_eq!(lists, 2, "{src:?}: {html}");
                assert!(html.contains("<li>tail</li>"), "{src:?}: {html}");
            }
        }
    }
}

#[test]
fn a_closed_code_block_keeps_the_span_closer_below_the_column() {
    for code in ["%%%", "x", "x\n    y", ""] {
        for blank in ["", "\n"] {
            let src = format!(
                "- head\n\n    ```\n    {code}\n    ```\n{blank}    %%%\n    zpay\n%%%\n\n  tail\n"
            );
            let html = both_paths(&src);
            assert!(!html.contains("zpay"), "{src:?}: {html}");
            assert!(
                html.contains("<p>tail</p>\n  </li>\n</ul>"),
                "{src:?}: {html}"
            );
        }
    }
}
