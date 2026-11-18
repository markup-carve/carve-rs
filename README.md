# carve-rs

[![CI](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

carve-rs parses and renders Carve in Rust. It implements Carve spec 0.1.
Read the [Carve documentation](https://markup-carve.github.io/carve/) for the language.

## Install

```sh
cargo add carve-lang
```

## Usage

```rust
let document = carve::parse("# Hello /Carve/");
let html = carve::render_html(&document)?;
```

CLI basics:

```sh
carve README.crv > README.html
carve lint README.crv
carve fmt README.crv
carve migrate --from html page.html
```

## Carve 0.2 library

The Carve 0.2 engine is the crate `carve2` in [`v2/`](v2) on the `next2` branch. Its public API is the module `carve2::api`: the calls of the 0.2 tooling specification, typed errors, a writer registry, and a JSON boundary that bindings and the `wasm32-wasip1` command `carve2 call` share. See [the 0.2 API guide](v2/README.md), which also maps every 0.1 call the bindings use to its 0.2 counterpart.

## Documentation

- [Carve documentation](https://markup-carve.github.io/carve/)
- [Playground](https://markup-carve.github.io/carve/playground)
- [Engine guides](docs/README.md)
- [Security for untrusted input](docs/security.md)
- [Development and contributing](docs/development.md)
