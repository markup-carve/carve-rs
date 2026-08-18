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
        let src = "> ".repeat(5000) + "x\n";
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
    // A 648-deep tree is built and DROPPED here, and the recursive Drop
    // is what overflows a default test stack - the same property the
    // other deep-tree tests in this file spawn a big stack for.
    on_big_stack(|| {
        let mut block = carve::BlockNode::Paragraph(carve::Paragraph {
            attrs: None,
            children: vec![carve::InlineNode::text("leaf".to_string())],
            ..Default::default()
        });
        for _ in 0..carve::MAX_RENDER_DEPTH + 16 {
            block = carve::BlockNode::BlockQuote(carve::BlockQuote {
                attrs: None,
                children: vec![block],
                pos: None,
            });
        }
        let doc = carve::Document {
            frontmatter: std::collections::BTreeMap::new(),
            frontmatter_raw: None,
            source_len: 0,
            ingest_payload_len: 0,
            footnote_defs: std::collections::BTreeMap::new(),
            footnote_def_pos: std::collections::BTreeMap::new(),
            children: vec![block],
        };

        // Past the ceiling, so the bound now shows as a REFUSAL rather than
        // as truncated output (PART 9 §25, carve-rs#511 item 5). Both properties
        // still hold: the recursion stayed bounded (no overflow to get here) and
        // the caller is told which renderer stopped and where.
        for (target, rendered) in [
            ("html", carve::render_html(&doc)),
            ("markdown", carve::render_markdown(&doc)),
            ("plain", carve::render_plain_text(&doc)),
            ("ansi", carve::render_ansi(&doc)),
        ] {
            let err = rendered.expect_err("a tree past the ceiling refuses");
            assert_eq!(err.renderer(), target);
            assert_eq!(err.limit(), carve::MAX_RENDER_DEPTH);
        }
    });
}

#[test]
fn non_html_renderers_bound_programmatic_inline_depth() {
    // A 648-deep tree is built and DROPPED here, and the recursive Drop
    // is what overflows a default test stack - the same property the
    // other deep-tree tests in this file spawn a big stack for.
    on_big_stack(|| {
        let mut inline = carve::InlineNode::text("leaf".to_string());
        for _ in 0..carve::MAX_RENDER_DEPTH + 16 {
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
            ingest_payload_len: 0,
            footnote_defs: std::collections::BTreeMap::new(),
            footnote_def_pos: std::collections::BTreeMap::new(),
            children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
                attrs: None,
                children: vec![inline],
                ..Default::default()
            })],
        };

        for (target, rendered) in [
            ("html", carve::render_html(&doc)),
            ("markdown", carve::render_markdown(&doc)),
            ("plain", carve::render_plain_text(&doc)),
            ("ansi", carve::render_ansi(&doc)),
        ] {
            let err = rendered.expect_err("an inline chain past the ceiling refuses");
            assert_eq!(err.renderer(), target);
            assert_eq!(err.limit(), carve::MAX_RENDER_DEPTH);
        }
    });
}

/// The HTML renderer's ceiling had to move off the parse cap too (issue 517).
///
/// `from_json` accepts trees a good deal deeper than the parser produces, and
/// past its ceiling this renderer returns without emitting - so a tree one
/// level past the cap rendered its containers and silently lost everything
/// inside them. The other four renderers were moved off the cap in #462; this
/// one kept the old shape because its constant was already symbolic.
#[test]
fn html_render_ceiling_sits_above_the_parse_cap() {
    on_big_stack(|| {
        let build = |depth: usize| {
            let mut node = carve::BlockNode::Paragraph(carve::Paragraph {
                attrs: None,
                children: vec![carve::InlineNode::text("body".to_string())],
                ..Default::default()
            });
            for _ in 0..depth {
                node = carve::BlockNode::Div(carve::Div {
                    attrs: None,
                    label: None,
                    children: vec![node],
                    pos: None,
                });
            }
            let mut doc = carve::parse("x\n");
            doc.children = vec![node];
            doc
        };

        // 201 is one past the parse cap (MAX_NESTING_DEPTH = 200, not public),
        // which is the depth that used to lose its content.
        let past_cap = carve::render_html(&build(201))
            .expect("the tree under test is within the render ceiling");
        assert!(
            past_cap.contains("body"),
            "content lost one level past the parse cap"
        );

        // The ceiling is still load-bearing, and now says so: past it the
        // renderer refuses instead of returning output with the body deleted
        // (PART 9 §25, carve-rs#511 item 5).
        for depth in [carve::MAX_RENDER_DEPTH + 8, carve::MAX_RENDER_DEPTH + 500] {
            let err = carve::render_html(&build(depth))
                .expect_err("past the ceiling the HTML renderer refuses");
            assert_eq!(err.renderer(), "html");
            assert_eq!(err.limit(), carve::MAX_RENDER_DEPTH);
        }
    });
}
