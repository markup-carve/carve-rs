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

Accepted HTML is delivered in UTF-8 chunks of at most 4096 bytes. Chunks end
at newlines where possible; long lines span multiple chunks. Concatenating the
chunks reproduces the renderer output exactly. Empty accepted output calls the
sink once with an empty string. Sink panics propagate to the caller.

A validation pass discards output before any callback runs. A second pass writes
directly to the bounded output buffer, without assembling the complete HTML
string. Source line indexes and reference definitions still use memory
proportional to the input. The borrowed layout subset is conservative;
unsupported syntax and rendering options return `NeedsAst`.
