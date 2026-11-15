# Oracle defects: resolve stage and HTML renderer

Inputs where the TypeScript engine contradicts a clause. The Rust engine
follows the clause; the extra fixture (`tests/data/extra.json`) carries the
clause's output as `expected` / `expectedTree` for these cases.

## ORACLE-001 `«` is not reversed in a heading slug

Clause: C2-H-145 ("Each glyph of the list `reversed` in
`data/heading-slug.json` is replaced by its ASCII source"); the list maps both
U+00AB and U+00BB to `"`.

```carve
# x→y «q»
```

Oracle: `<section id="x-y-«q">`. The `»` is reversed and then hyphenated
away, the `«` is kept.

Clause: `«` becomes `"`, which C2-H-146 turns into a hyphen run, so the slug is
`x-y-q`.

## ORACLE-002 no `data-source-line` on the paragraph of an include

Clause: C2-H-016 ("With `sourceLines` on, each block element carries
`data-source-line`"), C2-H-133 (an include renders as a paragraph) and
C2-A-182 (an include node has a position).

```carve
{{ x.crv }}
```

Oracle with `sourceLines`: `<p>{{ x.crv }}</p>`.

Clause: `<p data-source-line="1">{{ x.crv }}</p>`.
