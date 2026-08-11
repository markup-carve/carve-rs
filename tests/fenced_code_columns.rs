use carve::{BlockNode, CodeBlock};

fn code_blocks(src: &str) -> Vec<CodeBlock> {
    fn collect(blocks: &[BlockNode], out: &mut Vec<CodeBlock>) {
        for block in blocks {
            match block {
                BlockNode::CodeBlock(code) => out.push(code.clone()),
                BlockNode::List(list) => {
                    for item in &list.items {
                        collect(&item.children, out);
                    }
                }
                BlockNode::BlockQuote(quote) => collect(&quote.children, out),
                BlockNode::Admonition(admonition) => collect(&admonition.children, out),
                BlockNode::Div(div) => collect(&div.children, out),
                _ => {}
            }
        }
    }

    let doc = carve::parse(src);
    let mut out = Vec::new();
    collect(&doc.children, &mut out);
    out
}

#[test]
fn opener_at_document_column_zero_opens_fence() {
    let blocks = code_blocks("```\nx\n```");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].content, "x");
}

#[test]
fn indented_document_openers_do_not_open_fences() {
    for indent in 1..=4 {
        let src = format!("{} ```\nx\n{} ```", " ".repeat(indent), " ".repeat(indent));
        assert!(
            code_blocks(&src).is_empty(),
            "{indent}-space indented opener parsed as fenced code"
        );
    }
}

#[test]
fn opener_at_list_item_content_column_opens_fence() {
    assert_eq!(
        carve::to_html("- ```\n  x\n  ```\n").trim(),
        "<ul>\n  <li>\n    <pre><code>x\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn opener_one_column_past_list_item_content_column_is_text() {
    assert!(code_blocks("- x\n   ```\n   y\n   ```").is_empty());
}

#[test]
fn opener_at_block_quote_content_column_opens_fence() {
    assert_eq!(
        carve::to_html("> ```\n> x\n> ```\n").trim(),
        "<blockquote>\n  <pre><code>x\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn indented_closer_is_code_content_not_a_delimiter() {
    let blocks = code_blocks("```\nx\n ```\ny\n```");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].content, "x\n ```\ny");
}

#[test]
fn indented_backtick_run_survives_as_sample_text_inside_fence() {
    assert_eq!(
        carve::to_html("````\nexample:\n ```\n````\n").trim(),
        "<pre><code>example:\n ```\n</code></pre>"
    );
}

#[test]
fn fence_opens_on_a_list_item_continuation_line() {
    // Regression (corpus 84-list-lazy-continuation-7): the fence opener is not
    // on the marker line but on a later continuation line at the item's content
    // column. The lead paragraph's interrupt check dedents the opener, but its
    // closer lookahead runs over the raw remaining lines -- under the
    // column-exact rule those must be dedented by the same amount or the closer
    // is missed and the fence never opens.
    assert_eq!(
        carve::to_html("- item\n+\n```\nc\n```\n\ntail\n").trim(),
        "<ul>\n  <li>item\n    <pre><code>c\n</code></pre>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn nested_fence_content_dedents_to_the_item_content_column() {
    // Indentation INSIDE the fence is preserved relative to the content column,
    // not stripped wholesale: `    deep` (col 4, content col 2) keeps 2 spaces.
    assert_eq!(
        carve::to_html("- item\n+\n```\n  deep\ncode\n```\n").trim(),
        "<ul>\n  <li>item\n    <pre><code>  deep\ncode\n</code></pre>\n  </li>\n</ul>"
    );
}
