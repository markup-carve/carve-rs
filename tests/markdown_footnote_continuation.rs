use carve::ast::BlockNode;
use carve::{markdown_to_ast, parse, to_markdown};

#[test]
fn a_second_paragraph_stays_in_the_footnote() {
    let source = "see[^a]\n\n[^a]: note\n\n  second para\n";
    assert_eq!(parse(source).footnote_defs["a"].len(), 2);
    let markdown = to_markdown(source);
    assert_eq!(markdown, "see[^a]\n\n[^a]: note\n\n    second para\n");
    let back = markdown_to_ast(&markdown);
    assert_eq!(back.children.len(), 1);
    assert!(matches!(back.children[0], BlockNode::Paragraph(_)));
    let body = &back.footnote_defs["a"];
    assert_eq!(body.len(), 2);
    assert!(body
        .iter()
        .all(|block| matches!(block, BlockNode::Paragraph(_))));
}

#[test]
fn later_list_and_fence_keep_their_layout() {
    let source = "see[^a]\n\n[^a]: note\n\n  - one\n    - nested\n\n  ``` rust\n  let x = 1;\n\n  let y = 2;\n  ```\n";
    let markdown = to_markdown(source);
    assert_eq!(
        markdown,
        "see[^a]\n\n[^a]: note\n\n    - one\n      - nested\n\n    ```rust\n    let x = 1;\n\n    let y = 2;\n    ```\n"
    );
    assert!(markdown
        .lines()
        .all(|line| line.is_empty() || !line.ends_with([' ', '\t'])));
    let back = markdown_to_ast(&markdown);
    assert_eq!(back.children.len(), 1);
    let body = &back.footnote_defs["a"];
    assert_eq!(body.len(), 3);
    assert!(matches!(body[0], BlockNode::Paragraph(_)));
    let BlockNode::List(list) = &body[1] else {
        panic!("expected a list: {:?}", body[1]);
    };
    assert!(matches!(list.items[0].children[1], BlockNode::List(_)));
    let BlockNode::CodeBlock(code) = &body[2] else {
        panic!("expected a code block: {:?}", body[2]);
    };
    assert_eq!(code.content, "let x = 1;\n\nlet y = 2;\n");
}
