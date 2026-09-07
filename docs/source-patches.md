# Source-preserving patches

Use `to_carve_patch` when a tool needs a stale-safe structured formatting
change:

```rust
let source = "# Title   ";
let patch = carve::to_carve_patch(source);
let formatted = carve::apply_source_patch(source, &patch).unwrap();
```

`create_source_patch` also prepares patches for arbitrary replacements. Ranges
are half-open UTF-8 byte offsets. A byte length and stable `fnv1a64:` fingerprint prevent
application to stale source; edits must be sorted, non-overlapping character
boundaries. The types serialize to the shared camel-case JSON wire shape.

See the [shared contract](https://markup-carve.github.io/carve/source-patches).
