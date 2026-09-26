# carve-rs

[![CI](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Rust parser and renderer for the
[Carve](https://github.com/markup-carve/carve) markup language. The crate
implements Carve spec 0.1 and renders HTML, Markdown, plain text, ANSI, and
canonical Carve source. The
[versioning contract](https://markup-carve.github.io/carve/versioning) says what
a release may change.

## Install

```bash
cargo add carve-lang
```

Install the CLI with `cargo install carve-lang`, or download a release archive.

The package name is `carve-lang`; Rust code imports it as `carve`, and the CLI
binary is named `carve`.

## Library use

```rust
let html = carve::to_html("# Hello\n\nThis is /italic/ and *bold*.");
```

Lower-level functions expose the typed AST:

```rust
let document = carve::parse(source);
let html = carve::render_html(&document)?;
```

Checked render functions report content omitted for the selected output and
can return `RenderLossError` before publishing a value. Source-aware tools can
prepare canonical formatting with `to_carve_patch` and apply it with
`apply_source_patch`.

See [Source-preserving patches](https://github.com/markup-carve/carve-rs/blob/main/docs/source-patches.md)
and the [full usage walkthrough](https://github.com/markup-carve/carve-rs/blob/main/docs/reference.md).

## CLI and migration

The `carve` CLI renders documents, formats source, exports the serialized AST,
and imports HTML, Markdown, Djot, and BBCode. Importers emit structured
fidelity reports and participate in `--check-loss`.

- [Command line](https://github.com/markup-carve/carve-rs/blob/main/docs/cli.md)
- [Migration](https://github.com/markup-carve/carve-rs/blob/main/docs/migration.md)
- [Migration reports](https://github.com/markup-carve/carve-rs/blob/main/docs/migration-result.md)

## Extensions and security

Extensions can add syntax, AST transforms, and renderers. Built-in profiles
restrict constructs and resource use for untrusted content. Raw HTML, custom
renderers, and symbol values have separate trust boundaries.

For untrusted CLI input, use `--safe` to escape raw HTML. See
[Extensions](https://github.com/markup-carve/carve-rs/blob/main/docs/extensions.md),
[Security](https://github.com/markup-carve/carve-rs/blob/main/docs/security.md),
and [Linting](https://github.com/markup-carve/carve-rs/blob/main/docs/linting.md).

## File inclusion

Includes are opt-in. Library hosts supply a resolver. For named CLI input, the
root defaults to the file's directory; stdin stays literal unless
`--include-root` is supplied. Expansion reports dependencies and warnings.

The [file inclusion reference](https://github.com/markup-carve/carve-rs/blob/main/docs/reference.md#file-inclusion) covers
renaming, heading shifts, nested directives, containment, and current limits.

## Documentation

The [`docs/`](https://github.com/markup-carve/carve-rs/tree/main/docs) directory contains topic guides for rendering, extensions,
the CLI, migrations, conformance, editor integration, streaming, accessibility,
security, patches, and parser snapshots.

## Development

Start with [CONTRIBUTING.md](https://github.com/markup-carve/carve-rs/blob/main/CONTRIBUTING.md)
for the toolchain, the test commands, and what a spec-affecting change involves.
Build setup, tests, and engine-pin maintenance are in the
[development guide](https://github.com/markup-carve/carve-rs/blob/main/docs/development.md)
and [engine pin guard](https://github.com/markup-carve/carve-rs/blob/main/docs/engine-pin-guard.md).

### Whitespace and annotation offsets

An escaped space produces a `non_breaking_space` node. Preserved line-block
columns use the same node. Text and verbatim values keep literal Unicode,
including U+E000. HTML renders generated spaces as `&nbsp;`; Markdown uses
U+00A0, and plain text and ANSI use ordinary spaces. Package and envelope
versions remain on the existing release line.

Annotation offsets count Unicode codepoints in the contract's fixed field
order. They do not depend on JSON property insertion order or source positions.
The shared annotation fixture covers image alt text, math, breaks and reversed
ranges.
