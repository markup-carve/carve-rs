# Contributing to carve-rs

Thanks for your interest in contributing.

## Getting started

```bash
git clone https://github.com/markup-carve/carve-rs
cd carve-rs

# The spec corpus is a submodule; the conformance tests need it.
git submodule update --init

cargo build --locked --all-targets
```

Without the submodule, corpus-driven tests fail with an error naming it rather
than skipping.

Two names to keep straight: the crate publishes as `carve-lang` because `carve`
was taken on crates.io, but Rust code imports it as `carve` and the CLI binary is
`carve`.

The minimum supported Rust version is **1.75**, set as `rust-version` in
`Cargo.toml`. CI compiles the whole tree against that floor on every pull
request, so a dependency that raises its own minimum is caught before merge.

## Running the tests

```bash
cargo install cargo-nextest      # once
cargo nextest run -E 'not binary(perf_regressions)'
cargo test --doc
```

**Use nextest. Plain `cargo test` is not a slower version of the same run.** The
suite is about 6100 test functions spread across some 670 separate test binaries.
Nextest schedules every test from every binary through one queue; `cargo test`
runs one binary at a time and parallelizes only inside it, so it alternates
between a short burst of work and a serialization point, several hundred times.
Measured: the whole suite finishes in about 5m44s under nextest, while a
`cargo test` run over the same tests was around 10% through at 25 minutes. That
is a different order of magnitude, not a longer wait.

Doctests need the second line because nextest does not run them, by design: they
need a compiler invocation per test rather than a binary to schedule.
`src/extensions/` carries worked examples as doctests, so dropping
`cargo test --doc` would stop testing them while the run stayed green.

To narrow a run, name the target: `cargo nextest run -E 'binary(corpus)'` or
`cargo test --test corpus`. One trap worth knowing: `cargo build --tests --test X`
does not narrow anything. `--tests` wins and builds every binary.

### The timing suite

`tests/perf_regressions.rs` asserts wall-clock ratios, which is why the filter
above excludes it. It runs in `.github/workflows/guards.yml` on a runner of its
own, on pushes to `main` and nightly, rather than on the pull-request path.

A reading taken on a loaded machine is void. When these tests shared a runner
with the rest of the suite, one of them reported 1.79x, its documented regression
signature, on commits that changed no engine code. Locally that means an
otherwise idle machine and `--test-threads 1`:

```bash
cargo nextest run -E 'binary(perf_regressions)' --test-threads 1
```

For a branch, ask CI instead:

```bash
gh workflow run guards.yml -R markup-carve/carve-rs --ref <branch>
```

## The other gates

CI runs each of these, and each is worth a local pass before pushing:

| Command | What it checks |
|---------|----------------|
| `cargo check --tests --all-features` | compiles, including the test targets a `--lib` check skips |
| `cargo fmt --all -- --check` | formatting |
| `cargo clippy --locked --all-targets -- -D warnings` | lints, with warnings fatal |
| `cargo build --locked --no-default-features` | the `fs` feature is the only code in the crate that opens a file |
| `python3 tools/sync-implemented.py --check` | whether a missing corpus category is a list problem or an engine one |
| `python3 tools/seam-matrix.py --check` | the seam matrix still covers every shape family |

`RUSTFLAGS: -D warnings` is set for the whole CI workflow, so a warning fails the
build there even outside the clippy job.

`tools/check-schema-map.py` checks the ProseMirror schema map against
carve-grammars. It needs a checkout of that repository, so it is easier to read
the CI job than to reproduce it locally. A declared divergence says why as one of
three reason kinds, two of which name the ProseMirror node they claim upstream
does or does not publish; the checker resolves that name on every run, so such a
reason goes false by itself. `python3 -m unittest discover -s tools -p 'test_*.py'`
covers that gate, with `CARVE_GRAMMARS_DIR` pointing at the checkout.

## Conformance and the corpus

Part of the suite is driven by the shared spec corpus in `tests/spec`, so a spec
bump can change expectations with no local edit.

A corpus category this engine does not support yet is declared rather than
quietly absent. `IMPLEMENTED` in `tests/corpus.rs` lists the categories that must
pass, `KNOWN_GAPS` beside it lists the ones deliberately deferred, and it is
empty today. The list is guarded from both directions: a corpus category missing
from `IMPLEMENTED` fails the build, and an `IMPLEMENTED` entry with no corpus
pair fails it too. Implementing a feature therefore means adding its category to
the list as well as making the parser handle it. `tools/sync-implemented.py`
without `--check` adds exactly the categories that already render byte-exact.

`docs/conformance.md` covers how the corpus runners are wired.

## Project layout

```
src/
├── lib.rs             # public API
├── main.rs            # the `carve` CLI
├── parse.rs, parse/   # block and inline parsing
├── ast.rs             # AST node types
├── render*.rs         # HTML, Markdown, plain text, ANSI, Carve
├── html_import.rs     # HTML, Markdown, Djot and BBCode importers
├── markdown_import.rs
├── extensions/        # opt-in extensions
├── prosemirror/       # editor bridge
├── includes.rs        # file inclusion (behind the `fs` feature)
├── profile.rs         # allowed constructs for untrusted input
└── source_patch.rs    # source-preserving patches
tests/                 # integration tests plus the corpus runners
tests/spec/            # git submodule: markup-carve/carve
tools/                 # the Python guards CI runs
```

## Bindings that pin this engine

carve-rb, carve-py, carve-wasm and carve-go each pin a carve-rs revision. If your
change moves the AST, the wire format or rendered output, those pins are the
downstream blast radius. `docs/development.md` describes the pin shapes and
`docs/engine-pin-guard.md` the guard that reads them, including what it cannot
see.

## Spec changes

This repository implements the language; it does not define it. Syntax and
semantics live in [markup-carve/carve](https://github.com/markup-carve/carve)
(`resources/grammar.ebnf` plus the corpus), and the
[versioning contract](https://markup-carve.github.io/carve/versioning) says what
a release may change. If your change would alter what valid Carve means, open the
discussion there first.

Rendered output is byte-identical across implementations by design, so a change
that alters it is rarely a single-repository change. Expect to pair it with
[carve-js](https://github.com/markup-carve/carve-js) and
[carve-php](https://github.com/markup-carve/carve-php), and check whether the
spec pins the behavior at all: where it does not, every implementation can agree
with the others and still be wrong together. Link the sibling PRs from your
description.

## Pull requests

- One logical change per PR, with a test that fails without it.
- Make the test able to fail. Revert the fix and watch it go red before you trust
  it; several bugs across the Carve engines survived behind a check that
  structurally could not see what it was checking.
- Tests, `fmt`, `clippy` and the MSRV check green.
- Say what behavior changed in the description. The reasoning belongs in the
  commit body.
