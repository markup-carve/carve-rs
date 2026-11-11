// Corpus rows 1073, 1070-2 and 1031: render and span regressions against the
// released engine (carve#5885, carve#5895, carve-rs#4262).

use carve::ast::BlockNode;

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_owned()
}

fn verse(lead: usize) -> String {
    format!(
        "<ul>\n  <li>a\n    <div class=\"line-block\">\n      <p>{}x</p>\n    </div>\n  </li>\n</ul>",
        "&nbsp;".repeat(lead)
    )
}

#[test]
fn a_verse_lead_under_an_indented_opener_is_measured_per_frame() {
    assert_eq!(html("- a\n   ::: |\n  \tx\n   :::\n"), verse(3));
    assert_eq!(html("- a\n   ::: |\n    \tx\n   :::\n"), verse(3));
    assert_eq!(html("- a\n  \t::: |\n    \tx\n  \t:::\n"), verse(0));
    assert_eq!(
        html("> - a\n>    ::: |\n>   \tx\n>    :::\n"),
        format!("<blockquote>\n  {}\n</blockquote>", verse(3).replace('\n', "\n  "))
    );
}

#[test]
fn an_untabbed_verse_lead_keeps_its_line_start_columns() {
    assert_eq!(html("- a\n   ::: |\n      x\n   :::\n"), verse(3));
    assert_eq!(html("- a\n   ::: |\n    x\n   :::\n"), verse(1));
}

#[test]
fn a_lazy_comment_opener_left_of_a_nested_marker_is_measured_by_its_lines() {
    let hidden = "<ul>\n  <li>a\n    <ul>\n      <li>b\n</li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(html("- a\n  - b\n {% x\n    %}\n"), hidden);
    assert_eq!(html("- a\n  - b\n {% x\n     %}\n"), hidden);
    assert_eq!(
        html("- a\n  - b\n {% x\n   %}\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n{% x\n%}</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn an_item_fence_ends_before_the_sibling_its_next_line_opens() {
    let doc = carve::parse_with_options(
        "- p\n+\n```\na\n\n ```\n  zq\n",
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    );
    let ends: Vec<(usize, usize)> = doc
        .children
        .iter()
        .map(|block| match block {
            BlockNode::List(list) => {
                let pos = list.pos.as_ref().expect("list span");
                (pos.start_offset, pos.end_offset)
            }
            BlockNode::CodeBlock(code) => {
                let pos = code.pos.as_ref().expect("code span");
                (pos.start_offset, pos.end_offset)
            }
            other => panic!("unexpected block {other:?}"),
        })
        .collect();
    assert_eq!(ends, [(0, 13), (14, 22)]);
    let BlockNode::List(list) = &doc.children[0] else {
        unreachable!()
    };
    let item = &list.items[0];
    assert_eq!(item.pos.as_ref().map(|pos| pos.end_offset), Some(13));
    let BlockNode::CodeBlock(code) = &item.children[1] else {
        panic!("item code block")
    };
    assert_eq!(code.pos.as_ref().map(|pos| pos.end_offset), Some(13));
}
