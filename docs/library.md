# Rust library API

## Library use

```rust
let html = carve::to_html("# Hello\n\nThis is /italic/ and *bold*.");
assert!(html.contains("<h1>Hello</h1>"));
```

For lower-level access, `carve::parse` returns a typed `Document` AST and `carve::render_html` walks it to HTML:

```rust
let doc = carve::parse(source);
// inspect or transform doc.children …
let html = carve::render_html(&doc)?;
```

`carve::render_html_owned(doc)` consumes the document and avoids cloning its tree.
Use `carve::render_html_owned_with_options(doc, &options)` to pass rendering options.
Both return the same depth error as the borrowed renderer.

Besides HTML, the crate renders the same AST to Markdown, plain text, and
ANSI-styled text via `carve::to_markdown`, `carve::to_plain_text`, and
`carve::to_ansi` (each with a matching `render_*` function for a parsed
`Document`).

Raw nodes are routed to their named target. Use a checked sibling when omitted
content must be observable:

```rust
let result = carve::to_html_with_report(
    "`x`{=latex}",
    carve::CheckedRenderOptions::default(),
)?;
assert_eq!(result.losses[0].code, "raw-format-dropped");
```

Set `strict: true` to return `RenderLossError` before a value is published.
Reports keep the complete count and retain 100 positioned entries by default;
the existing string-returning functions remain available.

The report includes `raw-format-dropped` for a raw node the target
omits, `ruby-flattened` for a `ruby` node a target cannot carry natively,
`editorial-comment-flattened` for an editorial comment (`{#note#}`) that the
Plain or ANSI target writes as bare text, and `destination-denied` for a URL the
PART 9 section 25 sink denylist blanked. The HTML target also reports
`carrier-attribute-dropped` when a roundtrip carrier displaces an authored
attribute, with its name in `attribute` and no `format`.
Markdown export reports `definition-list-flattened` for a nested definition
list it cannot carry and `list-style-dropped` when it writes an alphabetic or
roman ordered list as decimal. The latter row names the style in `style`.
It also reports `markdown-container-flattened` for a `div`,
`markdown-caption-flattened` for figure or table captions,
`markdown-attributes-dropped` for block attributes without Markdown syntax,
and `raw-html-escaped` for raw HTML written as escaped text. A source-based
checked render reports an unattached block attribute that parsing discards;
an AST-only render has no source run to report. Roundtrip Markdown carries
containers, attributes, and captions, so those three losses are absent there.
Carve output reports `list-tightness-lost` when a nested item's blocks require
the writer to make that item loose.
A link destination, an autolink destination and an image source each take their
own row on the targets that emit a destination, in both safe modes. The emitted
value does not change: `href=""` is what it always was, and the row is what
tells a host rendering untrusted input which link it defused. The row's
`message` is fixed by the spec and names the sink: `Blanked a denied
destination scheme` for a link or autolink destination, `Blanked a denied image
source` for an image source. Nothing is appended, because `target` is already
its own field on the row.

Source-aware tools can prepare canonical formatting with `to_carve_patch` and
apply it with `apply_source_patch`; see
[source-preserving patches](https://github.com/markup-carve/carve-rs/blob/main/docs/source-patches.md).

## AST JSON errors

`AstJsonError::kind()` distinguishes syntax, depth, unknown-field, and other AST refusals. `path()` identifies an unknown field; syntax errors expose `line()`, `column()`, and the original serde error through `std::error::Error::source()`. The compatibility `to_json` and `markdown_to_carve` functions panic on refusal.
