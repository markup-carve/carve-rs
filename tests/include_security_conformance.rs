//! The spec's include SECURITY conformance corpus, run against this engine.
//!
//! `tests/spec/tests/include-security-conformance/` is the processor-neutral
//! executable contract for the security requirements normative in PART 9 §19.
//! It is the gate that would have caught a budget that bounds expansion but not
//! the READS taken to reach it, so it runs here rather than being trusted to
//! prose.
//!
//! Two groups of vectors are DRIVEN here. The `graph` kind needs no filesystem
//! and is about this engine's own bookkeeping - resolver calls, visited depth,
//! charged bytes. The `S9-root-configuration` vectors are about where the
//! containment root COMES FROM, so they are driven through the seam this engine
//! exposes for a configured value, `FileSystemResolver::new`. The remaining
//! `filesystem` vectors and the `remote` kind are still only ENUMERATED,
//! because the corpus contract says an adapter must fail on a kind it does not
//! know rather than pass by ignoring it.
//!
//! ## What this adapter pins, and why the count is one of them
//!
//! The corpus README requires every adapter to pin the corpus VERSION and the
//! VECTOR COUNT "so accidental omissions fail". This adapter pinned only the
//! version, which is how §19's bound on resolver invocations could be added to
//! the corpus and reach no engine: the two new vectors were read, skipped by a
//! limit the adapter did not deserialize, and nothing said so (carve-rs#1593).
//! So four things are pinned now - the version, the total vector count, the
//! graph count and the root-spec count - and each driven count is asserted
//! against its group, so a vector cannot be silently dropped on either side of
//! the seam.
//!
//! Unknown requirement ids, unknown kinds, unknown denial classes and unknown
//! `expected` members all fail, the last via `deny_unknown_fields`. A corpus
//! that grows a new observable therefore stops here instead of being quietly
//! unobserved.
//!
//! ## `rootSpec` goes through the seam UNCHANGED
//!
//! `root` is the ADAPTER's: a path it materializes and canonicalizes before
//! handing it over, so containment is the only question left. `rootSpec` is the
//! value the HOST was configured with, and what this engine's own root
//! configuration materializes it to is the behavior under test - so it reaches
//! `FileSystemResolver::new` verbatim. Canonicalizing it in the adapter first
//! would answer the vector with the adapter's own `canonicalize`, which is the
//! defect the vector exists for. Expanding the corpus's `<ABS:>` notation is
//! the corpus spelling out its temporary tree, not a canonicalization. A vector
//! names the root exactly one way and the corpus schema refuses both at once;
//! `the_vector_members_belong_to_their_kind` pins that here too.
//!
//! ## What is observed, and what is not
//!
//! "Left literal with a Warning" is portable as `status: denied` plus the
//! `denial` CLASS, never as warning text, so that is what is compared. The
//! engine's own rule ids map onto the classes in `denial_class`.
//!
//! This engine raises ONE rule id for every filesystem refusal,
//! `include-unresolved`, so `outside-root` and `not-found` are a single
//! observable here - which is why the containment vectors stay enumerated. What
//! IS observable is whether a configured value became a root at all, and that
//! is exactly what `S9-root-configuration` pins. Those vectors therefore
//! compare `status`, `resolverCalls` and `canonicalId`, and compare the
//! `denial` class only where this engine can tell it apart; a class outside
//! `INDISTINGUISHABLE_FILESYSTEM_DENIALS` fails rather than falling through.
//!
//! `chargedBytes`, `maxVisitedDepth` and `remoteFetches` are not COMPARED:
//! `IncludeResult` exposes no byte or depth counter, so this engine cannot
//! answer them without a new public surface. They are still read, by
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
const VECTOR_COUNT: usize = 23;
/// Of which `graph`, the kind this adapter drives in memory.
const GRAPH_VECTOR_COUNT: usize = 6;
/// Of which name their root as a `rootSpec`, driven through the configuration
/// seam rather than through a root the adapter already materialized.
const ROOT_SPEC_VECTOR_COUNT: usize = 5;

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
    "S9-root-configuration",
];

/// Every vector kind the corpus may carry.
const KNOWN_KINDS: &[&str] = &["activation", "filesystem", "remote", "graph"];

/// Every portable denial class the corpus may expect.
const KNOWN_DENIALS: &[&str] = &[
    "outside-root",
    "not-found",
    "no-root",
    "remote-not-allowed",
    "depth",
    "budget",
    "resolver-calls",
];

