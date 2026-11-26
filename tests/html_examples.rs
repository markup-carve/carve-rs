//! C2-H-001: each oracle tree of the specification examples renders to the
//! expected HTML.
mod common;
#[path = "support/holds.rs"]
mod holds;

use carve::ast::owned::Node;
use common::options_of;
use holds::{Gate, Holds};

/// `(example id, cause)`.
const HELD: Holds = &[];

#[test]
fn c2_h_001_oracle_trees_render_the_expected_html() {
    let all = common::examples();
    assert!(all.len() > 400, "corpus not read");
    let mut gate = Gate::new("html examples", HELD);
    for ex in &all {
        let Ok(Node::Document(doc)) = serde_json::from_value::<Node>(
            ex.spec_tree
                .clone()
                .unwrap_or_else(|| ex.oracle_tree.clone()),
        ) else {
            gate.check(&ex.id, false, || "no document".into());
            continue;
        };
        let arena = carve::Arena::new();
        let html = carve::html::render(&doc.in_arena(&arena), &options_of(&ex.opts));
        gate.check(&ex.id, html == ex.html, || {
            format!("{}\n--- expected\n{}\n--- got\n{}", ex.input, ex.html, html)
        });
    }
    let judged = gate.judge();
    if let Ok(path) = std::env::var("HTML_FAILURES") {
        let _ = std::fs::write(path, judged.lines().join("\n"));
    }
    judged.assert();
}
