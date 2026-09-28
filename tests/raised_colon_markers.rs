use carve::{parse_with_options, to_carve, to_html, Options};
use serde_json::Value;

#[test]
fn raised_colon_markers_follow_the_shared_corpus() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/raised-colon-markers.json")).unwrap();
    for row in rows {
        let source = row["source"].as_str().unwrap();
        let expected = row["html"].as_str().unwrap();
        assert_eq!(to_html(source).trim(), expected.trim(), "{}", row["name"]);
        assert_eq!(
            to_html(&to_carve(source)).trim(),
            expected.trim(),
            "formatted {}",
            row["name"]
        );
    }
}

#[test]
fn bullet_ordered_and_task_markers_fold_at_each_column() {
    for marker in ["- second", "1. second", "- [x] second"] {
        for column in [1, 2, 4, 6, 8] {
            let source = format!(
                "- head\n\n      :::\n      a\n{}{marker}\n",
                " ".repeat(column)
            );
            assert!(
                to_html(&source).contains(&format!("<p>a\n{marker}</p>")),
                "{source}"
            );
        }
    }
}

#[test]
fn lazy_marker_positions_still_reach_the_authored_end() {
    use carve::ast::BlockNode;
    for column in [1, 2, 4, 6, 8] {
        let source = format!(
            "- head\n\n      :::\n      café\n{}- second\n",
            " ".repeat(column)
        );
        let document = parse_with_options(
            &source,
            &Options {
                positions: true,
                ..Default::default()
            },
        );
        let BlockNode::List(list) = &document.children[0] else {
            panic!("list")
        };
        let BlockNode::Div(div) = &list.items[0].children[1] else {
            panic!("div")
        };
        let BlockNode::Paragraph(paragraph) = &div.children[0] else {
            panic!("paragraph")
        };
        let pos = paragraph.pos.as_ref().unwrap();
        assert_eq!(pos.start_line, 4);
        assert_eq!(pos.end_line, 5);
        assert_eq!(pos.end_offset, source.chars().count() - 1);
    }
}
