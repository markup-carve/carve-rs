# Command line


The crate ships a `carve` binary that reads Carve source from a file or stdin
and writes the rendered output to stdout. HTML is the default; pass a format
flag for Markdown, plain text, or ANSI-colored terminal output.

Prebuilt binaries are attached to every release for macOS (Apple silicon and
Intel), Linux (glibc on x86-64 and ARM64, musl on x86-64) and Windows, so the
CLI does not require a Rust toolchain:

```bash
brew install markup-carve/carve/carve       # macOS and Linux

# or take the archive for your platform straight from the release page and put
# `carve` on your PATH: https://github.com/markup-carve/carve-rs/releases
```

Each archive ships a `.sha256` sidecar next to it. From source instead:

```bash
cargo install --locked carve-lang           # from crates.io
cargo install --locked --path .             # from a checkout
```

`--locked` builds the dependency set in the crate's `Cargo.lock`, which is the
one the release was tested against; without it Cargo re-resolves and you get
whatever the registry serves that day.

Then:

```bash
carve README.crv > README.html      # HTML (default, interactive)
carve --static README.crv           # self-contained HTML (print / PDF / archival)
carve --markdown README.crv         # Markdown
carve --plain README.crv            # plain text
carve --ansi README.crv             # ANSI-colored terminal text
echo '# Hello' | carve              # render from stdin
carve merge base.crv ours.crv theirs.crv # structural three-way merge
```

The library exports `merge_ast`, `merge_ast_with_resolver`, `create_ast_patch`,
and `apply_ast_patch` for the same workflow over typed `Document` values. A
resolver can select base, ours, theirs, or a JSON-encoded replacement for each
conflict. `ast_patch_to_json` and `ast_patch_from_json` exchange the same
`{op,path,value}` wire format as the JS and PHP engines. The merge combines
independent field edits, insertions, deletions, and moves, while unresolved
ambiguous edits are returned as JSON-Pointer conflicts. Derived position
metadata is intentionally regenerated after serialization.

Other options:

```bash
carve --mention-url '/users/{name}' --tag-url '/topics/{name}' social.crv
carve --symbol 'rocket=🚀' --symbol 'tada=🎉' symbols.crv
carve --no-raw-html untrusted.crv   # escape =html raw blocks/spans
carve --safe --profile comment untrusted.crv   # and restrict which constructs are allowed
carve --help
```

`--html` / `--markdown` (`--md`) / `--plain` (`--plain-text`) / `--ansi` select
the format (last one wins). `--mention-url` / `--tag-url` build HTML links and
apply to HTML output only. `--no-raw-html` (alias `--safe`) escapes `=html` raw
blocks and spans instead of emitting them verbatim, which is the safe choice when
rendering untrusted input; it composes with every format and with `--profile`.
`--profile NAME` (`full` | `article` | `comment` | `minimal`) restricts which
constructs are allowed at all and caps input length, and `--profile-base-host`
gives its link policy a host to judge internal vs external links against; see
[Untrusted input](#untrusted-input). `--static` (vs the default `--interactive`) renders
self-contained HTML: interactive constructs flatten (a `::: details` becomes an
expanded `<section>`) and client-script visuals (mermaid / chart / math) degrade
to source. Pass `--extensions` to enable the bundled interactive extensions
(details, spoiler, mermaid, chart, math) so `--static` has something to flatten;
without it the CLI parses those words as plain containers. Supplying build
renderers for the diagrams/math requires the library API (`Options::with_mode` +
`with_renderers`); see `docs/extensions.md` and `examples/static_mode.rs`.

---

[Back to the README](../README.md)
