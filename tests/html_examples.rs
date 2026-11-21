//! C2-H-001: each oracle tree of the specification examples renders to the
//! expected HTML.
mod common;

use carve::ast::owned::Node;
use common::options_of;

#[test]
fn c2_h_001_oracle_trees_render_the_expected_html() {
    let all = common::examples();
    assert!(all.len() > 400, "corpus not read");
    let mut pass = 0;
    let mut failures = Vec::new();
    for ex in &all {
        let Ok(Node::Document(doc)) = serde_json::from_value::<Node>(
            ex.spec_tree
                .clone()
                .unwrap_or_else(|| ex.oracle_tree.clone()),
        ) else {
            failures.push(format!("{}: no document", ex.id));
            continue;
        };
        let arena = carve::Arena::new();
        let html = carve::html::render(&doc.in_arena(&arena), &options_of(&ex.opts));
        if html == ex.html {
            pass += 1;
        } else {
            failures.push(format!(
                "=== {}\n{}\n--- expected\n{}\n--- got\n{}",
                ex.id, ex.input, ex.html, html
            ));
        }
    }
    println!("html: {pass}/{} pass", all.len());
    if let Ok(path) = std::env::var("HTML_FAILURES") {
        let _ = std::fs::write(path, failures.join("\n"));
    }
    assert!(failures.is_empty(), "{} failures", failures.len());
}
