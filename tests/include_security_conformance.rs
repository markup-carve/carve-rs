//! The spec's include SECURITY conformance corpus, run against this engine.
//!
//! `tests/spec/tests/include-security-conformance/` is the processor-neutral
//! executable contract for the security requirements normative in PART 9 §19.
//! It is the gate that would have caught a budget that bounds expansion but not
//! the READS taken to reach it, so it runs here rather than being trusted to
//! prose.
//!
//! Only the `graph` vectors are DRIVEN here: they need no filesystem and they
//! are the ones about this engine's own bookkeeping - resolver calls, visited
//! depth, charged bytes. The `filesystem` and `remote` kinds belong with the
//! filesystem resolver. Every other kind is still ENUMERATED, because the
//! corpus contract says an adapter must fail on a kind it does not know rather
//! than pass by ignoring it.
//!
//! ## What this adapter pins, and why the count is one of them
//!
//! The corpus README requires every adapter to pin the corpus VERSION and the
//! VECTOR COUNT "so accidental omissions fail". This adapter pinned only the
//! version, which is how §19's bound on resolver invocations could be added to
//! the corpus and reach no engine: the two new vectors were read, skipped by a
//! limit the adapter did not deserialize, and nothing said so (carve-rs#1593).
//! So three things are pinned now - the version, the total vector count and the
//! graph count - and the driven count is asserted against the graph count, so a
//! vector cannot be silently dropped on either side of the seam.
//!
//! Unknown requirement ids, unknown kinds and unknown `expected` members all
//! fail, the last via `deny_unknown_fields`. A corpus that grows a new
//! observable therefore stops here instead of being quietly unobserved.
//!
//! ## What is observed, and what is not
//!
//! "Left literal with a Warning" is portable as `status: denied` plus the
//! `denial` CLASS, never as warning text, so that is what is compared. The
//! engine's own rule ids map onto the classes in `denial_class`.
//!
//! `chargedBytes`, `maxVisitedDepth`, `canonicalId` and `remoteFetches` are not
//! COMPARED: `IncludeResult` exposes no byte or depth counter, so this engine
//! cannot answer them without a new public surface. They are still read, by
//! `the_vector_members_belong_to_their_kind`, which asserts that each member
//! appears only on the kinds that may carry it. A member declared and never
//! read would be a field the compiler is entitled to call dead, and a guard
//! nobody exercises is exactly the shape of gap #1593 is about.

use std::cell::RefCell;
use std::collections::BTreeMap;

use carve::includes::{
    expand_includes, IncludeContext, IncludeOptions, IncludeResolved, IncludeResolver,
    IncludeWarning,
};
use carve::parse;

/// The corpus revision this adapter was written against.
const CORPUS_VERSION: u32 = 1;
/// Total vectors in that revision. Raising it is a deliberate act: read the new
/// vectors first and make sure this adapter answers them.
const VECTOR_COUNT: usize = 14;
/// Of which `graph`, the kind this adapter drives.
const GRAPH_VECTOR_COUNT: usize = 6;

/// Every requirement id the corpus may carry. An id outside this list means the
/// corpus grew a requirement this adapter has not been read against.
const KNOWN_REQUIREMENTS: &[&str] = &[
    "S1-opt-in",
    "S2-contained-paths",
    "S3-remote-allowlist",
    "S4-depth-bound",
    "S5-byte-bound",
    "S6-post-budget-no-read",
    "S7-call-bound",
    "S8-post-call-bound-no-read",
];

/// Every vector kind the corpus may carry.
const KNOWN_KINDS: &[&str] = &["activation", "filesystem", "remote", "graph"];

