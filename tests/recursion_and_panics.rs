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
fn list_marker_multibyte_first_char_with_trailing_ws_does_not_panic() {
    // A bullet whose first content char is multibyte, followed by trailing
    // whitespace, made `strip_container_prefixes` compute the structural prefix
    // by length subtraction. `marker_tail` end-trims the trailing whitespace,
    // so the length difference no longer matched the byte offset of the content
    // slice and the cut landed inside the leading multibyte char (`- ́ ` was a
    // 5-byte crash reproducer). The structural length is now the byte offset of
    // the trimmed content within the original line, which is char-boundary safe.
    // carve-js and carve-php both render these as `<ul><li>...</li></ul>`.
    let cases = [
        // (input, expected list-item inner HTML)
        ("- \u{301} ", "\u{301}"),  // combining acute, the original repro
        ("* \u{301} ", "\u{301}"),  // bullet `*` variant
        ("- \u{301}\t", "\u{301}"), // trailing TAB instead of space
        ("- \u{00a0} ", "&nbsp;"),  // NBSP
        ("- \u{1f600} ", "\u{1f600}"), // astral emoji
        ("- \u{200e} ", "\u{200e}"), // bidi LRM control
        ("- \u{feff} ", "\u{feff}"), // BOM / ZWNBSP
    ];
    for (input, inner) in cases {
        let html = carve::to_html(input);
        let expected = format!("<ul>\n  <li>{inner}</li>\n</ul>");
        assert!(
            html.contains(&expected),
            "input {input:?}: expected {expected:?} in {html:?}"
        );
    }
}

/// Run `f` on a worker thread with an ample stack. The block-container cap is
/// MAX_NESTING_DEPTH = 200; a degrading parse still builds an AST up to that
/// depth and the recursive-descent parser/renderer use one native frame per
/// level. In a RELEASE build 200 levels fit comfortably in a default 2 MiB
/// stack (verified), but a DEBUG `cargo test` build has much larger,
/// un-inlined frames, so these worst-case-depth probes get a generous stack.
/// The property under test is the DEGRADATION logic (deep input returns a
/// bounded AST instead of recursing unbounded), not the exact per-frame size.
fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

#[test]
fn deeply_nested_blockquote_degrades_without_overflow() {
    on_big_stack(|| {
        let src = ">".repeat(5000) + " x\n";
        let html = carve::to_html(&src); // must return, not abort
        assert!(html.contains("blockquote"), "expected some quote nesting");
    });
}

#[test]
fn deeply_nested_indented_list_degrades_without_overflow() {
    on_big_stack(|| {
        let mut src = String::new();
        for i in 0..400 {
            src.push_str(&"  ".repeat(i));
            src.push_str("- x\n");
        }
        let _ = carve::to_html(&src); // must return, not abort
    });
}

#[test]
fn deeply_nested_inline_brackets_degrade_without_overflow() {
    on_big_stack(|| {
        let src = "[".repeat(9000) + "x" + &"](u)".repeat(9000) + "\n";
        let _ = carve::to_html(&src); // must return, not throw/abort
        let spans = "[".repeat(9000) + "x" + &"]{.c}".repeat(9000) + "\n";
        let _ = carve::to_html(&spans);
    });
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
        children: vec![carve::InlineNode::text("leaf".to_string())],
        ..Default::default()
    });
    for _ in 0..500 {
        block = carve::BlockNode::BlockQuote(carve::BlockQuote {
            attrs: None,
            children: vec![block],
            attribution: None,
            pos: None,
        });
    }
    let doc = carve::Document {
        frontmatter: std::collections::BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        footnote_defs: std::collections::BTreeMap::new(),
        children: vec![block],
    };

    let _ = carve::render_markdown(&doc);
    let _ = carve::render_plain_text(&doc);
    let _ = carve::render_ansi(&doc);
}

#[test]
fn non_html_renderers_bound_programmatic_inline_depth() {
    let mut inline = carve::InlineNode::text("leaf".to_string());
    for _ in 0..500 {
        inline = carve::InlineNode::Emphasis(carve::Emphasis {
            attrs: None,
            kind: carve::EmphasisKind::Italic,
            children: vec![inline],
            pos: None,
        });
    }
    let doc = carve::Document {
        frontmatter: std::collections::BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        footnote_defs: std::collections::BTreeMap::new(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            attrs: None,
            children: vec![inline],
            ..Default::default()
        })],
    };

    let _ = carve::render_markdown(&doc);
    let _ = carve::render_plain_text(&doc);
    let _ = carve::render_ansi(&doc);
}
