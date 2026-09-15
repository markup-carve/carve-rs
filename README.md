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
and as `carve migrate --from <format>`. Every importer can emit a report and
participate in `--check-loss`; HTML additionally takes a mode and adapter. The
other importers fail closed until they expose construct-level fidelity - see
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

Source-aware tools can prepare canonical formatting with `to_carve_patch` and
apply it with `apply_source_patch`; see
[source-preserving patches](https://github.com/markup-carve/carve-rs/blob/main/docs/source-patches.md).


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
| [Source-preserving patches](https://github.com/markup-carve/carve-rs/blob/main/docs/source-patches.md) | stale-safe UTF-8 edits |
| [Migration results](https://github.com/markup-carve/carve-rs/blob/main/docs/migration-result.md) | the loss report shape |
| [Parser snapshots](https://github.com/markup-carve/carve-rs/blob/main/docs/parser-snapshots.md) | the snapshot suite |
| [Engine pin guard](https://github.com/markup-carve/carve-rs/blob/main/docs/engine-pin-guard.md) | for repos pinning this engine |
| [Conformance](https://github.com/markup-carve/carve-rs/blob/main/docs/conformance.md) | what the spec corpus covers |
| [Development](https://github.com/markup-carve/carve-rs/blob/main/docs/development.md) | building, design, and pinning this engine |

## File inclusion

`{{ path }}` include directives (spec §19) are a **processor-level, opt-in**
feature. The parser never touches the filesystem and does not know the directive
exists, so with no resolver configured `{{ … }}` stays literal text.

The CLI enables inclusion for file inputs, with the containment root defaulting
to the **directory of the input document**, never the process working
directory. Stdin has no path context and therefore no inferable root, so
directives stay literal unless `--include-root` names one:

```bash
carve book/main.crv                       # root defaults to book/
carve --include-root . book/main.crv      # widen the root to the project
carve --include-root ./book < main.crv    # required to enable includes on stdin
```

`FileSystemResolver::new` refuses a root spec that is not **absolute** (spec
§19): a relative one has no base the specification names, and every
canonicalizer resolves it against the process working directory, which is
exactly what §19 forbids the root defaulting to. The relative spellings above
keep working because the CLI expands them in its own argument parsing before
constructing the resolver - an embedder that wants the same convenience does the
same.

```
{{ chapters/intro.crv }}              include a whole file
{{ chapters/intro.crv #setup }}       include one heading subtree
{{ notes.crv @lines:10-25 }}          include a physical line range
{{ chapters/intro.crv @shift:2 }}     shift included heading levels by +2
{{ chapters/intro.crv @shift:auto }}  shift to sit under the heading in scope
```

`#section` and `@lines` are the two selection mechanisms and are mutually
exclusive. Every failure (a missing file, binary content, both selectors, a
cycle, the depth limit, the size budget) emits a warning on stderr and leaves
the directive **literal**; inclusion never silently drops a directive. A
rejected directive has **no observable side effects**: the output is
byte-identical to the same document with that directive written as literal text
from the start, so a rejected include can never renumber an id or footnote
label that a later, successful include claims.

`carve fmt` **preserves** a well-formed directive verbatim rather than escaping
its braces, so formatting a document never breaks its includes. A run that is
not a well-formed directive (`{{ oops`, or an unknown `@option`) is ordinary
text and is escaped as such. To keep a literal `{{ … }}` out of the include
processor's way, put it in a code span or fence, where directives are inert.

Paths are checked against the root by **canonicalizing and then testing
containment**, not by banning `..` lexically. A `../shared/glossary.crv` whose
target stays inside the root resolves; symlinks out of the root, symlinked
directory components, and absolute paths outside the root are denied.

From the library, expansion is a pass between parse and render:

```rust
use carve::{expand_includes, parse, render_html, FileSystemResolver, IncludeOptions};

let source = std::fs::read_to_string("book/main.crv")?;
let resolver = FileSystemResolver::new("book")?;
let options = IncludeOptions::new()
    .with_resolver(&resolver)
    .with_source_path("book/main.crv");

let result = expand_includes(parse(&source), &source, &options);
for warning in &result.warnings {
    eprintln!("{}: {}", warning.rule, warning.message);
}
// Every target touched, resolved or not: hosts key file watchers off this.
for dependency in &result.dependencies {
    println!("{} (resolved: {})", dependency.id, dependency.resolved);
}
let html = render_html(&result.doc);
```

Implement `IncludeResolver` for a virtual filesystem, an in-memory map, or any
other source; `FileSystemResolver` is the ready-made one for trusted hosts.
`prepare_doc_with_includes` runs expansion and then the same extension-hook and
profile pipeline the `to_*` entry points use, so included content is subject to
exactly the same sanitization as content the author typed directly.

Source-position remapping for included spans (spec I4) is not implemented in any
engine yet; warnings carry the identity of the file they arose in instead.

### One self-contained file

`carve flatten` writes the document back as Carve with every include expanded in
place - the deliberate opposite of `carve fmt`, which leaves directives alone so
formatting returns the author's document. Flattening is for handing the document
to something that has no filesystem behind it: a web editor, a paste box, a
colleague.

```bash
carve flatten book/main.crv > one-file.crv
carve flatten --include-root ./book < main.crv
```

Two things it changes beyond inlining, both reported on stderr: the output is
CANONICAL Carve, so formatting is normalized rather than preserved, and
colliding heading ids and footnote labels are renamed (two files that were never
in one document together can each define `intro`). The renames are written into
the source, so the flattened file renders exactly like the expanded original.

### Turning the filesystem off entirely

`FileSystemResolver` is behind the default-on `fs` feature. Build with
`--no-default-features` and the crate carries no code that opens a file at all,
which is what a sandboxed, WASM or browser-hosted embedder wants: the guarantee
is then structural rather than a matter of not configuring a resolver. The CLI's
include path compiles out with it, so `--include-root` is refused rather than
silently ignored. Such a build can still expand includes from a resolver the
host writes itself (over an in-memory map, a virtual filesystem, `fetch`), since
a resolver is just a function.

`FileSystemResolver` reads at most 4 MiB per target (`DEFAULT_MAX_FILE_BYTES`),
adjustable with `with_max_file_bytes`. The expansion byte budget cannot stand in
for this: it charges a target only once its source is in hand, so without a cap
one oversized file is read into memory in full before expansion refuses it.

## Development

Build, contributor, engine-pin, and design notes are in the [development guide](docs/development.md).
