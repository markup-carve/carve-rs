# Linting


`carve::lint_carve` reports silent degradations - places where a document parses
and renders without error, but something the author wrote does not reach the
output. It returns a `Vec<LintWarning>`, each carrying a stable `rule` id shared
with carve-js and carve-php, a message, a 1-based line and column, and byte
offsets into the source you passed.

```rust
let warnings = carve::lint_carve("`c`{kbd}\n");
assert_eq!(warnings[0].rule, "semantic-attribute-outside-span");
```

The same check from the command line, which is what a CI gate wants:

```bash
carve lint docs/*.crv
```

```
docs/guide.crv:3:1 unattached-block-attribute — This block attribute reaches no block: ...
```

Exit codes are the interface, and the three-way split is deliberate: **0**
clean, **1** findings, **2** a file could not be read. Collapsing 2 into 1 would
report an unreadable path as a lint failure; collapsing it into 0 would pass a
build whose documents were never opened. A bad path is reported and skipped, so
one missing file in a glob still lets every other document be checked.

Reads stdin with no path, or with `-`, and reports under `<stdin>`.
`--extensions` is the only option it takes, because it is the only render option
the linter reads - every other flag is REFUSED with exit 2 rather than accepted
and ignored, so `carve lint --static` cannot exit 0 having linted with a flag
the caller believed was doing something.

The line format and the exit codes match carve-js's `carve lint` exactly, so a
script that parses one CLI parses the other. The `rule` id is shared across
engines by contract; the message PROSE is not - the same trigger reports the
same id everywhere, worded for each engine. The rule SETS also differ today:
`unattached-block-attribute` exists here and not in carve-js, and several of
carve-js's rules have no counterpart here.

The compact semantic span attribute rules (spec PART 9 §10):

| rule | fires on |
| --- | --- |
| `semantic-attribute-value-ignored` | a value on a reserved name that only selects a wrapper: `[x]{kbd="V"}` renders `<kbd>x</kbd>` and `V` reaches no output |
| `semantic-attribute-outside-span` | a reserved name anywhere other than an ordinary `[content]{attrs}` span, where it stays a raw attribute: `` `c`{kbd} `` renders `<code kbd="">c</code>` |

The composite-figure rules (spec PART 9 §4c):

| rule | fires on |
| --- | --- |
| `figure-group-opener-metadata` | a `::: figure` opener carrying a quoted title or a `[label]`, which stays a generic container - the group has no title or label slot |
| `figure-group-nested` | a bare `::: figure` opener inside an open group's body, which stays a generic container - groups do not nest |
| `figure-group-panel-number` | a `#` placeholder in a PANEL caption, which stays literal - panels are not sequence units |

Both are tier-aware. `abbr`, `time` and `kbd` are reserved in core; `samp`,
`var`, `cite` and `dfn` only become elements once the `SemanticSpan` extension
is registered, and until then they are ordinary attributes whose value reaches
the output intact. Pass the same `Options` you render with so the diagnostics
describe the output you will actually get:

```rust
let warnings = carve::lint_carve_with_options(source, &options);
```

`cite` on a block quote is a valid HTML URL attribute and is deliberately not
reported.

---

[Back to the README](../README.md)
