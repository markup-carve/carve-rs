# carve-rs

[![CI](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Rust parser and HTML renderer for the [Carve](https://github.com/markup-carve/carve) markup language.

> Carve is a post-Djot lightweight markup language with visual mnemonics and human-centered design. See the [language site](https://markup-carve.github.io/carve/) for the spec.

## Status

MVP. The crate currently passes 8 of the 18 pairs in the [spec corpus](https://github.com/markup-carve/carve/tree/master/tests/corpus):

| Pair | Construct | Status |
|---|---|---|
| 01-emphasis | `/italic/`, `*bold*`, `_underline_`, `~strike~`, `^super^`, `,,sub,,`, `==hl==`, `/*bi*/` | passing |
| 02-headings | `# H1` … `#### H4` | passing |
| 03-links | `[text](url)` | passing |
| 04-images | `![alt](src)` (inline + block) | passing |
| 05-lists | unordered (`- item`) | passing |
| 06-task-lists | `- [ ] todo`, `- [x] done` | passing |
| 11-fenced-code | ` ``` ` blocks with language tag | passing |
| 12-inline-code | `` `code` `` | passing |
| 07-blockquote-with-attribution | `> quote` + `^ Attribution` caption | deferred |
| 08-image-with-caption | `![…](…)` + `^ caption` | deferred |
| 09–10 | tables, rowspan/colspan | deferred |
| 13 | admonitions (`::: note`) | deferred |
| 14 | abbreviations (`*[ABBR]:`) | deferred |
| 15 | `@mentions`, `#tags` | deferred |
| 16 | inline extensions (`:type[…]`) | deferred |
| 17 | attribute blocks (`{#id .class}`) | deferred |
| 18 | YAML frontmatter | deferred |

The deferred set is wired into the test suite as `#[ignore]`d tests so progress stays visible — promote a slug into `IMPLEMENTED` in `tests/corpus.rs` when the parser supports it.

## Library use

```rust
let html = carve::to_html("# Hello\n\nThis is /italic/ and *bold*.");
assert!(html.contains("<h1>Hello</h1>"));
```

For lower-level access, `carve::parse` returns a typed `Document` AST and `carve::render_html` walks it to HTML:

```rust
let doc = carve::parse(source);
// inspect or transform doc.children …
let html = carve::render_html(&doc);
```

## CLI

The crate ships a `carve` binary that reads Carve source from a file or stdin and writes HTML to stdout:

```bash
cargo install --path .

carve README.crv > README.html
echo '# Hello' | carve
carve --help
```

## Building from source

```bash
git clone https://github.com/markup-carve/carve-rs
cd carve-rs
git submodule update --init   # pulls the spec corpus
cargo test
```

The spec corpus lives in `tests/spec/` as a git submodule of [`markup-carve/carve`](https://github.com/markup-carve/carve). Running `cargo test` without initializing the submodule will fail with a clear error message.

## Design

- **Linear-time** parsing: block lexer reads line by line, inline scanner does a single linear pass with no backtracking.
- **Zero dependencies** in the runtime crate. Tests use only `std`.
- **Conformance via corpus**: every supported construct has a `.crv` / `.html` pair in the upstream spec. The Rust output must match the JS reference byte-for-byte (after trimming).

See `src/parse.rs` for the parser and `src/render.rs` for the renderer. The AST in `src/ast.rs` mirrors the shape of [`carve-js`'s `ast.ts`](https://github.com/markup-carve/carve-js/blob/main/src/ast.ts).

## License

MIT — see [LICENSE](LICENSE).
