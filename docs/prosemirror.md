# ProseMirror / Tiptap


The AST converts to a ProseMirror document and back, so a Tiptap editor and this
engine can share one stored document:

```rust
let doc = carve::parse(source);
let editor = carve::to_prosemirror(&doc);
let back = carve::from_prosemirror(&editor.json)?;
```

Node and mark names come from the map carve-grammars publishes, vendored under
`resources/` with the commit it was copied from, rather than restated here - the
same arrangement carve-php uses. Every name in the conversion is read from it;
none is written out, and a test fails if one is.

The editor model is smaller than Carve's AST, so `to_prosemirror` reports what
it could not carry rather than losing it quietly:

```rust
let editor = carve::to_prosemirror(&doc);
if !editor.dropped.is_empty() || !editor.degraded.is_empty() {
    // `dropped` - the content is gone.
    // `degraded` - the node type is gone, its text survives: a soft break
    //   becomes a space, an escape becomes the character it escaped.
}
```

An application that stores what the editor returns should assert both are empty
before saving. Going the other way, a ProseMirror name the map does not know is
an **error**, not a skip: an editor that grew a node type nobody mapped is
exactly where a quiet skip destroys the most content.

On the shared corpus, 791 documents report nothing lost and round-trip to
byte-identical HTML; 215 report what they lost. The spec's
[format bridges](https://markup-carve.github.io/carve/format-bridges) page has
the reasoning behind the arrangement.

---

[Back to the README](../README.md)
