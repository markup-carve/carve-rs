# Conformance

The crate passes every `.crv` / `.html` pair currently checked into its
`tests/spec` submodule. The conformance test suite includes an all-pairs gate so
new corpus pairs fail CI until the parser and renderer support them.
The table below samples early categories; the full list is `IMPLEMENTED` in
`tests/corpus.rs`.

| Pair | Construct | Status |
|---|---|---|
| 01-emphasis | `/italic/`, `*bold*`, `_underline_`, `~strike~`, `=hl=`, `{^super^}`, `{,sub,}`, `/*bi*/` | passing |
| 02-headings | `# H1` … `#### H4` | passing |
| 03-links | `[text](url)` | passing |
| 04-images | `![alt](src)` (inline + block) | passing |
| 05-lists | unordered (`- item`) | passing |
| 06-task-lists | `- [ ] todo`, `- [x] done` | passing |
| 07-blockquote-with-attribution | `> quote` + `^ Attribution` caption | passing |
| 08-image-with-caption | `![…](…)` + `^ caption` | passing |
| 09–10 | tables, rowspan/colspan | passing |
| 11-fenced-code | ` ``` ` blocks with language tag | passing |
| 12-inline-code | `` `code` `` | passing |
| 13-attributes | attribute blocks (`{#id .class}`) | passing |
| 14-frontmatter | YAML frontmatter | passing |
| 15-heading-ids | generated heading ids | passing |
| 16-reference-link | `[text][ref]` | passing |
| 17-collapsed-reference-link | `[text][]` | passing |
| 18-unresolved-reference-link | an unresolved `[text][ref]` stays literal | passing |
| 318-composite-figures | `::: figure` groups: panels, group caption on the closer, `Figure 2a` crossrefs | passing |

---

[Back to the README](../README.md)
