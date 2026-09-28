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

Snapshots cache the previous document. A single edit inside a single-line plain
paragraph reparses that paragraph and reuses the other blocks when the document
contains only plain paragraphs. This subset accepts Unicode letters, numbers,
combining marks, spaces, and sentence punctuation (`.`, `,`, `!`, `?`). Edits
that introduce structure, newlines, or unsupported syntax use a full parse.

`reused_previous_tree` reports whether unchanged blocks were reused.
`parsed_source_bytes` counts bytes passed to the parser, including a failed
local attempt before fallback. An empty edit set parses zero bytes for an eligible plain-paragraph document. Identity
matching uses the cached old document instead of reparsing the old source.

Source maps and snapshots still traverse or copy the document; this is not a
constant-time update API. Codepoint-to-byte mapping uses one prefix index.
Full parsing remains authoritative for references, headings, footnotes,
containers, and multiple edits.
