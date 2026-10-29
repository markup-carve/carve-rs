use carve::{parse_with_options, render_html, Options};

#[test]
fn a_closed_fence_at_the_marker_note_body_column_opens_in_the_note() {
    let source = "- [^n]: text\n    ```\n    code\n    ```\nafter [^n]\n";
    for positions in [false, true] {
        let doc = parse_with_options(source, &Options::default().with_positions(positions));
        assert!(matches!(
            doc.footnote_defs["n"].as_slice(),
            [
                carve::ast::BlockNode::Paragraph(_),
                carve::ast::BlockNode::CodeBlock(_)
            ]
        ));
        assert!(doc
            .children
            .iter()
            .all(|block| !matches!(block, carve::ast::BlockNode::CodeBlock(_))));
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn a_document_column_closer_does_not_close_a_marker_note_fence() {
    let source = "- [^n]: text\n    ```\ncode\n```\nafter [^n]\n";
    for positions in [false, true] {
        let doc = parse_with_options(source, &Options::default().with_positions(positions));
        assert!(doc.footnote_defs["n"]
            .iter()
            .all(|block| !matches!(block, carve::ast::BlockNode::CodeBlock(_))));
        assert!(render_html(&doc).unwrap().contains("after "));
    }
}

#[test]
fn a_closed_item_fence_does_not_hide_a_later_note_definition() {
    let source = "- ```\n  code\n  ```\n\nafter [^n]\n\n[^n]: %% c\n\n  - child\n";
    for positions in [false, true] {
        let doc = parse_with_options(source, &Options::default().with_positions(positions));
        assert!(doc.footnote_defs.contains_key("n"));
        assert!(render_html(&doc).unwrap().contains("doc-endnotes"));
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn continued_marker_notes_follow_code_ownership() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/continued_marker_note_code_floor.json"
    ))
    .unwrap();
    for row in cases.as_array().unwrap() {
        for positions in [false, true] {
            let doc = parse_with_options(
                row["source"].as_str().unwrap(),
                &Options::default().with_positions(positions),
            );
            assert_eq!(
                render_html(&doc).unwrap().trim(),
                row["html"].as_str().unwrap().trim(),
                "{} positions={positions}",
                row["id"]
            );
            if row["id"].as_str().unwrap().contains("col4") {
                assert!(doc.footnote_defs["n"]
                    .iter()
                    .any(|block| matches!(block, carve::ast::BlockNode::CodeBlock(_))));
                assert!(!render_html(&doc).unwrap().contains('\u{e000}'));
            }
        }
    }
}

#[test]
fn a_comment_line_in_a_marker_line_note_changes_no_owner() {
    let pairs = [
        (
            "- [^n]: text\n    %% c\n    ```\n    code\n    ```\nafter [^n]\n",
            "- [^n]: text\n    ```\n    code\n    ```\nafter [^n]\n",
        ),
        (
            "- [^n]: text\n\n    %% c\n    ```\n    code\n    ```\nafter [^n]\n",
            "- [^n]: text\n\n    ```\n    code\n    ```\nafter [^n]\n",
        ),
        (
            "- [^n]: text\n    more\n    %% c\n    ~~~\n    code\nafter [^n]\n",
            "- [^n]: text\n    more\n\n    ~~~\n    code\nafter [^n]\n",
        ),
    ];
    for (commented, plain) in pairs {
        let html = carve::to_html(commented);
        assert_eq!(html, carve::to_html(plain), "{commented:?}");
        assert!(html.contains("<li></li>"), "{commented:?}: {html}");
        assert!(html.contains("<p>after "), "{commented:?}: {html}");
        assert!(
            html.contains("<pre><code>code\n</code></pre>"),
            "{commented:?}: {html}"
        );
    }
}
