# HTML import fanout measurements

Footnote lookup uses lazy target counts, subtree intervals, href ranges and membership sets. Cleanup compacts sibling vectors once. Empty-code settlement, nested-span repair, hard-break spacing and blank-row removal use linear passes. Generated admonition title IDs use a live index of title ordinals and reserved names, updated when backlinks and earlier notes disappear.

## Method

Measured on 2026-10-08 against `3f9b1d2ed4e302d6222c2615618ab037c79b18de`. The candidate source hashes and Rust binary hashes are recorded in the [raw results](measurements/html-import-fanout-20261008.json).

Each public API with the Word adapter was measured serially on CPU 13 (AMD Ryzen 9 PRO 7940HS w/ Radeon 780M Graphics). Each size received one warmup and three timed calls. A second round reversed revision and size order, giving six samples per revision and size. The tables use all six samples. JavaScript and PHP collected garbage before each timed call. Report serialization, hashing and file I/O were outside the timer.

Runtimes: v22.22.2; PHP 8.5.11 (cli) (built: Sep 24 2026 13:49:29) (NTS); rustc 1.97.1 (8bab26f4f 2026-07-14). PHP used its default CLI JIT settings with PCOV and Xdebug disabled. These timings use a different PHP configuration from the benchmark site's tracing-JIT core runs.

PHP candidate measurements were refreshed after tightening the subclass hook guard: every size received the same six samples in both size orders, with the original before samples retained. The raw results record that phase separately. PHP ordinary-input and distinct-footnote controls were then repeated with both revisions together because the refresh ran at higher host load; the original control samples are also retained.

## Paired results

The table shows the larger size of each pair. Per-byte growth compares that size with the smaller size: linear work stays near 1x; quadratic work approaches the input-size multiplier. All 40 before/after output and report hashes match.

| Input | Count | API | Before ms | After ms | Speedup | After per-byte growth |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| Footnote admonition titles | 2,048 | ast | 941.201 | 31.067 | 30.30x | 1.19x |
| Ordinary admonition titles | 4,096 | ast | 8195.438 | 35.786 | 229.01x | 1.10x |
| Blank table rows | 32,768 | carve | 2205.319 | 148.891 | 14.81x | 1.30x |
| Long inverse backlink classes | 4,096 | carve | 22.985 | 18.731 | 1.23x | 1.21x |
| Long inverse reference classes | 4,096 | carve | 335.792 | 17.914 | 18.74x | 1.13x |
| Shared-body backlinks | 4,096 | carve | 1784.426 | 30.995 | 57.57x | 1.24x |
| Wrapped backlinks | 4,096 | carve | 3455.143 | 37.829 | 91.34x | 1.23x |
| Shared definition wrapper | 8,192 | ast | 4138.154 | 58.742 | 70.45x | 1.18x |
| Empty note wrappers | 4,096 | ast | 46.169 | 36.638 | 1.26x | 1.20x |
| Duplicate reference IDs | 4,096 | carve | 1719.409 | 26.471 | 64.95x | 1.14x |
| Aliases for one definition | 4,096 | carve | 58.587 | 29.935 | 1.96x | 1.31x |
| Separator siblings | 512 | ast | 68.594 | 0.552 | 124.25x | 0.92x |
| Deep scope wrappers | 2,048 | ast | 45.022 | 14.455 | 3.11x | 1.11x |
| Overlapping backlink blocks | 2,048 | ast | 921.797 | 31.468 | 29.29x | 1.27x |
| Deep aliases (depth rejection) | 2,048 | ast | 66.121 | 12.129 | 5.45x | 1.14x |
| Empty code spans | 16,384 | carve | 2316.737 | 45.385 | 51.05x | 1.47x |
| Nested text-bearing spans | 16,384 | carve | 5016.436 | 91.472 | 54.84x | 1.15x |
| Hard-break spacing | 16,384 | ast | 2504.725 | 73.312 | 34.17x | 1.17x |
| Distinct mutual footnotes | 1,024 | carve | 21.157 | 23.537 | 0.90x | 1.12x |
| Ordinary paragraphs | 1,024 | carve | 22.723 | 24.384 | 0.93x | 1.15x |

## Ordinary input

- 256 paragraphs: before 5.175 ms (4.875–5.260); after 5.322 ms (4.800–6.154).
- 1024 paragraphs: before 22.723 ms (21.093–23.639); after 24.384 ms (21.883–27.455).

These short samples include JIT warmup effects. The raw ranges should accompany any claim about ordinary-input overhead.

## Validation

295 library tests and 7,316 suite tests passed; five tests were ignored. Formatting and Clippy passed. All 17 timing guards passed, including ordinary and footnote admonition titles. Baseline guards used a separate Cargo target directory. All 173 semantic cases match. Claude reviewed the diff; findings were addressed and checked.

## Reproduce

The [fixture generator](measurements/html-import-fanout-fixtures.py) accepts a shape and count. Copy the probe into each checkout when comparing a historical revision. Use distinct Cargo target directories for Rust revisions.

```sh
mkdir -p /tmp/carve-footnote-fixtures
for n in 1024 4096; do
  python3 docs/measurements/html-import-fanout-fixtures.py backs "$n" > "/tmp/carve-footnote-fixtures/backs-$n.html"
done
CARGO_TARGET_DIR="$PWD/target-fanout-after" cargo build --release --manifest-path docs/measurements/html-import-fanout-worker/Cargo.toml
"$PWD/target-fanout-after/release/footnote-worker" backs 1024,4096 reproduction carve
```

Pin the process to the same CPU, repeat with reversed revision and size order, and compare output/report hashes. The semantic inputs are in [the fixture file](measurements/html-import-fanout-semantic-fixtures.json).

## Remaining costs

Attribute-heavy HTML still encounters quadratic duplicate attribute checks in upstream parsers. Deep div nesting also makes parse5 repeat scope scans. The deep-scope fixture uses object boundaries to isolate engine-owned ancestor lookup; it does not establish linear parsing of arbitrary deep HTML. These changes do not establish a core parse/render chart speedup.

The relevant upstream code is in [parse5](https://github.com/inikulin/parse5/blob/e65eae9a9dc27f1b7eb71868d245ca83070ccc4a/packages/parse5/lib/tokenizer/index.ts), [html5ever](https://github.com/servo/html5ever/blob/7760920edff08e6dfd4b62affee39d8084e9dc45/html5ever/src/tokenizer/mod.rs), and [Lexbor](https://github.com/lexbor/lexbor/blob/master/source/lexbor/html/tree.c).
