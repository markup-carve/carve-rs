# carve-rs

[![CI](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Rust parser and HTML renderer for the [Carve](https://github.com/markup-carve/carve) markup language.

> Carve is a post-Markdown lightweight markup language with visual mnemonics and human-centered design. See the [language site](https://markup-carve.github.io/carve/) for the spec.

## Status

The crate passes every `.crv` / `.html` pair currently checked into its
`tests/spec` submodule. The conformance test suite includes an all-pairs gate so
new corpus pairs fail CI until the parser and renderer support them.

| Pair | Construct | Status |
|---|---|---|
| 01-emphasis | `/italic/`, `*bold*`, `_underline_`, `~strike~`, `^super^`, `,sub,`, `=hl=`, `/*bi*/` | passing |
| 02-headings | `# H1` … `#### H4` | passing |
| 03-links | `[text](url)` | passing |
| 04-images | `![alt](src)` (inline + block) | passing |
| 05-lists | unordered (`- item`) | passing |
| 06-task-lists | `- [ ] todo`, `- [x] done` | passing |
| 11-fenced-code | ` ``` ` blocks with language tag | passing |
| 12-inline-code | `` `code` `` | passing |
| 07-blockquote-with-attribution | `> quote` + `^ Attribution` caption | passing |
| 08-image-with-caption | `![…](…)` + `^ caption` | passing |
| 09–10 | tables, rowspan/colspan | passing |
| 13 | admonitions (`::: note`) | passing |
| 14 | abbreviations (`*[ABBR]:`) | passing |
| 15 | `@mentions`, `#tags` | passing |
| 16 | inline extensions (`:type[…]`) | passing |
| 17 | attribute blocks (`{#id .class}`) | passing |
| 18 | YAML frontmatter | passing |

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

Besides HTML, the crate renders the same AST to Markdown, plain text, and
ANSI-styled text via `carve::to_markdown`, `carve::to_plain_text`, and
`carve::to_ansi` (each with a matching `render_*` function for a parsed
`Document`).

## Extensions

Opt-in extensions implement `CarveExtension` and are passed through `Options`.
An extension can add inline/block matchers, run `after_parse` and
`before_render` AST transforms, and override renderers for extension nodes such
as `:kbd[Ctrl]`.

```rust
use carve::{CarveExtension, InlineExtension, Options, RenderContext};

struct Kbd;

impl CarveExtension for Kbd {
    fn name(&self) -> &'static str {
        "kbd"
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        (node.name == "kbd").then(|| {
            format!("<kbd>{}</kbd>", ctx.render_inlines(&node.children))
        })
    }
}

let kbd = Kbd;
let options = Options::new().with_extension(&kbd);
let html = carve::to_html_with_options("Press :kbd[Ctrl].", &options);
assert_eq!(html, "<p>Press <kbd>Ctrl</kbd>.</p>");
```

## CLI

The crate ships a `carve` binary that reads Carve source from a file or stdin and writes HTML to stdout:

```bash
cargo install --path .

carve README.crv > README.html
echo '# Hello' | carve
carve --mention-url '/users/{name}' --tag-url '/topics/{name}' social.crv
carve --emoji 'rocket=🚀' --emoji 'tada=🎉' emoji.crv
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
