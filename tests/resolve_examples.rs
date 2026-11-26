//! C2-A-133: resolving each oracle tree with its result fields stripped gives
//! the oracle tree back.
mod common;
#[path = "support/holds.rs"]
mod holds;

use carve::ast::owned::Node;
use carve::options::Options;
use holds::{Gate, Holds};
use serde_json::Value;

pub fn options_of(opts: &Value) -> Options {
    let mut o = Options::default();
    if let Some(h) = opts["headingIds"]
        .as_str()
        .and_then(carve::options::HeadingIds::from_name)
    {
        o.heading_ids = h;
    }
    o
}

/// `(example id, cause)`.
const HELD: Holds = &[];

#[test]
fn c2_a_133_resolve_restores_the_oracle_results() {
    let mut gate = Gate::new("resolve examples", HELD);
    let all = common::examples();
    assert!(all.len() > 400, "corpus not read");
    for ex in &all {
        let expected = ex.spec_tree.as_ref().unwrap_or(&ex.oracle_tree);
        let mut stripped = expected.clone();
        common::strip_results(&mut stripped);
        let Ok(Node::Document(doc)) = serde_json::from_value::<Node>(stripped) else {
            gate.check(&ex.id, false, || "no document".into());
            continue;
        };
        let arena = carve::Arena::new();
        let mut doc = doc.in_arena(&arena);
        carve::resolve::resolve(&mut doc, &options_of(&ex.opts), &arena);
        let back = serde_json::to_value(Node::Document(Box::new(doc.owned()))).unwrap_or_default();
        gate.check(
            &ex.id,
            common::sorted(&back) == common::sorted(expected),
            || "the resolved tree differs".into(),
        );
    }
    gate.finish();
}
