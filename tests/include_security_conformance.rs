//! The spec's include SECURITY conformance corpus, run against this engine.
//!
//! `tests/spec/tests/include-security-conformance/` is the processor-neutral
//! executable contract for the security requirements normative in PART 9 §19.
//! It is the gate that would have caught a budget that bounds expansion but not
//! the READS taken to reach it, so it runs here rather than being trusted to
//! prose.
//!
//! Only the `graph` vectors are driven here: they need no filesystem and they
//! are the ones about this engine's own bookkeeping - resolver calls, visited
//! depth, charged bytes. The `filesystem` and `remote` kinds belong with the
//! filesystem resolver.

use std::cell::RefCell;
use std::collections::BTreeMap;

use carve::includes::{
    expand_includes, IncludeContext, IncludeOptions, IncludeResolved, IncludeResolver,
};
use carve::parse;

#[derive(serde::Deserialize)]
struct Suite {
    version: u32,
    vectors: Vec<Vector>,
}

#[derive(serde::Deserialize)]
struct Vector {
    name: String,
    kind: String,
    #[serde(default)]
    entry: String,
    #[serde(default)]
    files: BTreeMap<String, String>,
    #[serde(rename = "maxDepth", default)]
    max_depth: Option<usize>,
    #[serde(rename = "maxBytes", default)]
    max_bytes: Option<usize>,
    expected: Expected,
}

#[derive(serde::Deserialize)]
struct Expected {
    #[serde(rename = "resolverCalls", default)]
    resolver_calls: Option<Vec<String>>,
}

/// Records what it was asked for, which is the whole point: these vectors gate
/// the CALLS, not just the output.
struct Recording {
    files: BTreeMap<String, String>,
    calls: RefCell<Vec<String>>,
}

impl IncludeResolver for Recording {
    fn resolve(&self, path: &str, _ctx: &IncludeContext<'_>) -> Option<IncludeResolved> {
        self.calls.borrow_mut().push(path.to_string());
        self.files
            .get(path)
            .map(|source| IncludeResolved::with_id(source.clone(), path))
    }
}

#[test]
fn the_graph_vectors_hold() {
    let raw = include_str!("spec/tests/include-security-conformance/vectors.json");
    let suite: Suite = serde_json::from_str(raw).expect("the vector file parses");
    assert_eq!(
        suite.version, 1,
        "the adapter pins the corpus version it was written against"
    );

    let graph: Vec<&Vector> = suite.vectors.iter().filter(|v| v.kind == "graph").collect();
    assert!(
        !graph.is_empty(),
        "the suite must carry graph vectors, or this gate checks nothing"
    );

    let mut failures = Vec::new();
    for vector in graph {
        let want = match &vector.expected.resolver_calls {
            Some(calls) => calls,
            None => continue,
        };
        let resolver = Recording {
            files: vector.files.clone(),
            calls: RefCell::new(Vec::new()),
        };
        let mut opts = IncludeOptions::new().with_resolver(&resolver);
        if let Some(depth) = vector.max_depth {
            opts = opts.with_max_depth(depth);
        }
        if let Some(bytes) = vector.max_bytes {
            opts = opts.with_max_bytes(bytes);
        }
        expand_includes(parse(&vector.entry), &vector.entry, &opts);
        let got = resolver.calls.borrow().clone();
        if &got != want {
            failures.push(format!(
                "{}: resolver calls {got:?}, expected {want:?}",
                vector.name
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
