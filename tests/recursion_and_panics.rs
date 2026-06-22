//! Severity-1 robustness: untrusted input must never panic or abort.

#[test]
fn link_def_lone_quote_does_not_panic() {
    // A link-definition title tail of a single quote char satisfies both
    // starts_with and ends_with on the same byte; the interior slice must not
    // underflow.
    let _ = carve::to_html("[a]: http://x \"\n");
    let _ = carve::to_html("[a]: http://x '\n");
    // sanity: a real title still parses
    let html = carve::to_html("[a]: http://x \"t\"\n\n[a][]");
    assert!(html.contains("href=\"http://x\""), "{html}");
}

#[test]
fn deeply_nested_blockquote_degrades_without_overflow() {
    let src = ">".repeat(5000) + " x\n";
    let html = carve::to_html(&src); // must return, not abort
    assert!(html.contains("blockquote"), "expected some quote nesting");
}

#[test]
fn deeply_nested_indented_list_degrades_without_overflow() {
    let mut src = String::new();
    for i in 0..400 {
        src.push_str(&"  ".repeat(i));
        src.push_str("- x\n");
    }
    let _ = carve::to_html(&src); // must return, not abort
}

#[test]
fn deeply_nested_inline_brackets_degrade_without_overflow() {
    let src = "[".repeat(9000) + "x" + &"](u)".repeat(9000) + "\n";
    let _ = carve::to_html(&src); // must return, not throw/abort
    let spans = "[".repeat(9000) + "x" + &"]{.c}".repeat(9000) + "\n";
    let _ = carve::to_html(&spans);
}

#[test]
fn normal_nesting_still_renders() {
    let html = carve::to_html("# H\n\n- a\n  - b\n\n> q\n\n[t](u) /i/ *b*\n");
    assert!(html.contains("<h1>"), "{html}");
    assert!(html.contains("<ul>"), "{html}");
    assert!(html.contains("<blockquote>"), "{html}");
    assert!(html.contains("<em>i</em>"), "{html}");
}

#[test]
fn non_html_renderers_bound_programmatic_block_depth() {
    let mut block = carve::BlockNode::Paragraph(carve::Paragraph {
        attrs: None,
        children: vec![carve::InlineNode::Text("leaf".to_string())],
    });
    for _ in 0..500 {
        block = carve::BlockNode::BlockQuote(carve::BlockQuote {
            attrs: None,
            children: vec![block],
            attribution: None,
        });
    }
    let doc = carve::Document {
        frontmatter: std::collections::BTreeMap::new(),
        footnote_defs: std::collections::BTreeMap::new(),
        children: vec![block],
    };

    let _ = carve::render_markdown(&doc);
    let _ = carve::render_plain_text(&doc);
    let _ = carve::render_ansi(&doc);
}

#[test]
fn non_html_renderers_bound_programmatic_inline_depth() {
    let mut inline = carve::InlineNode::Text("leaf".to_string());
    for _ in 0..500 {
        inline = carve::InlineNode::Emphasis(carve::Emphasis {
            attrs: None,
            kind: carve::EmphasisKind::Italic,
            children: vec![inline],
        });
    }
    let doc = carve::Document {
        frontmatter: std::collections::BTreeMap::new(),
        footnote_defs: std::collections::BTreeMap::new(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            attrs: None,
            children: vec![inline],
        })],
    };

    let _ = carve::render_markdown(&doc);
    let _ = carve::render_plain_text(&doc);
    let _ = carve::render_ansi(&doc);
}
