# Development

Building the crate, what a binding that pins this engine has to know, and the design the parser follows.

## Building from source


```bash
git clone https://github.com/markup-carve/carve-rs
cd carve-rs
git submodule update --init   # pulls the spec corpus
cargo test
```

The spec corpus lives in `tests/spec/` as a git submodule of [`markup-carve/carve`](https://github.com/markup-carve/carve). Running `cargo test` without initializing the submodule will fail with a clear error message.

## For bindings that pin this engine


carve-rb, carve-py, carve-wasm and carve-go each pin a carve-rs revision - three
as a Cargo git dependency (the crate publishes as `carve-lang`, not `carve`), one
as a revision file beside a prebuilt wasm. `tools/check-engine-pin.py` is the
single reader for both shapes: it asserts the pin names a real commit that is an
ancestor of `main`, that a lockfile agrees with its manifest, and - optionally -
that a committed artifact matches the digest recorded beside its revision. The
lag behind `main` is reported as a number and never gates; `--max-age-days`
gates on age instead.

See [engine-pin-guard.md](engine-pin-guard.md) for the CI snippet each
binding should use and for what the guard cannot see.

## Design


- **Linear-time** parsing: block lexer reads line by line, inline scanner does a single linear pass with no backtracking.
- **Few dependencies**: `html5ever` and `markup5ever_rcdom` for the HTML importer,
  `pulldown-cmark` for the Markdown importer, `regex`, and `unicode-normalization`
  for heading-id NFC.
- **Conformance via corpus**: every supported construct has a `.crv` / `.html` pair in the upstream spec. The Rust output must match the JS reference byte-for-byte (after trimming).

See `src/parse.rs` for the parser and `src/render.rs` for the renderer. The AST in `src/ast.rs` mirrors the shape of [`carve-js`'s `ast.ts`](https://github.com/markup-carve/carve-js/blob/main/src/ast.ts).

---

[Back to the README](../README.md)
