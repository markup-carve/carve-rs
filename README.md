# carve-rs

[![CI](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Rust parser and renderer (HTML, Markdown, plain text, ANSI) for the [Carve](https://github.com/markup-carve/carve) markup language.

> Carve is a post-Markdown lightweight markup language with visual mnemonics and human-centered design. See the [language site](https://markup-carve.github.io/carve/) for the spec.

Implements **Carve spec 0.1** (see [Versioning & Changelog](https://markup-carve.github.io/carve/versioning)).

## Install

```sh
cargo add carve-lang
```

The crate is published on crates.io as **`carve-lang`** (the name `carve` was
taken), but it is imported as `carve` and its CLI binary is `carve`:

```rust
let html = carve::to_html("# Hello /Carve/");
```

Other formats convert in: HTML, Markdown, Djot and BBCode, in the library
and as `carve migrate --from <format>`. Only HTML drops anything, and only
it takes a mode, an adapter and a loss report - see
[docs/migration.md](https://github.com/markup-carve/carve-rs/blob/main/docs/migration.md).

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


## Extensions

Nine semantic inline names are built in and need no registration: `abbr`,
`cite`, `dfn`, `kbd`, `samp`, `var`, `time`, `code`, and `mark`.
`:name[content]{attrs}` remains an ordinary inline-extension AST node and maps
to the same-named HTML element; unknown names retain the generic
`<span class="ext-name">` fallback. Plain and ANSI render only the content.

The same registry has compact span-attribute sugar: `[Ctrl]{kbd}` and
`[HTML]{abbr="HyperText Markup Language"}`. Attributes can combine, for example
`[CSS]{dfn abbr="Cascading Style Sheets"}`; non-semantic attributes remain on
one outer span.
`:cite[…]` is distinct from bibliographic `[@key]` citations, and `:abbr[…]`
does not declare an automatic abbreviation.

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

Locale-specific quote glyphs are also an opt-in extension:

```rust
use carve::{Options, SmartQuotes};

let quotes = SmartQuotes::new("de");
let options = Options::new().with_extension(&quotes);
let html = carve::to_html_with_options("\"Hallo\"", &options);
assert_eq!(html, "<p>„Hallo“</p>");
```

### Built-in extensions

The crate ships the same opt-in extensions as carve-js: `Autolink`,
`ExternalLinks`, `HeadingPermalinks`, `TableOfContents`, `Wikilinks`,
`TabNormalize`, `FencedRender` (with a Mermaid preset), `MathBlock`,
`Spoiler`, `Details` and `ListTable`, plus the Tier-3 set. Each one, with
its options and its rendered output, is in
[docs/extensions.md](https://github.com/markup-carve/carve-rs/blob/main/docs/extensions.md).

## Untrusted input

Rendering attacker-controlled Carve needs the safe path: `--safe` on the
CLI, or the checked render options in the library, which escape raw HTML
blocks instead of emitting them. Nesting depth and other renderer limits
are bounded by default. The full threat model, every knob and what each one
refuses is in [docs/security.md](https://github.com/markup-carve/carve-rs/blob/main/docs/security.md).

## CLI

A `carve` binary ships with the crate:

```sh
carve README.crv > README.html      # render (HTML by default)
carve --markdown README.crv         # or --plain, --ansi, --json
carve lint README.crv               # report problems, change nothing
carve fmt -w README.crv             # format canonically
carve migrate --from html page.html # convert into Carve
```

Install it with `cargo install carve-lang`, or take the archive for your
platform from the [releases page](https://github.com/markup-carve/carve-rs/releases).
Every subcommand and flag is in [docs/cli.md](https://github.com/markup-carve/carve-rs/blob/main/docs/cli.md).

## Documentation

| | |
| --- | --- |
| [Migrating into Carve](https://github.com/markup-carve/carve-rs/blob/main/docs/migration.md) | HTML, Markdown, Djot and BBCode importers |
| [Extensions](https://github.com/markup-carve/carve-rs/blob/main/docs/extensions.md) | every built-in extension and its output |
| [Command line](https://github.com/markup-carve/carve-rs/blob/main/docs/cli.md) | every subcommand and flag |
| [Untrusted input](https://github.com/markup-carve/carve-rs/blob/main/docs/security.md) | the threat model and the safe path |
| [Linting](https://github.com/markup-carve/carve-rs/blob/main/docs/linting.md) | the lint rules and how to run them |
| [Rendering behavior](https://github.com/markup-carve/carve-rs/blob/main/docs/rendering.md) | section wrappers and heading ids |
| [Accessibility lint](https://github.com/markup-carve/carve-rs/blob/main/docs/accessibility-lint.md) | the accessibility rules |
| [ProseMirror / Tiptap](https://github.com/markup-carve/carve-rs/blob/main/docs/prosemirror.md) | editor interchange |
| [Streaming render](https://github.com/markup-carve/carve-rs/blob/main/docs/streaming-render.md) | rendering without buffering |
| [Reversible patches](https://github.com/markup-carve/carve-rs/blob/main/docs/reversible-patches.md) | editing an AST in place |
| [Migration results](https://github.com/markup-carve/carve-rs/blob/main/docs/migration-result.md) | the loss report shape |
| [Parser snapshots](https://github.com/markup-carve/carve-rs/blob/main/docs/parser-snapshots.md) | the snapshot suite |
| [Engine pin guard](https://github.com/markup-carve/carve-rs/blob/main/docs/engine-pin-guard.md) | for repos pinning this engine |
| [Conformance](https://github.com/markup-carve/carve-rs/blob/main/docs/conformance.md) | what the spec corpus covers |
| [Development](https://github.com/markup-carve/carve-rs/blob/main/docs/development.md) | building, design, and pinning this engine |

The language itself is specified at the
[Carve site](https://markup-carve.github.io/carve/).
