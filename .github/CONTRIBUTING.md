# Contributing to carve-rs

Thanks for your interest in contributing.

## Getting started

```bash
git clone https://github.com/markup-carve/carve-rs
cd carve-rs
cargo build --locked --all-targets
```

Two names to keep straight: the crate publishes as `carve-lang` because `carve`
was taken on crates.io, but Rust code imports it as `carve` and the CLI binary is
`carve`.

The minimum supported Rust version is **1.85** (edition 2024), set as
`rust-version` in `Cargo.toml`.

## Running the tests

```bash
cargo test -- --skip c2_fixed_oracle_corpus_and_clause_differences
cargo test --test corpus
```

The README examples run as doctests, so `cargo test --doc` covers them.

## The other gates

CI (`.github/workflows/engine.yml`) runs each of these:

| Command | What it checks |
|---------|----------------|
| `cargo fmt --check` | formatting, also with `--manifest-path bench/Cargo.toml` |
| `cargo clippy --all-targets --all-features -- -D warnings -W clippy::pedantic -W clippy::cognitive_complexity` | lints |
| `cargo clippy --lib --no-default-features` and `cargo test --no-default-features --test the_library_opens_no_file_without_the_fs_feature` | without `fs` the library holds no code that opens a file |
| `python3 tools/check_complexity.py` | at most 15 cyclomatic complexity per hand-written function (needs `lizard`) |
| `sh tools/consumer-smoke.sh` | the packaged crate in a downstream consumer |
| `cargo build --release --target wasm32-wasip1 --bin carve` and `node tools/wasi-smoke.mjs` | the WASI command that carve-go and hugo-carve run |

## Conformance and the spec data

`build.rs` generates the tables from `spec-data/`, a verbatim copy of
`spec/0.2/data/` of markup-carve/carve at the commit in
`spec-data/SOURCE_COMMIT`. `spec-conformance/` and `spec-trees/` copy its
`conformance/` and `trees/`. CI fails when a copy differs from the pinned
commit, so moving the pin is one commit that updates copies and pin together.

## Project layout

```
src/
├── lib.rs             # crate root; the public library is `api`
├── main.rs            # the `carve` CLI (needs the `fs` feature)
├── api/               # the calls of tooling.md, errors, writers, JSON boundary
├── reader/, block/, inline/   # parsing
├── resolve/           # references, notes, ids
├── html/              # HTML renderer
├── write/             # canonical Carve writer
├── import/            # HTML importer
├── include/           # include expansion; `include/fs.rs` behind `fs`
└── lint/              # lint rules
tests/                 # integration tests and their fixtures
spec-data/, spec-conformance/, spec-trees/   # pinned copies from the spec
bench/                 # timing program (own workspace)
consumer-smoke/        # downstream consumer check (own workspace)
tools/                 # CI helpers
```

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
- Tests, `fmt` and `clippy` green.
- Say what behavior changed in the description. The reasoning belongs in the
  commit body.
