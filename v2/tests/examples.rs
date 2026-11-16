//! The gate of stage 1: every specification example, from source, gives the
//! expected HTML of examples.json, and the tree equals the oracle's.
mod common;

use common::{options_of, parse_json, sorted};

fn report(name: &str, pass: usize, total: usize, failures: &[String]) {
    println!("{name}: {pass}/{total} pass");
    if let Ok(dir) = std::env::var("EXAMPLE_FAILURES") {
        let _ = std::fs::write(format!("{dir}/{name}.txt"), failures.join("\n"));
    }
}

#[test]
fn c2_examples_render_the_expected_html() {
    let all = common::examples();
    assert_eq!(all.len(), 411, "fixture not read");
    let mut failures = Vec::new();
    for ex in &all {
        let html = carve2::render(&ex.input, &options_of(&ex.opts));
        if html != ex.html {
            failures.push(format!(
                "=== {}\n{}\n--- expected\n{}\n--- got\n{}",
                ex.id, ex.input, ex.html, html
            ));
        }
    }
    report("html", all.len() - failures.len(), all.len(), &failures);
    assert!(failures.is_empty(), "{} failures", failures.len());
}

#[test]
fn c2_a_examples_give_the_spec_tree() {
    let all = common::examples();
    let mut failures = Vec::new();
    for ex in &all {
        let arena = carve2::Arena::new();
        let doc = carve2::parse_resolved_in(&ex.input, &options_of(&ex.opts), &arena).unwrap();
        let got = parse_json(&carve2::to_json(&doc));
        let want = ex.spec_tree.as_ref().unwrap_or(&ex.oracle_tree);
        let (want, got) = (sorted(want), sorted(&got));
        if want != got {
            failures.push(format!(
                "=== {}\n{}\n--- expected\n{want}\n--- got\n{got}",
                ex.id, ex.input
            ));
        }
    }
    report("tree", all.len() - failures.len(), all.len(), &failures);
    assert!(failures.is_empty(), "{} failures", failures.len());
}
