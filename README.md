# carve-rs

The Rust engine of the [Carve](https://markup-carve.github.io/carve/) markup language, version 0.2: parse, resolve, render HTML, format, lint and import. The crate is published as `carve-lang` and imported as `carve`; it also builds the `carve` command.

Its public library is the module `carve::api`. The rest of the crate is engine internals: those modules are hidden from rustdoc and may change in any commit.

The calls follow `spec/0.2/tooling.md` (C2-T-005), spelled the Rust way (C2-T-001): `parseWithDiagnostics` becomes `parse_with_diagnostics`, `strictLosses` becomes `strict_losses`. Run `cargo doc --open` for the full reference.

## Install

```toml
[dependencies]
carve = { package = "carve-lang", version = "0.2" }
```

The `fs` feature (on by default) holds every code path that opens a file: the include resolver `carve::include::fs`, the `includes.root` option of the JSON boundary and the `carve` command. With `default-features = false` the library opens no file, for sandboxed, WASM and browser hosts; a host then passes its own `Resolver`.

```toml
carve = { package = "carve-lang", version = "0.2", default-features = false }
```

## Calls

| `tooling.md` | Rust | Status |
|---|---|---|
| `parse` | `api::parse(input, &Options) -> Result<Tree, Error>` | available |
| `parseWithDiagnostics` | `api::parse_with_diagnostics` returns `Parsed { tree, diagnostics, suppressed_diagnostics }` | available |
| `parseWithLayout` | `api::parse_with_layout` | `Error::Unsupported` for now |
| `render` | `api::render(input, &Options)`, `api::render_tree(&Tree, &Options)` | available |
| `renderWithDiagnostics` | `api::render_with_diagnostics`, `api::render_tree_with_diagnostics` return `Rendered { html, diagnostics, suppressed_diagnostics }` | available |
| `format`, `formatWithDiagnostics` | `Carve::format`, `Carve::format_with_diagnostics` | available |
| `write`, `writeWithDiagnostics` | `Carve::write`, `Carve::write_with_diagnostics`, `Carve::write_to(target, ...)` | `html`, `carve`, `markdown`, `plain` and `ansi` built in |
| `lint` | `api::lint(input, &LintOptions) -> Result<Vec<Entry>, Error>` | available |
| `importDocument` | `api::import_document(html, &ImportOptions)`, `Carve::import_document` return `Imported { tree, source, diagnostics, suppressed_diagnostics }`; `Carve::import_document_with(html, &ImportOptions, &LossOptions { allow_losses, max_losses })` adds `allowLosses` and `maxLosses` | available (HTML) |
| `includeDependencies` | `api::include_dependencies(&Tree)` | available |
| `encodeTree`, `encodeEnvelope` | `api::encode_tree`, `api::encode_tree_with(&Tree, EncodeOptions { positions })`, `api::encode_envelope` | available |
| `decodeTree`, `validateTree` | `api::decode_tree(&str)`, `api::validate_tree(&Tree)` | available |
| `migrateTree` | `api::migrate_tree(&Value) -> Result<Tree, Error>`, `api::migrate_tree_with(&Value, &MigrateOptions { source })` returns `Migrated { tree, envelope, diagnostics }` (C2-A-178 to C2-A-180) | available, without the layout record |
| `positionOf` | `api::position_of(input, offset) -> Result<Position { line, column }, Error>` (C2-T-041) | available |
| `diffTree`, `applyPatch` | `api::diff_tree(&Tree, &Tree) -> Vec<Value>`, `api::apply_patch(&Tree, &[Value], Option<&str>) -> Result<Tree, Error>` | available |
| `sourceFingerprint`, `treeFingerprint` | `api::source_fingerprint(&[u8])`, `api::tree_fingerprint(&Tree)`: `sha256:` and the hex SHA-256 of the source or of `encode_tree` | available |
| `headingId` | `api::heading_id(text, HeadingIds) -> Option<String>`: the generated id without the duplicate suffix, `None` under `off` | available |
| `VERSION`, `SPEC_VERSION`, `NODE_TYPES` | `api::VERSION`, `api::SPEC_VERSION { version, commit }`, `api::NODE_TYPES { block, inline }` from the pinned `ast-schema.json` | available |

`input` is anything that converts into `api::Input`: `&str`, `&String`, `&[u8]` or `&Vec<u8>`. Bytes are never refused for their encoding (C2-B-218).

```rust
use carve::api::{self, Options};

let opts = Options::default();
let html = api::render("Hello *world*", &opts)?;
let tree = api::parse("# Title", &opts)?;
let json = api::encode_tree(&tree);
let again = api::decode_tree(&json)?;
assert_eq!(api::render_tree(&again, &opts)?, api::render("# Title", &opts)?);
# Ok::<(), carve::api::Error>(())
```

## Options

`api::Options` holds the parse, resolution and render options of `data/options.json` (`heading_ids`, `mode`, `profile`, `raw_html`, `url_policy`, `includes` and the rest). `api::LintOptions` holds `rules`, `platforms` and `includes`, and `api::WriteOptions` holds `align_tables`, `normalize` and the render options a writer reads.

Typed structs cannot carry an unknown option, so the refusals of C2-H-137 happen where options arrive as data: the JSON boundary refuses an unknown option, a value outside the option's range, and an option that no stage of the call reads (`alignTables` on `render`, for example). `api::lint` checks rule ids against `data/lint-rules.json` and platforms against `data/options.json` in both forms.

Includes use the resolver of `includes.md`:

```rust,no_run
use carve::api::{self, Includes, Options};
use carve::include::fs::resolver;

let root = std::path::Path::new("docs");
let opts = Options {
    includes: Some(Includes::new(resolver(root).expect("readable root"))),
    ..Options::default()
};
let tree = api::parse("{{ intro.crv }}", &opts)?;
for dependency in api::include_dependencies(&tree) {
    println!("{} read={}", dependency.path, dependency.file.is_some());
}
# Ok::<(), carve::api::Error>(())
```

A host with its own storage passes any `Resolver`, a function from an `api::Request` to the file's bytes and id, or an `api::Cause`.

## Errors

Every call returns `api::Error`. Its variants are the library errors of C2-T-040, and `spec_name()`, `category()` and `status()` give the matching row. `path()` gives the RFC 6901 pointer of what an option or tree error names (`/profile`, `/children/0/type`), which the JSON boundary writes as `path` (C2-A-167):

| Variant | `spec_name()` | `category()` | `status()` |
|---|---|---|---|
| `Option` | `CarveOptionError` | `usage` | 2 |
| `Input` | `CarveInputError` | `input-invalid` | 2 |
| `Render` | `CarveRenderError` | `render-loss` | 1 |
| `Tree` | `CarveTreeError` | `ast-json-invalid` | 2 |
| `Write` | `CarveWriteError` | `writer-refused` | 1 |
| `ImportInput` | `ImportInputError` | `import-input-invalid` | 2 |
| `Patch` | `CarvePatchError` | `ast-json-invalid` | 2 |
| `Export` | `CarveExportError` | `render-loss` | 1 |
| `Unsupported` | `CarveOptionError` | `usage` | 2 |

`Patch { path, reason }` refuses an `apply_patch`: `path` points into the patch (`/0/op`), or into the tree the patch gave. Its row is not in C2-T-040 yet; on the JSON boundary the error object also carries `path` and `reason`.

`Unsupported` is not a row of C2-T-040. It marks a 0.2 call that this engine does not offer yet, so a host can test for it instead of matching a message.

## Diagnostics

A record entry is `api::Entry` with the library fields of C2-T-020: `code`, `severity`, `channel`, `message`, and, where they apply, `path`, `range` (`start_offset` and `end_offset` in UTF-8 bytes), `file` and `foreign_type`. It serializes with the spec's camelCase names. Include entries past `include_diagnostics` (100) are held back and counted in `suppressed_diagnostics` (C2-T-024).

## Writers

`api::Writer` is one interface for every output format that writes a resolved tree: a target name and `write(&Tree, &WriteOptions) -> Result<Written, Error>`. `Carve::new()` has the built-in writers `html`, `carve` and the exports `markdown`, `plain` and `ansi`. Register more with `Carve::register_writer`; a second writer for the same target, or a writer that takes one of the three export names, is refused. `Carve::write_to` refuses an unknown target with an `Error::Option` whose message lists the known targets.

## Exports: Markdown, plain text and ANSI

`carve::export` writes a 0.2 tree as Markdown (CommonMark with the GFM tables, task lists, strikethrough and footnotes), as plain text, or as text styled with ANSI escape sequences for a terminal. It ports carve-js `@markup-carve/carve/export` (`docs/export.md` there lists the Markdown fallbacks and the loss codes). Each format has one call that returns the text and the losses of the run in one `Written` result:

```rust
use carve::api::{self, Options};
use carve::export::{self, ExportOptions};

let tree = api::parse("= Notes\n\nSee *this* and </#Notes>.\n", &Options::default())?;
let md = export::to_markdown(&tree, &ExportOptions::default())?;
assert_eq!(md.output, "# Notes\n\nSee **this** and [Notes](#notes).\n");
assert_eq!(export::to_plain_text(&tree, &ExportOptions::default())?.output, "Notes\n\nSee this and Notes.\n");
let ansi = export::to_ansi(&tree, &ExportOptions::default())?;
assert!(ansi.output.contains("\x1b[1mthis\x1b[0m"));
# Ok::<(), carve::api::Error>(())
```

`ExportOptions` holds `smart_typography` (default true), `strict_losses` (refuses with `Error::Export`, which carries the losses) and `url_policy` (a denied destination is written empty and reported). The losses are entries of the `render-loss` channel with the pointer of their node. `export::export_to(Target, ...)` writes by target, and the writers of the registry (`markdown`, `plain`, `ansi`) read the same three options from `WriteOptions::render`. On the command line, `carve render --to markdown|plain|ansi|carve FILE` writes the other formats.

The `codeBlockRenderer` render option is `Options::code_block_renderer`: a `CodeBlockRenderer` wrapping a host function from a code block node (its JSON value) and the block's default HTML to the HTML written in its place, or `None` for the default (C2-H-206, C2-H-207). It runs for every ordinary code block, in containers and expanded includes too, but not for a raw block, a code span or a block outside the profile.

## Extensions (provisional)

`api::extension` is the extension host of `extensions.md`. A host registers an `Extension`: its `ExtensionDeclaration`, renderers per `Channel` and an optional `DocumentTransform`. The types may change before 0.2 is final.

```rust
use carve::api::{self, Carve, Channel, Error, Extension, ExtensionDeclaration, Options, RenderContext, Renderer};
use serde_json::Value;

struct Note;

impl Renderer for Note {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        Ok(format!("<aside>{}</aside>", cx.render_blocks(&node["slots"]["body"])?))
    }
}

let mut carve = Carve::new();
carve.register_extension(Extension {
    declaration: ExtensionDeclaration {
        name: "ex.note".into(),
        version: "1".into(),
        required: false,
        slots: vec!["body".into()],
    },
    renderers: vec![(Channel::Interactive, Box::new(Note))],
    transform: None,
})?;
let tree = api::decode_tree(r#"{"type":"document","astVersion":"2.0",
  "extensions":[{"name":"ex.note","version":"1","required":false,"slots":["body"]}],
  "children":[{"type":"block_extension","name":"ex.note","version":"1","children":[],
    "slots":{"body":[{"type":"paragraph","children":[{"type":"text","value":"Hi"}]}]},
    "fallback":[{"type":"paragraph","children":[{"type":"text","value":"Hi"}]}]}]}"#)?;
assert_eq!(carve.render_tree(&tree, &Options::default())?, "<aside><p>Hi</p></aside>");
# Ok::<(), carve::api::Error>(())
```

The render calls of a host with extensions (`Carve::render`, `render_with_diagnostics`, `render_tree`, `render_tree_with_diagnostics`) work in this order:

1. The document transforms run in registration order, each on a copy of the resolved view, and each sees the result of the one before (C2-X-053). A transform whose view gives an authored id to a second node fails (C2-X-055).
2. Each qualified extension node is rendered by the renderer of the channel that `Options::mode` selects: `Channel::Interactive` for `interactive`, `Channel::BuildTime` for `static` (C2-X-052). With no renderer in that channel the fallback is written and `extension-fallback-rendered` reported (C2-X-035).
3. A failing transform keeps its input view and reports `extension-transform-failed`; a failing renderer writes the fallback and reports `extension-renderer-failed`. Both entries name the extension (C2-X-050).

A renderer receives the node as its JSON value and a `RenderContext` with the options, the mode, `escape`, `escape_attribute`, `render_blocks`, `render_inlines`, `plain_text`, `label` (the `labels` option), `next_id` (a control id no authored or earlier id takes, C2-X-048) and `attributes` (the attribute list of an element, written by the core rules, C2-X-033). With no extension registered the host methods are the free functions, so the output does not change.

Over JSON, `host.documentTransforms` registers transforms that need no host code: each `{"name", "after", "insert"}` inserts the node `insert` after the node at the JSON pointer `after`. This is the form of the conformance host:

```json
{"call": "render", "tree": {...}, "host": {"documentTransforms": [
  {"name": "sample.insert", "after": "/children/0", "insert": {"type": "block_extension", "name": "sample.insert", "version": "1", "children": [], "fallback": [...]}}
]}}
```

`Carve::enable_standard("carve.tabs", renderer)` enables the standard `carve.tabs` extension (C2-X-039, provisional): its matcher claims `tabs` divs, and with `renderer` its standard renderers render them. Over JSON, the option `extensions` names it and `renderers` maps it to `"standard"`:

```json
{"call": "render", "source": "...", "options": {"extensions": ["carve.tabs"], "renderers": {"carve.tabs": "standard"}}}
```

The same call enables the other standard extensions, all provisional:

- `carve.wiki` and `carve.cite` claim `:wiki[...]` and `:cite[...]` (C2-X-057, C2-X-058). They have no standard renderer, so the content renders as plain text and attributes such as `locator` stay on the node.
- `carve.imgFence.img` and `carve.imgFence.image` claim `img` and `image` code fences whose body is SVG (C2-X-059). Their standard renderer writes one `<img>` with a sanitized `data:image/svg+xml` URL; SVG it cannot sanitize keeps the code block and reports `extension-renderer-failed`.
- `carve.fencedRender.<word>` for `mermaid`, `d2`, `dot`, `graphviz`, `wavedrom`, `abc`, `plantuml`, `puml`, `vega-lite` and `chart`: the diagram fence of that word as `<pre class="mermaid" role="img" aria-label="mermaid">` with the escaped source (`vega-lite` and `chart` as a JSON `<script>` in a `<div>`), for a client library to draw.
- `carve.mathBlock`: a `math` fence as `<div class="math display">\[...\]</div>`.
- `carve.codeGroup`: a `code-group` div whose body is code blocks as one switcher (radio inputs; one headed section per panel in `static` mode).
- `carve.codeCallouts`: a document transform that turns a trailing `<n>` on a code line into a callout bubble and a following `<n> text` paragraph into its list.
- `carve.details`: a `details` div as `<details>` with its title as `<summary>` (`Details` without one), expanded in `static` mode.
- `carve.spoiler`: `:spoiler[x]` as `<span class="spoiler">` and a `spoiler` div as `<details class="spoiler">`; `static` mode reveals both.
- `carve.wiki` with its standard renderer: `:wiki[Page#anchor|Display]` as a link through a slug of the page, which the URL policy must allow (C2-X-057). `wikilinks` takes a URL generator, the link classes and `new_window`.
- `carve.tabNormalize` (tabs in code as two spaces; `tab_normalize(width)` for another width) and `carve.inlineFootnotesToParentheses`: document transforms of the render view. `default_attributes` adds attributes per node type under the authored ones and has no JSON form.

These come from `api::extensions`, the matcher surface of C2-X-005 to C2-X-031 with the ported carve-js standard extensions. A factory returns a `Bundle` of extensions and matchers that `Carve::register` adds; the factories take the options the JSON form cannot carry, such as a build-time renderer for `static` mode (`build`), a highlighter or the `aria` mode of `code_group`. Without a build-time renderer, `static` mode writes the fallback code block (C2-X-052). Two matchers of one source name fail at registration (C2-X-014).

```rust
use carve::api::{Carve, Options, extensions};

let mut carve = Carve::new();
carve.register(extensions::Bundle::merge([
    extensions::presets(extensions::PresetBuilders::default()),
    extensions::math_block(extensions::MathBlockOptions::default()),
]))?;
let html = carve.render("```math\nx^2\n```", &Options::default())?;
assert_eq!(html, r#"<div class="math display">\[x^2\]</div>"#);
# Ok::<(), carve::api::Error>(())
```

The render options `soft_break` (`softBreak`: `newline`, `space` or `br`), `allow_losses` (`allowLosses`, render-loss codes left out of the record and of `strict_losses`) and `max_losses` (`maxLosses`, default 100; later losses are counted in `suppressed_diagnostics`) follow C2-H-203 to C2-H-205.

## Bindings and the JSON boundary

`api::json::call(&str) -> String` takes one JSON request and returns one JSON response. Bindings (carve-py with PyO3, carve-rb with magnus, carve-wasm with wasm-bindgen) can wrap this single function instead of mirroring every Rust type, and hosts that run the WASI build use it through `carve call`.

A request names the call with its `tooling.md` spelling:

```json
{"call": "renderWithDiagnostics", "source": "Hi *there*", "options": {"profile": "comment"}}
```

Fields: `source` (text) or `sourceBytes` (a list of byte values); `tree` (a tree as a JSON value) for the calls that read a tree and for `render` instead of a source; `json` (text) for `decodeTree`; `options`, keyed as in `data/options.json`; `positions` for `parse`; `target` for `write` (`carve`, `html`, `markdown`, `plain` or `ansi`; the three exports also read `smartTypography`, `strictLosses` and `urlPolicy`); `file` for `includeDependencies`, the id of the top-level document, which a tree in JSON does not carry (`Tree::set_file` in Rust). `from` for `importDocument` (`"html"`, with the HTML as `source`). `a` and `b` for `diffTree`; `tree`, `patch` and `fingerprint` for `applyPatch`; `text` for `headingId`; `offset` for `positionOf`. In JSON, `includes` is `{"root": DIR}` with optional `file`, `depth` and `bytes`; `extensions` and `renderers` accept the standard extensions only, since host code has no JSON form (C2-T-010). For the same reason `codeBlockRenderer` takes the harness function of the conformance cases, `{"syntax": "ts", "result": "<div>{default}</div>"}`: `result` replaces each block of that `syntax` (of any syntax without it), with `{default}` and `{syntax}` filled in, and `null` keeps the default HTML. `format` and `importDocument` answer with `source`, `write` with `output`.

The response is `{"ok": RESULT}` or an error with the row of C2-T-040:

```json
{"error": {"name": "CarveOptionError", "category": "usage", "status": 2, "message": "option nope: unknown option"}}
```

## WASI command

`carve` builds for `wasm32-wasip1`:

```sh
cargo build --release --target wasm32-wasip1 --bin carve
echo '{"call":"render","source":"Hi"}' | wasmtime target/wasm32-wasip1/release/carve.wasm call
```

`carve call` reads one request on standard input, writes the response and a line feed on standard output, and exits with the status of the error, or 0. carve-go and hugo-carve can run it under any WASI runtime. CI runs the build under Node's WASI runtime (`tools/wasi-smoke.mjs`).

## Migrating from 0.1

The 0.1 engine is gone from this crate; the 0.2 `carve` command replaces the 0.1 one. The table maps the 0.1 calls that the bindings (carve-py, carve-rb, carve-wasm) use today. `pending` means the 0.2 call exists and returns `Error::Unsupported` until the engine implements it. `gone` means 0.2 has no counterpart.

| 0.1 call | 0.2 call |
|---|---|
| `parse`, `parse_with_options` | `api::parse` |
| `prepare_document_for_render` | `api::parse`: the tree it returns is resolved |
| `to_html`, `to_html_with_options`, `try_to_html_with_options` | `api::render` |
| `render_html`, `render_html_with_options` | `api::render_tree` |
| `to_html_with_report`, `with_render_loss_report` | `api::render_with_diagnostics` |
| `apply_profile_with_typography` | `Options::profile` and `Options::smart_typography` on `api::render` |
| `to_json`, `try_to_json_with_options` | `api::parse` then `api::encode_tree` |
| `from_json` | `api::decode_tree` (2.0 trees only) |
| `expand_includes` | `Options::includes` on `api::parse` and `api::render` |
| `select_include_region` | include options on the directive (`includes.md`), expanded by `api::parse` |
| `lint_carve`, `lint_carve_with_options` | `api::lint` |
| `lint_accessibility` | `api::lint`: `a11y/image-alt` and `heading-level-skip` are catalog rules |
| `extensions::registry::by_key`, `extensions::registry::keys` | `Carve::register_extension` (provisional) |
| `to_carve`, `render_carve`, `try_to_carve_with_options`, `to_carve_with_report` | `Carve::format`, `Carve::write` |
| `to_markdown`, `render_markdown`, `render_markdown_with_options`, `try_to_markdown_with_options`, `to_markdown_with_report` | `export::to_markdown`, writer target `markdown` |
| `to_plain_text`, `render_plain_text`, `render_plain_text_with_options`, `try_to_plain_text_with_options`, `to_plain_text_with_report` | `export::to_plain_text`, writer target `plain` |
| `to_ansi`, `render_ansi`, `render_ansi_with_options`, `try_to_ansi_with_options`, `to_ansi_with_report` | `export::to_ansi`, writer target `ansi` |
| `html_to_ast`, `migrate_html` | `api::import_document` |
| `parse_with_source_layout` | `api::parse_with_layout` (pending) |
| `djot_to_carve`, `migrate_djot`, `bbcode_to_carve`, `migrate_bbcode`, `try_migrate_markdown`, `try_markdown_to_ast` | gone: 0.2 specifies no Markdown, Djot or BBCode importer (C2-T-003) |
| `create_ast_patch`, `apply_ast_patch`, `ast_patch_to_json`, `ast_patch_from_json`, `create_reversible_ast_patch`, `apply_reversible_ast_patch` | gone: 0.2 has no patch API (`UPGRADING.md`, Part 2) |
| `merge_ast`, `merge_ast_with_resolver` | gone: no merge API in 0.2 |
| `parse_snapshot`, `reparse` | gone: no incremental or editor API in 0.2 |
| `to_prosemirror`, `from_prosemirror` | gone: not part of 0.2 |
| `decision_trace`, `parse_locator` | gone: 0.1 debugging aids |
| `stamp_carve`, `read_stamp`, `needs_review` | gone: version stamps belong to the 0.1 upgrade flow; the rewrite tool is part of carve-js |
| `sanitize_svg` | gone from the core; an extension renderer's job under the extension host |

Option types: `carve::Options::new()` and `Options::default()` become `api::Options::default()`. `Profile`, `Mode` and the heading-id setting (`AsciiHeadingIds` in 0.1, `HeadingIds` in 0.2) keep their roles. `IncludeOptions`, `IncludeContext` and `IncludeResolved` become `api::Includes` with an `api::Resolver`.

## Spec gaps and engine choices

- `tooling.md` gives `parse` a `positions` parameter. Here the tree in memory always carries positions, and `encode_tree_with(..., EncodeOptions { positions: false })` and the JSON request field `positions` drop them on output.
- `tooling.md` gives `includeDependencies` a tree. `api::include_dependencies` reads the include nodes of an expanded tree; the `from` of a top-level include is the `file` id given in `Includes`, or set with `Tree::set_file` on a decoded tree.
- On bytes with malformed UTF-8, `parse_with_diagnostics`, `render_with_diagnostics` and `lint` read the replaced text and move each range back to input byte offsets; a range edge inside a replaced sequence moves to the end of that sequence.
- The engine does not yet implement `parseWithLayout`; it returns `Error::Unsupported`. `migrateTree` returns no source-layout record, and the JSON call answers `tree`, `diagnostics` and, for an envelope, `envelope`.
- `migrateTree` reads a 1.x `keyValues` object without `order` in sorted member order: the JSON reader of this crate does not keep member order.
- `tree_fingerprint` hashes this engine's `encode_tree`. Member order is not fixed across engines (carve#6099), so a fingerprint from another engine does not match the same tree here.
- Extension host: resolution still visits only the fallback of an extension node, where C2-X-032 asks for its slots when a renderer is registered, so `render_blocks` and `render_inlines` render a slot as a document of its own, without the references, notes and ids of the whole document. The claim ownership of C2-X-054 is not tracked.
- Include resolver on WASI preview 1 and Windows: Rust has no stable identity of an open file there, so the resolver checks after the open that the target still canonicalizes to itself inside the root and has the type, length and modification time of the opened file.
- The social option's host functions (`resolveMention`, `resolveTag`, `context`) have no JSON form and no Rust form yet; `mentionUrl` and `tagUrl` templates work.