/// The two classes this engine cannot tell apart.
///
/// `FileSystemResolver` answers a refusal and a miss the same way - `None`,
/// reported as the one canonical rule id `include-unresolved` - so a driven
/// vector expecting either is compared on `status` alone. Naming them rather
/// than defaulting to "do not compare" is what makes a THIRD class arriving on
/// a driven vector fail here instead of passing unobserved.
#[cfg(feature = "fs")]
const INDISTINGUISHABLE_FILESYSTEM_DENIALS: &[&str] = &["outside-root", "not-found"];

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
    /// The value the HOST was configured with. Typed rather than opaque,
    /// because unlike `root` this one is USED: it is handed to the engine's
    /// configuration seam unchanged.
    #[serde(rename = "rootSpec", default)]
    root_spec: Option<String>,
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
    #[serde(rename = "canonicalId", default)]
    canonical_id: Option<String>,
    // Declared, not observed - see the module doc.
    #[serde(rename = "chargedBytes", default)]
    charged_bytes: Option<usize>,
    #[serde(rename = "maxVisitedDepth", default)]
    max_visited_depth: Option<usize>,
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
    assert_eq!(
        suite
            .vectors
            .iter()
            .filter(|v| v.root_spec.is_some())
            .count(),
        ROOT_SPEC_VECTOR_COUNT,
        "the root-spec count moved: this adapter drives exactly these through the seam"
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
/// This is what READS the members the drivers do not compare, so they are
/// load-bearing rather than dead. It also catches the corpus moving a member to
/// a kind this adapter would then silently ignore - `entry` and `files`
/// migrating onto a `filesystem` vector, say, which walks no graph here.
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
        // A vector names the root exactly ONE way, and the corpus schema
        // refuses both at once: `root` is materialized here, `rootSpec` goes
        // through the engine's own configuration seam, and the two answer
        // different questions.
        assert_eq!(
            vector.root.is_some() ^ vector.root_spec.is_some(),
            kind == "filesystem",
            "{name}: a filesystem vector names its root exactly one way, and no other kind names one"
        );
        assert_eq!(
            vector.from.is_some(),
            kind == "filesystem",
            "{name}: `from` is a filesystem member"
        );
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
        // A configured value that names no root leaves no resolver to call, and
        // the corpus states that emptiness rather than leaving it implied - so
        // a `no-root` vector carries `resolverCalls` exactly as a walk does.
        assert_eq!(
            expected.resolver_calls.is_some(),
            walks_a_graph || expected.denial.as_deref() == Some("no-root"),
            "{name}: resolver calls are recorded for the kinds walked in memory and for a value that configures no root"
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

// ---------------------------------------------------------------------------
// S9-root-configuration: the configured value, through the real seam
// ---------------------------------------------------------------------------

/// A temporary tree, removed when the vector is done with it.
#[cfg(feature = "fs")]
struct TmpTree(std::path::PathBuf);

#[cfg(feature = "fs")]
impl TmpTree {
    fn new(name: &str) -> Self {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "carve-isc-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        std::fs::create_dir_all(&base).expect("tmp base");
        Self(base)
    }

    fn base(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(feature = "fs")]
impl Drop for TmpTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Materialize a vector's `tree`. Only plain file content appears on the
/// root-spec vectors; a `{ "symlink": … }` entry belongs to the containment
/// vectors this adapter does not drive, so it panics rather than being skipped.
#[cfg(feature = "fs")]
fn materialize_tree(base: &std::path::Path, tree: &serde_json::Value, name: &str) {
    let entries = tree
        .as_object()
        .unwrap_or_else(|| panic!("{name}: `tree` must be an object"));
    for (rel, value) in entries {
        let content = value
            .as_str()
            .unwrap_or_else(|| panic!("{name}: tree entry {rel} is not file content"));
        let abs = base.join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).expect("tree parent");
        }
        std::fs::write(&abs, content).expect("tree file");
    }
}

/// Bind the corpus's `<ABS:path>` notation to this run's temporary tree.
///
/// This is the corpus spelling out where its tree landed, NOT a
/// canonicalization: the base is used exactly as `temp_dir` gave it, so what
/// the seam receives is the configured value and nothing else.
#[cfg(feature = "fs")]
fn bind_abs(spec: &str, base: &std::path::Path) -> String {
    match spec
        .strip_prefix("<ABS:")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        Some(rel) => base.join(rel).to_string_lossy().into_owned(),
        None => spec.to_string(),
    }
}

/// Delegates to the real resolver and records every path it was asked for.
#[cfg(feature = "fs")]
struct RecordingFs {
    inner: carve::FileSystemResolver,
    calls: RefCell<Vec<String>>,
}

#[cfg(feature = "fs")]
impl IncludeResolver for RecordingFs {
    fn resolve(&self, path: &str, ctx: &IncludeContext<'_>) -> Option<IncludeResolved> {
        self.calls.borrow_mut().push(path.to_string());
        self.inner.resolve(path, ctx)
    }
}

