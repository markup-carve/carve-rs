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

Accepted HTML is delivered in newline-terminated chunks, with any final
unterminated line delivered last. Concatenating the chunks reproduces the
renderer output exactly. Empty accepted output calls the sink once with an
empty string. Sink failures propagate to the caller.

The complete HTML string is buffered before the first callback. This API
measures acceptance and controls delivery; parsing and rendering still finish
before delivery starts. Borrowed events and unbuffered rendering remain future
work.
