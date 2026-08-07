# Checking a binding's engine pin

`tools/check-engine-pin.py` answers one question, in one place, for every
repository that pins this engine: **which carve-rs is this binding running, and
is that a revision that really exists on `main`?**

Four bindings pin carve-rs. Until markup-carve/carve-rs#771 only one of them
measured its pin at all, and only as a warning annotation that could never fail
a job. The rule had been written once per repository, so no single check covered
the class, and three of the four looked unpinned to anyone reading them - because
the crate publishes as **`carve-lang`**, not `carve` (the name `carve` was taken
on crates.io), so grepping a manifest for "carve" finds the binding's own package
and nothing else.

## The two shapes

| shape | where the revision lives | who uses it |
| --- | --- | --- |
| Cargo git dependency | `rev = "..."` in `Cargo.toml`, and again in `Cargo.lock` as `source = "git+...carve-rs?rev=..."` | carve-rb (`ext/carve/`), carve-py, carve-wasm |
| bare revision file | one 40-hex revision in a text file beside a committed artifact | carve-go (`internal/wasm/REV`) |

The reader is parameterized by path and form, so a fifth binding inherits the
guard instead of reinventing it.

## What it asserts

The obvious gate is "fail when the pin is behind `main`". Do not build that one.
carve-rs merges continuously, so it would be red from the moment any PR opens,
unclearable by the action it recommends, and the predictable end state is
someone raising the tolerance until it means nothing.

The inverse failure matters just as much. A gate whose only assertion is about
the distance stops asserting anything the moment the distance is zero, and a
healthy pin is the state this is trying to reach. **A gate that stops working
once its subject is healthy is not a gate** (markup-carve/carve#755). So the
load-bearing assertions are the ones that hold and can fail with the pin sitting
exactly on the engine's tip:

| check | holds that |
| --- | --- |
| `pin_present` | the pin file exists, is readable, and names the engine. Finding no engine dependency is a FAILURE, never a pass. |
| `pin_well_formed` | exactly one revision, 40 lowercase hex characters. An abbreviation resolves locally and then never matches the lockfile. |
| `lock_agrees` | (Cargo form) the lockfile's own `source` line names the same revision as the manifest, and the package it names is `carve-lang`. Read from the LOCK, not from the manifest twice. |
| `revision_exists` | it is a real commit in carve-rs. |
| `revision_on_branch` | it is an ancestor of `main`, so the artifact did not come from an unmerged or rewritten branch. |
| `artifact_digest` | (optional) the committed artifact hashes to the digest recorded beside the revision. |
| `pin_age` | (optional) the pin is not older than `--max-age-days`. |

The lag behind `main` is **printed as a number** and never gates. Age gates
instead, because age is something the actor controls: a pin older than N days is
cleared by bumping it, whereas a pin behind by zero commits is unreachable while
the engine is merging.

Exit codes: `0` every assertion holds, `1` an assertion failed, `2` usage or
setup error. Pass `--github` for `::error::` annotations.

## Wiring it into a binding's CI

Both forms need a carve-rs checkout with full history to compare against, so
`fetch-depth: 0` is required.

For a Cargo binding (carve-py and carve-wasm use the repository root; carve-rb
uses `ext/carve/`):

```yaml
  engine-pin:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Check out carve-rs
        uses: actions/checkout@v4
        with:
          repository: markup-carve/carve-rs
          path: carve-rs
          fetch-depth: 0

      - name: Check the pinned engine revision
        run: |
          python3 carve-rs/tools/check-engine-pin.py \
            --engine carve-rs --github --max-age-days 14 \
            --form cargo --manifest Cargo.toml --lock Cargo.lock
```

For a binding that records a revision beside a prebuilt artifact:

```yaml
      - name: Check the recorded engine revision
        run: |
          python3 carve-rs/tools/check-engine-pin.py \
            --engine carve-rs --github --max-age-days 14 \
            --form rev-file --file internal/wasm/REV
```

Add `--artifact internal/wasm/carve.wasm --artifact-digest
internal/wasm/carve.wasm.sha256` once the build script records the digest; see
below for why that is not assumed.

## What it cannot see

- **That a committed artifact was built from the revision recorded beside it**,
  unless a digest is recorded too. Rebuilding the artifact to compare would need
  the full toolchain in every job, and asserting the pairing without either would
  be a check that cannot fail - precisely the shape this replaces. The
  `artifact_digest` assertion is therefore made only when both `--artifact` and
  `--artifact-digest` are given, and a binding that wants it has to have its
  build script write the digest.
- **Whether the engine change the pin is missing matters.** That is what each
  binding's scheduled corpus run answers, and the two are complementary: the
  corpus run is blind to any engine change no corpus document exercises
  (markup-carve/carve-rs#449 is the concrete case), and this one is blind to
  what the difference between two real revisions means.
- **A revision on `main` that was later reverted.** It is still an ancestor, so
  it still passes.

## Tests

`tests/engine_pin_guard.rs` watches the guard fail once per assertion, against
throwaway git fixtures rather than the live repository, so "a revision that does
not exist" and "a revision that is not on `main`" are constructed rather than
waited for. Every case except `pin_age` runs with the pin **exactly on the engine
tip**, which is the property the ticket asked for: a healthy pin is not what
silences these checks.
