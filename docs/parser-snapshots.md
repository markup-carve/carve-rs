# Parser snapshots and edits

`parse_snapshot` creates a source-authoritative snapshot. `reparse` applies an
ordered set of UTF-8 byte edits atomically and returns a new document, source
layout, snapshot, and changed ranges.

```rust
use carve::{parse_snapshot, reparse, TextChange};

let first = parse_snapshot("# Title\n\nBody.\n");
let next = reparse(first.snapshot, &[TextChange {
    range: 9..13,
    replacement: "Text".into(),
}])?;

assert_eq!(next.snapshot.source(), "# Title\n\nText.\n");
# Ok::<(), carve::IncrementalParseError>(())
```

The API rejects overlapping, out-of-bounds, and non-UTF-8-boundary changes.
That gives editors one validated update contract and prevents a malformed LSP
change from corrupting parser state.

The current draft establishes snapshot and edit semantics but still performs a
full parse. `reused_previous_tree` is therefore `false`. The value now is a
stable integration boundary for WASM and LSP work; block reuse, stable node
identities, and partial diagnostics can be added without changing how callers
submit edits or receive updates.
