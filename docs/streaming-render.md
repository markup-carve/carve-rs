# Streaming render boundary

`try_render_html_streaming` exposes whether the borrowed layout renderer can
authoritatively handle a document.

```rust
use carve::{try_render_html_streaming, Options, StreamOutcome};

let mut html = String::new();
let outcome = try_render_html_streaming("# Title\n", &Options::default(), |chunk| {
    html.push_str(chunk);
});
assert_eq!(outcome, StreamOutcome::Complete);
```

When the result is `NeedsAst`, the sink has not been called. A server can safely
fall back to the normal AST renderer without retracting partial output. This
explicit boundary is valuable for low-allocation render services and makes
fallback rates measurable instead of hiding them inside `to_html`.

The current draft emits one complete accepted chunk. It does not yet claim a
public event parser or time-to-first-byte improvement. Borrowed events,
multi-chunk delivery, capability reporting, and WASM adapters remain follow-up
work.
