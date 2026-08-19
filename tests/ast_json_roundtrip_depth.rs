//! `from_json` must accept whatever `to_json` produced, at any depth the parser
//! itself allows (carve-rs#389).
//!
//! The two caps count different things: the parser bounds NODE depth, the JSON
//! reader bounds raw structural depth, and a node costs two structural levels
//! (its object, then its `children` array). A reader budget equal to the
//! parser's therefore rejected ASTs this crate had just emitted - the round trip
//! failed at about 99 containers while the parser nested 200.
//!
//! This asserts the property rather than a number, so it keeps holding if either
//! cap moves.

/// Nest `n` containers, each fence one colon wider than the one inside it.
fn nested_containers(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&":".repeat(n - i + 2));
        s.push_str(&format!(" d{i}\n\n"));
    }
    s.push_str("X\n\n");
    for i in (0..n).rev() {
        s.push_str(&":".repeat(n - i + 2));
        s.push_str("\n\n");
    }
    s
}

/// A release build fits 200 levels in a default 2 MiB stack, but a debug
/// `cargo test` build has much larger un-inlined frames, so a worst-case-depth
/// probe needs a generous one - the same reason `recursion_and_panics.rs` spawns
/// a worker. The property under test is the ROUND TRIP, not the frame size.
fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

#[test]
fn from_json_accepts_what_to_json_produced_at_the_parsers_own_depth_limit() {
    on_big_stack(|| {
        // 200 is the parser's MAX_NESTING_DEPTH. Anything it can emit, the reader
        // must ingest - a document that survives `parse` and `to_json` must survive
        // `from_json`.
        for depth in [40, 95, 100, 150, 200] {
            let source = nested_containers(depth);
            let doc = carve::parse(&source);
            let json = carve::to_json(&doc);
            assert!(
                carve::from_json(&json).is_ok(),
                "from_json rejected an AST to_json produced at depth {depth}",
            );
        }
    });
}

#[test]
fn the_encoder_refuses_an_api_tree_past_its_depth_budget() {
    on_big_stack(|| {
        let mut inline = carve::InlineNode::text("leaf");
        for _ in 0..2_000 {
            inline = carve::InlineNode::Emphasis(carve::Emphasis {
                attrs: None,
                kind: carve::EmphasisKind::Italic,
                children: vec![inline],
                pos: None,
            });
        }
        let mut doc = carve::parse("leaf\n");
        let carve::BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
            unreachable!()
        };
        paragraph.children = vec![inline];

        let error = carve::try_to_json(&doc).expect_err("deep API tree must refuse");
        assert!(error.to_string().contains("encoder's depth budget"));
    });
}