/// The `S9-root-configuration` vectors, driven through `FileSystemResolver::new`.
///
/// That constructor IS this engine's root-configuration seam: it is the only
/// public surface that turns a configured value into a root, so it is where the
/// vector's question is answered. The spec reaches it verbatim.
#[cfg(feature = "fs")]
#[test]
fn the_root_spec_vectors_hold() {
    let suite = suite();
    let mut failures = Vec::new();
    let mut driven = 0usize;

    for vector in suite.vectors.iter().filter(|v| v.root_spec.is_some()) {
        let name = &vector.name;
        assert_eq!(
            vector.kind, "filesystem",
            "{name}: a root spec is configured for a filesystem vector"
        );
        assert!(
            vector.allow_absolute.is_none(),
            "{name}: this driver does not widen absolute spellings"
        );
        driven += 1;

        let tmp = TmpTree::new(name);
        materialize_tree(
            tmp.base(),
            vector
                .tree
                .as_ref()
                .expect("a filesystem vector has a tree"),
            name,
        );

        let spec = bind_abs(vector.root_spec.as_ref().expect("root spec"), tmp.base());
        let from = vector
            .from
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{name}: `from` names the including file"));
        let request = vector
            .request
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{name}: `request` names one target"));

        // THE SPEC IS NOT TOUCHED. Whatever `FileSystemResolver::new` makes of
        // it - a root, or an error that leaves the host with no resolver at all
        // - is the answer the vector asks for.
        let configured = carve::FileSystemResolver::new(&spec).ok();

        let source_path = std::fs::canonicalize(tmp.base().join(from))
            .expect("the including file exists in the tree")
            .to_string_lossy()
            .into_owned();
        let entry = format!("{{{{ {request} }}}}\n");

        let recording = configured.map(|inner| RecordingFs {
            inner,
            calls: RefCell::new(Vec::new()),
        });
        let mut opts = IncludeOptions::new().with_source_path(source_path);
        if let Some(resolver) = &recording {
            opts = opts.with_resolver(resolver);
        }
        let result = expand_includes(parse(&entry), &entry, &opts);

        // With no root there is no resolver, and `dependencies` is empty
        // without one, so the two branches below never overlap.
        let (status, denial, canonical_id) = match &recording {
            None => ("denied", Some("no-root"), None),
            Some(_) => match result.dependencies.as_slice() {
                [dep] if dep.resolved => ("allowed", None, Some(dep.id.clone())),
                [_] => ("denied", None, None),
                other => {
                    failures.push(format!(
                        "{name}: one directive should report one dependency, got {}",
                        other.len()
                    ));
                    continue;
                }
            },
        };

        if let Some(expected) = &vector.expected.status {
            if status != expected {
                failures.push(format!("{name}: status {status:?}, expected {expected:?}"));
            }
        }

        match (&vector.expected.denial, denial) {
            (None, None) => {}
            (Some(expected), Some(got)) if expected == got => {}
            // `outside-root` and `not-found` are one observable here (see the
            // module doc); `status` above is what carries those vectors.
            (Some(expected), None)
                if INDISTINGUISHABLE_FILESYSTEM_DENIALS.contains(&expected.as_str())
                    && status == "denied" => {}
            (expected, got) => {
                failures.push(format!(
                    "{name}: denial class {got:?}, expected {expected:?}"
                ));
            }
        }

        if let Some(want) = &vector.expected.resolver_calls {
            let got = recording
                .as_ref()
                .map(|r| r.calls.borrow().clone())
                .unwrap_or_default();
            if &got != want {
                failures.push(format!("{name}: resolver calls {got:?}, expected {want:?}"));
            }
        }

        if let Some(want) = &vector.expected.canonical_id {
            // `<ROOT>` is the root the spec named, which is also the one path
            // prefix a committed vector may not hard-code.
            let root_real = std::fs::canonicalize(&spec).expect("an honored root exists");
            let got = canonical_id
                .as_deref()
                .map(|id| id.replace(root_real.to_string_lossy().as_ref(), "<ROOT>"));
            if got.as_deref() != Some(want.as_str()) {
                failures.push(format!("{name}: canonical id {got:?}, expected {want:?}"));
            }
        }
    }

    assert_eq!(
        driven, ROOT_SPEC_VECTOR_COUNT,
        "a root-spec vector was read but not driven"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn suite() -> Suite {
    let raw = include_str!("spec/tests/include-security-conformance/vectors.json");
    serde_json::from_str(raw).expect("the vector file parses against the pinned shape")
}
