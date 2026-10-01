# Engine maintenance measurements, 2026-10-01

Baseline: `5a83fdf4e12f002ffe6946a398d33804a3d23258` on `main`. Candidate: `quality/measured-maintenance-20261001`.

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

Three warmups and seven samples per case for TypeScript and Rust; PHP uses two warmups and seven samples. The table shows median milliseconds. Both revisions run the same harness and inputs on this host. Results retain median, minimum, input bytes, and SHA-256 output hashes in [maintenance-results.json](../benchmarks/maintenance-results.json).

| Case | Size | Before ms | After ms | After / before | Same output hash |
| --- | ---: | ---: | ---: | ---: | --- |
| quoted_fences | 128 | 2.891 | 2.643 | 0.914 | yes |
| verse_definitions | 128 | 2.887 | 3.207 | 1.111 | no |
| paragraphs | 128 | 2.762 | 2.707 | 0.980 | yes |
| quoted_fences | 1024 | 23.383 | 4.279 | 0.183 | yes |
| verse_definitions | 1024 | 5.233 | 6.669 | 1.274 | no |
| paragraphs | 1024 | 5.253 | 5.277 | 1.005 | yes |
| quoted_fences | 4096 | 310.998 | 10.749 | 0.035 | yes |
| verse_definitions | 4096 | 16.408 | 19.781 | 1.206 | no |
| paragraphs | 4096 | 15.095 | 15.024 | 0.995 | yes |

Wall-clock results are local medians, not CI thresholds or release performance guarantees. The host was shared; small changes can reflect scheduler noise. Output hashes permit comparison without discarding behavior changes. TypeScript and PHP use in-process conversion; Rust uses the CLI, including process startup and serialization. Compare before and after within one engine, not absolute times across engines. Rust verse-definition hashes differ because the old engine registered literal definitions inside quoted verse; the candidate keeps them literal. That row measures corrected behavior rather than an equivalent-output optimization.

## Reproduce

Build both worktrees and install their locked dependencies. Pass each absolute worktree path to the candidate harness. For Rust, build both binaries with `CARGO_PROFILE_DEV_OPT_LEVEL=2` and copy each binary before building the other revision when using a shared target directory.

```sh
python3 benchmarks/maintenance.py /absolute/path/to/carve-binary
```

## Final review and remaining maintenance

Local Claude CLI reviewed the completed diff. Review findings were checked against source, tests, and the current executable spec. Traversal order, list-opener context, source-line ownership, cached lookahead, and importer validation costs were corrected during review.

The five cases in [verse-oracle-cases.json](../benchmarks/verse-oracle-cases.json) include source and rendered results from all engines and executable spec commit `e12ed741313c16185375e6f17b0cc6fe3e4366c2`. TypeScript and Rust match all five cases. PHP matches the definition-after-verse case and disagrees on four existing verse cases. The prose grammar describes fence openers inside verse as ordinary text, while the executable oracle protects colon closers inside closed opaque spans. The TypeScript and Rust changes follow the executable oracle; that grammar disagreement remains explicit.

The parser still has substantial container and definition prepass code. These changes consolidate verse ownership and reference traversal; they do not replace every prepass. The structural pass adds work to documents containing verse and definitions.
