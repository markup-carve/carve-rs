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

## ORACLE-003 degraded childless inline nodes are not their source text

Clause: C2-H-165 ("every other inline node with no children as its source
text").

```carve
a $`m` b <https://a.b> c {#cc#}
```

With `profile: minimal` the oracle writes `a m b https://a.b c ` : the value of
the math node, the visible text of the autolink, and nothing for the editorial
comment (C2-H-165 names only `comment`, the source comment, as giving no
text). A display math paragraph `$$`m`` gives `<p>m</p>`.

Clause: `a $`m` b &lt;https://a.b&gt; c {#cc#}` and `<p>$$`m`</p>`. The
engine rebuilds the source text from the node, since the tree holds no source.

## ORACLE-004 the caption of a degraded figure loses its number

Clause: C2-H-121 (no render option changes the tree, so the caption keeps the
number resolution gave it), C2-H-143 (`caption_number` gives its number).

```carve
![i](a.png)
^ Figure #: cap
```

With `profile: comment` (no `figure`) the oracle writes `<p>[img: i]</p>` and
`<p>Figure : cap</p>`.

Clause: `<p>Figure 1: cap</p>`.

## ORACLE-005 `resolveTree` applies the profile to the tree

Clause: C2-H-121 ("No option of this section changes the tree, except that
`headingIds` controls a resolution result").

```carve
See [docs](https://x.org) and *bold*.
```

`resolveTree(parseTree(x), {profile: "minimal"})` returns a tree in which the
link is gone and its text stands as a second `text` node beside `See `, which
also breaks C2-A-040 (no two adjacent `text` nodes). The fixture trees of
`html-31` and `html-32` (tests/data/examples.json) are such trees, so those two
gate examples do not exercise the profile code of this renderer; the extra
fixture is generated with trees resolved without the render options.
