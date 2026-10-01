# Engine maintenance measurements, 2026-10-01

Historical main baseline: `5a83fdf4e12f002ffe6946a398d33804a3d23258`. Candidate: `quality/measured-maintenance-20261001`.

The PR was rebased onto main `9d8812547dfbdaa40784f32f357f8393f5139e5d`. Intervening commits changed release versions and notes, and the corpus pin; production parser and importer code did not change.

## Maintenance and verification

| Measure | Before | After |
| --- | ---: | ---: |
| Reference-resolution module lines, including tests | 515 | 401 |
| Catch-all child dispatch arms in the reference resolver | 2 | 0 |
| Separate verse definition-prepass state machines | 2 | 0 |

Reference resolution uses the existing exhaustive authored-child visitor. Its image hook handles inline, block, and figure images. Shared traversal reaches ruby content, citation suffixes, table cell blocks, short captions, and extension fallbacks; imported-AST tests verify these paths.

Verse ownership records consumed source lines during the block parser's structural pass. Internal lookahead fragments pause that recording. Openers remain visible to list-column tracking, while verse bodies remain opaque to definition extraction. Unclosed verse fences now count as open containers for lazy list continuation. A cursor caches the code-closer index; a quoted-run index replaces repeated scans for quoted closers.

Validation: 237 unit tests, 97 include tests, 61 performance tests, 7,176 integration tests, 30 documentation tests, and the include-conformance test passed; 3 integration tests were ignored. Clippy with warnings denied and formatting checks passed.

| Quoted opener lines | Prefix checks before | Prefix checks after |
| ---: | ---: | ---: |
| 128 | 18,744 | 2,360 |
| 256 | 70,200 | 4,664 |
| 512 | 271,416 | 9,272 |
| 1,024 | 1,067,064 | 18,488 |

At 1,024 lines, prefix work fell 57.7 times. Increasing input eight times increases final work 7.83 times, versus 56.9 times before. With definition-shaped lines present, the final counts are 3,175, 6,247, 12,391, and 24,679. The test caps eightfold growth at twelvefold work.

## Timing measurements

Three warmups and seven timed samples per case. The table shows median milliseconds for this engine. Both revisions run the same harness and inputs on this host. [maintenance-results.json](../benchmarks/maintenance-results.json) retains medians, minima, input bytes, and SHA-256 output hashes.

| Case | Size | Before ms | After ms | After / before | Same output hash |
| --- | ---: | ---: | ---: | ---: | --- |
| quoted_fences | 128 | 2.447 | 3.112 | 1.272 | yes |
| verse_definitions | 128 | 2.111 | 3.237 | 1.533 | no |
| paragraphs | 128 | 2.335 | 2.930 | 1.255 | yes |
| quoted_fences | 1024 | 23.834 | 5.450 | 0.229 | yes |
| verse_definitions | 1024 | 6.060 | 8.456 | 1.395 | no |
| paragraphs | 1024 | 5.843 | 6.542 | 1.120 | yes |
| quoted_fences | 4096 | 315.885 | 13.123 | 0.042 | yes |
| verse_definitions | 4096 | 17.235 | 22.529 | 1.307 | no |
| paragraphs | 4096 | 20.239 | 17.674 | 0.873 | yes |

Wall-clock results are local medians from a shared host. They are not CI thresholds or release performance guarantees. Compare before and after within this engine. Output hashes distinguish equivalent-output workloads from corrected behavior. The CLI measurements include process startup and serialization. Verse-definition hashes differ because the old engine registered literal definitions inside quoted verse. That row measures corrected behavior rather than an equivalent-output optimization.

## Reproduce

Build both binaries with `CARGO_PROFILE_DEV_OPT_LEVEL=2` and pass the absolute binary path to the candidate harness. Copy each binary before building the other revision when using a shared target directory. Run the counted regression with `cargo test --lib repeated_unclosed_quoted_fences_have_linear_work -- --nocapture`.

```sh
python3 benchmarks/maintenance.py /absolute/path/to/carve-binary
```

## Remaining maintenance

The five cases in [verse-oracle-cases.json](../benchmarks/verse-oracle-cases.json) include source and rendered results from all engines and executable spec commit `e12ed741313c16185375e6f17b0cc6fe3e4366c2`. TypeScript and Rust match all five cases after trimming outer whitespace. PHP matches the definition-after-verse case and disagrees on four existing verse cases. The prose grammar describes fence openers inside verse as ordinary text, while the executable oracle protects colon closers inside closed opaque spans. The TypeScript and Rust changes follow the executable oracle; that grammar disagreement remains explicit.

The parser still has substantial container and definition prepass code. These changes consolidate verse ownership and reference traversal; they do not replace every prepass. The structural pass adds work to documents containing verse and definitions.
