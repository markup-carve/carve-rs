# Nested container costs

Baseline: `2745754ccd8790625eb29cc31643501590a8fa2d`. Candidate source hashes, raw timing
samples and host load are in [the observations](nested-runtime-costs.json).

Two fresh process rounds, baseline-candidate then candidate-baseline, using an isolated baseline build directory. Per fixture/mode: 150ms warmup, seven batches of at least 50ms; parsing includes result disposal, render reuses AST. Shared host; timing observations are not cross-language rankings.

The fixtures use `"> ".repeat(depth) + "end\n"` and `"- ".repeat(depth) + "end\n"`.
Reported output bytes include indentation, which grows with nesting depth.
These results do not establish linear cost in source bytes or a speed ranking
against other language implementations. They cover warm operations, excluding
process startup and fixture construction. PHP uses a clean INI without JIT or
profiling extensions; Rust uses a release build.

| Fixture | Depth | Operation | HTML bytes | Baseline ms, rounds 1 / 2 | Candidate ms, rounds 1 / 2 |
|---|---:|---|---:|---:|---:|
| quote | 48 | parse | 5722 | 0.041 / 0.042 | 0.043 / 0.043 |
| quote | 48 | render | 5722 | 0.020 / 0.020 | 0.019 / 0.020 |
| quote | 48 | html | 5722 | 0.058 / 0.089 | 0.051 / 0.075 |
| quote | 96 | parse | 20650 | 0.098 / 0.096 | 0.090 / 0.102 |
| quote | 96 | render | 20650 | 0.070 / 0.083 | 0.076 / 0.065 |
| quote | 96 | html | 20650 | 0.208 / 0.183 | 0.198 / 0.227 |
| quote | 192 | parse | 78154 | 0.251 / 0.220 | 0.224 / 0.228 |
| quote | 192 | render | 78154 | 0.479 / 0.459 | 0.459 / 0.455 |
| quote | 192 | html | 78154 | 0.938 / 1.008 | 0.988 / 0.893 |
| list | 48 | parse | 19107 | 0.491 / 0.461 | 0.360 / 0.368 |
| list | 48 | render | 19107 | 0.053 / 0.054 | 0.020 / 0.020 |
| list | 48 | html | 19107 | 0.511 / 0.502 | 0.371 / 0.345 |
| list | 96 | parse | 75075 | 1.797 / 1.702 | 1.293 / 1.320 |
| list | 96 | render | 75075 | 0.385 / 1.087 | 0.058 / 0.058 |
| list | 96 | html | 75075 | 2.776 / 2.734 | 1.340 / 1.483 |
| list | 192 | parse | 297603 | 6.413 / 6.277 | 4.778 / 5.364 |
| list | 192 | render | 297603 | 1.575 / 1.524 | 0.274 / 0.304 |
| list | 192 | html | 297603 | 7.924 / 7.636 | 5.131 / 5.456 |

## Reproduction

Run the same benchmark source against a baseline checkout and a candidate
checkout, reversing their order in the second round. The Rust example must be
copied into the baseline checkout first. The PHP script accepts the checkout
whose Composer autoloader it should use.

```sh
cargo run --release --example nested_costs
```

The benchmark checks parse/render and combined HTML agreement before timing.
Separate corpus comparisons check baseline/candidate HTML byte-for-byte.
