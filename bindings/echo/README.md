# Echo binding

This experimental binding exposes the conforming `carve-rs` HTML renderer to
the [Echo programming language](https://echoc.dev/) through a minimal C ABI.

Build the native bridge first:

```sh
CARGO_TARGET_DIR=bindings/echo/native/target \
  cargo build --release --manifest-path bindings/echo/native/Cargo.toml
```

Then build and run the Echo example. Echo requires `clang` for its final link:

```sh
cd bindings/echo/examples
echoc build --release --target render -o render
LD_LIBRARY_PATH=../native/target/release ./render # Linux
# DYLD_LIBRARY_PATH=../native/target/release ./render # macOS
```

The public Echo API is deliberately narrow:

```echo
string $html = carve::to_html($source);
```

Use `carve::try_to_html` to receive `result<string, carve::Error>` instead of
terminating on invalid UTF-8 or a native failure.

The `carve-echo` example accepts a `.crv` file and writes HTML:

```sh
echoc build --release --target carve-echo -o carve-echo
LD_LIBRARY_PATH=../native/target/release ./carve-echo document.crv
```

The Echo wrapper copies the returned HTML into an owned Echo `string`, then
immediately releases the Rust allocation. Invalid UTF-8 and native failures
terminate with an error; the underlying `carve::to_html` operation is otherwise
infallible.

Run `python3 bindings/echo/test_corpus.py` from the repository root after
checking out the `tests/spec` submodule to compare the Echo output with every
shared HTML fixture.
