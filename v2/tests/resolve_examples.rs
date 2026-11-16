//! C2-A-133: resolving each oracle tree with its result fields stripped gives
//! the oracle tree back.
mod common;

use carve2::ast::owned::Node;
use carve2::options::Options;
use serde_json::Value;

pub fn options_of(opts: &Value) -> Options {
    let mut o = Options::default();
    if let Some(h) = opts["headingIds"]
        .as_str()
        .and_then(carve2::options::HeadingIds::from_name)
    {
        o.heading_ids = h;
    }
    o
}

#[test]
fn c2_a_133_resolve_restores_the_oracle_results() {
    let mut pass = 0;
    let mut failures = Vec::new();
    let all = common::examples();
    assert_eq!(all.len(), 404, "fixture not read");
    for ex in &all {
        let expected = ex.spec_tree.as_ref().unwrap_or(&ex.oracle_tree);
        let mut stripped = expected.clone();
        common::strip_results(&mut stripped);
        let Ok(Node::Document(doc)) = serde_json::from_value::<Node>(stripped) else {
            failures.push(format!("{}: no document", ex.id));
            continue;
        };
        let arena = carve2::Arena::new();
        let mut doc = doc.in_arena(&arena);
        carve2::resolve::resolve(&mut doc, &options_of(&ex.opts), &arena);
        let back = serde_json::to_value(Node::Document(Box::new(doc.owned()))).unwrap_or_default();
        if common::sorted(&back) == common::sorted(expected) {
            pass += 1;
        } else {
            failures.push(ex.id.clone());
        }
    }
    println!("resolve: {pass}/{} pass", all.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