/// Every portable denial class the corpus may expect.
const KNOWN_DENIALS: &[&str] = &[
    "outside-root",
    "remote-not-allowed",
    "depth",
    "budget",
    "resolver-calls",
];

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    version: u32,
    vectors: Vec<Vector>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    name: String,
    #[serde(default)]
    description: String,
    requirement: String,
    kind: String,
    #[serde(default)]
    entry: String,
    #[serde(default)]
    files: Option<BTreeMap<String, String>>,
    #[serde(rename = "maxDepth", default)]
    max_depth: Option<usize>,
    #[serde(rename = "maxBytes", default)]
    max_bytes: Option<usize>,
    #[serde(rename = "maxResolverCalls", default)]
    max_resolver_calls: Option<usize>,
    expected: Expected,
    // Members of the non-graph kinds. Declared so `deny_unknown_fields` fails on
    // a member the corpus INVENTS rather than on one it already had.
    #[serde(default)]
    tree: Option<serde_json::Value>,
    #[serde(default)]
    root: Option<serde_json::Value>,
    #[serde(default)]
    from: Option<serde_json::Value>,
    #[serde(default)]
    request: Option<serde_json::Value>,
    #[serde(default)]
    trusted: Option<serde_json::Value>,
    #[serde(default)]
    enabled: Option<serde_json::Value>,
    #[serde(rename = "allowAbsolute", default)]
    allow_absolute: Option<serde_json::Value>,
    #[serde(rename = "allowedRemoteHosts", default)]
    allowed_remote_hosts: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    #[serde(rename = "resolverCalls", default)]
    resolver_calls: Option<Vec<String>>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    denial: Option<String>,
    // Declared, not observed - see the module doc.
    #[serde(rename = "chargedBytes", default)]
    charged_bytes: Option<usize>,
    #[serde(rename = "maxVisitedDepth", default)]
    max_visited_depth: Option<usize>,
    #[serde(rename = "canonicalId", default)]
    canonical_id: Option<String>,
    #[serde(rename = "remoteFetches", default)]
    remote_fetches: Option<serde_json::Value>,
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

/// This engine's warning rule ids, as the corpus's portable denial classes.
///
/// The first refusal class raised decides it: `include-unresolved` is a
/// per-directive outcome, not a refusal of the render, and a run that then hits
/// a total is denied by that total.
fn denial_class(warnings: &[IncludeWarning]) -> Option<&'static str> {
    warnings
        .iter()
        .find_map(|warning| match warning.rule.as_str() {
            "include-depth" => Some("depth"),
            "include-budget" => Some("budget"),
            "include-call-limit" => Some("resolver-calls"),
            _ => None,
        })
}

#[test]
fn the_corpus_shape_is_pinned() {
    let suite = suite();
    assert_eq!(
        suite.version, CORPUS_VERSION,
        "the adapter pins the corpus version it was written against"
    );
    assert_eq!(
        suite.vectors.len(),
        VECTOR_COUNT,
        "the corpus changed size: read the new vectors before moving the pin"
    );
    assert_eq!(
        suite.vectors.iter().filter(|v| v.kind == "graph").count(),
        GRAPH_VECTOR_COUNT,
        "the graph count moved: this adapter drives exactly these"
    );
    for vector in &suite.vectors {
        assert!(
            KNOWN_KINDS.contains(&vector.kind.as_str()),
            "{}: unknown kind {:?}",
            vector.name,
            vector.kind
        );
        assert!(
            KNOWN_REQUIREMENTS.contains(&vector.requirement.as_str()),
            "{}: unknown requirement {:?}",
            vector.name,
            vector.requirement
        );
        if let Some(denial) = &vector.expected.denial {
            assert!(
                KNOWN_DENIALS.contains(&denial.as_str()),
                "{}: unknown denial class {:?}",
                vector.name,
                denial
            );
        }
        assert!(
            !vector.description.is_empty(),
            "{}: a vector without a description cannot be read",
            vector.name
        );
    }
}

