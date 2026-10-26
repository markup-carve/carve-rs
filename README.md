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

## Documentation

- [Carve documentation](https://markup-carve.github.io/carve/)
- [Playground](https://markup-carve.github.io/carve/playground)
- [Engine guides](docs/README.md)
- [Security for untrusted input](docs/security.md)
- [Development and contributing](docs/development.md)

## License

[MIT](LICENSE).
