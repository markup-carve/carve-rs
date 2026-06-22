use carve::{BlockNode, CodeBlock};

fn only_code_block(src: &str) -> CodeBlock {
    let doc = carve::parse(src);
    assert_eq!(doc.children.len(), 1);
    match doc.children.into_iter().next().unwrap() {
        BlockNode::CodeBlock(code) => code,
        other => panic!("expected code block, got {other:?}"),
    }
}

#[test]
fn fenced_code_header_renders_as_pre_title() {
    assert_eq!(
        carve::to_html("```php \"h\"\nx\n```"),
        "<pre title=\"h\"><code class=\"language-php\">x\n</code></pre>"
    );
}

#[test]
fn block_attribute_title_wins_over_fence_header() {
    assert_eq!(
        carve::to_html("{title=\"from attr\"}\n```php \"from header\"\nx\n```"),
        "<pre title=\"from attr\"><code class=\"language-php\">x\n</code></pre>"
    );
}

#[test]
fn fenced_code_label_is_preserved_but_not_rendered() {
    let code = only_code_block("```php \"h\" [Composer]\nx\n```");
    assert_eq!(code.lang.as_deref(), Some("php"));
    assert_eq!(code.title.as_deref(), Some("h"));
    assert_eq!(code.label.as_deref(), Some("Composer"));
    assert_eq!(
        carve::to_html("```php \"h\" [Composer]\nx\n```"),
        "<pre title=\"h\"><code class=\"language-php\">x\n</code></pre>"
    );
}

#[test]
fn fenced_code_info_string_is_strictly_ordered_and_separated() {
    assert!(matches!(
        carve::parse("```php[Composer]\nx\n```").children.as_slice(),
        [BlockNode::Paragraph(_)]
    ));
    assert!(matches!(
        carve::parse("```php [Composer] \"h\"\nx\n```")
            .children
            .as_slice(),
        [BlockNode::Paragraph(_)]
    ));
    assert!(matches!(
        carve::parse("```php title=\"h\"\nx\n```")
            .children
            .as_slice(),
        [BlockNode::Paragraph(_)]
    ));
}