/// Every member appears only on the kinds that may carry it.
///
/// This is what READS the members the graph driver does not compare, so they
/// are load-bearing rather than dead. It also catches the corpus moving a
/// member to a kind this adapter would then silently ignore - `entry` and
/// `files` migrating onto a `filesystem` vector, say, which drives nothing
/// here.
#[test]
fn the_vector_members_belong_to_their_kind() {
    for vector in &suite().vectors {
        let name = &vector.name;
        let kind = vector.kind.as_str();
        let walks_a_graph = matches!(kind, "graph" | "activation");
        assert_eq!(
            !vector.entry.is_empty(),
            walks_a_graph,
            "{name}: `entry` belongs to the kinds walked in memory"
        );
        assert_eq!(
            vector.files.is_some(),
            walks_a_graph,
            "{name}: `files` belongs to the kinds walked in memory"
        );
        assert_eq!(
            vector.tree.is_some(),
            kind == "filesystem",
            "{name}: `tree` is materialized only for a filesystem vector"
        );
        for (member, present) in [
            ("root", vector.root.is_some()),
            ("from", vector.from.is_some()),
        ] {
            assert_eq!(
                present,
                kind == "filesystem",
                "{name}: `{member}` is a filesystem member"
            );
        }
        assert_eq!(
            vector.request.is_some(),
            matches!(kind, "filesystem" | "remote"),
            "{name}: `request` resolves one path, so it belongs to those two kinds"
        );
        for (member, present) in [
            ("trusted", vector.trusted.is_some()),
            ("enabled", vector.enabled.is_some()),
        ] {
            assert_eq!(
                present,
                kind == "activation",
                "{name}: `{member}` states the activation mode"
            );
        }
        assert!(
            vector.allow_absolute.is_none() || kind == "filesystem",
            "{name}: `allowAbsolute` is a containment member"
        );
        assert_eq!(
            vector.allowed_remote_hosts.is_some(),
            kind == "remote",
            "{name}: `allowedRemoteHosts` is a remote member"
        );
        assert!(
            vector.max_resolver_calls.is_none() || kind == "graph",
            "{name}: the call bound is observed on a graph walk"
        );

        let expected = &vector.expected;
        assert!(
            expected.charged_bytes.is_none() || kind == "graph",
            "{name}: `chargedBytes` is a graph observable"
        );
        assert!(
            expected.max_visited_depth.is_none() || kind == "graph",
            "{name}: `maxVisitedDepth` is a graph observable"
        );
        assert!(
            expected.canonical_id.is_none() || kind == "filesystem",
            "{name}: `canonicalId` is a containment observable"
        );
        assert!(
            expected.remote_fetches.is_none() || kind == "remote",
            "{name}: `remoteFetches` is a remote observable"
        );
        assert_eq!(
            expected.resolver_calls.is_some(),
            walks_a_graph,
            "{name}: resolver calls are recorded for the kinds walked in memory"
        );
    }
}

#[test]
fn the_graph_vectors_hold() {
    let suite = suite();
    let graph: Vec<&Vector> = suite.vectors.iter().filter(|v| v.kind == "graph").collect();

    let mut failures = Vec::new();
    let mut driven = 0usize;
    for vector in graph {
        let want =
            vector.expected.resolver_calls.as_ref().unwrap_or_else(|| {
                panic!("{}: a graph vector must state resolverCalls", vector.name)
            });
        driven += 1;
        let resolver = Recording {
            files: vector.files.clone().unwrap_or_default(),
            calls: RefCell::new(Vec::new()),
        };
        let mut opts = IncludeOptions::new().with_resolver(&resolver);
        if let Some(depth) = vector.max_depth {
            opts = opts.with_max_depth(depth);
        }
        if let Some(bytes) = vector.max_bytes {
            opts = opts.with_max_bytes(bytes);
        }
        // THE LINE #1593 WAS ABOUT. Without it the vector runs under this
        // engine's own default of 1000, every directive resolves, and the
        // recorded calls are not the ones the vector pins.
        if let Some(calls) = vector.max_resolver_calls {
            opts = opts.with_max_resolver_calls(calls);
        }
        let result = expand_includes(parse(&vector.entry), &vector.entry, &opts);
        let got = resolver.calls.borrow().clone();
        if &got != want {
            failures.push(format!(
                "{}: resolver calls {got:?}, expected {want:?}",
                vector.name
            ));
        }
        let denial = denial_class(&result.warnings);
        if let Some(expected) = &vector.expected.denial {
            if denial != Some(expected.as_str()) {
                failures.push(format!(
                    "{}: denial class {denial:?}, expected {expected:?}",
                    vector.name
                ));
            }
        }
        if let Some(expected) = &vector.expected.status {
            let status = if denial.is_some() {
                "denied"
            } else {
                "allowed"
            };
            if status != expected {
                failures.push(format!(
                    "{}: status {status:?}, expected {expected:?}",
                    vector.name
                ));
            }
        }
    }

    assert_eq!(
        driven, GRAPH_VECTOR_COUNT,
        "a graph vector was read but not driven"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn suite() -> Suite {
    let raw = include_str!("spec/tests/include-security-conformance/vectors.json");
    serde_json::from_str(raw).expect("the vector file parses against the pinned shape")
}
