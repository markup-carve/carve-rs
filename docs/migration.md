# Migrating into Carve

HTML, Markdown, Djot and BBCode all convert into Carve. Every importer supports
the shared report and `--check-loss`; HTML additionally takes a mode and an
adapter. Markdown, Djot, and BBCode report fidelity as unverified and fail a
loss check until their importers expose construct-level outcomes.

HTML migration is available through `html_to_ast` and `html_to_carve`. Both
return the value plus ordered loss diagnostics and accept safe, semantic, and
trusted-roundtrip policies. The CLI equivalent is:

```sh
carve migrate --from html --mode safe --report report.json input.html
```

A `<math>` element is read for the TeX it already carries: a `<semantics>`
annotation declaring `application/x-tex`, `text/x-tex` or `LaTeX`, else
`alttext` with the assumption reported. There is no MathML-to-TeX converter
here by decision (carve#1210 D6), so an element carrying neither is dropped
with a warning in `safe` and `semantic` and preserved verbatim in
`roundtrip` - its children are a token stream, and concatenating them reads
`<mfrac><mn>1</mn><mn>2</mn></mfrac>` as `12` rather than as one half.

`--adapter word` and `--adapter google-docs` add one recognition the `generic`
default does not risk: footnote-shaped HTML. A word processor writes a note as
a body anchor and a definition block that link to each other, and none of them
uses the `doc-noteref` / `doc-endnotes` roles a Carve engine writes, so under
`generic` a note arrives as a literal link beside an orphaned list. Under those
two adapters the pair is matched through the fragment each anchor addresses and
written back as a footnote reference and definition, whatever the ids are
called - Word's `_ftnref1`/`_ftn1`, Google Docs' `ftnt_ref1`/`ftnt1`,
LibreOffice's `sdfootnote1anc`/`sdfootnote1sym` and Pandoc's `fnref1`/`fn1` all
pair by the same rule. Back-links, the marker anchors they sit on, and the rule
separating the notes from the body are generated navigation and are dropped. A
reference whose target is missing stays a link, and a definition nothing
references stays ordinary content rather than becoming a definition that renders
as nothing. Name the adapter only for input you know came from that editor: on
arbitrary HTML a mutually linked anchor pair is not proof of a footnote, which
is why `generic` stays out.

Markdown migration is `markdown_to_ast`, `try_markdown_to_ast`,
`markdown_to_carve`, `try_markdown_to_carve`, or
`carve migrate --from markdown input.md`. The `try_` functions return an error
when nesting is too deep or Carve cannot write the document.
`try_migrate_markdown` also returns diagnostics. The older functions panic on
these errors. The importer parses the source to a tree and
writes it canonically, so the output is the document rather than the author's
spelling: a setext heading comes back as `#`, an indented code block as a
fence. It has no mode or adapter. `--report` emits a dropped/fallback
`fidelity-unverified` finding, so `--check-loss` deliberately exits 1. A blank
GFM table row is dropped with a `structure-unspellable` warning in the migration
report; `markdown_to_ast` and `markdown_to_carve` do not return diagnostics. An
ordered task item keeps its marker as text, as in `1. [x] done`, because Carve
spells a checkbox only behind a bullet. A bullet task item reads a box even when
its label names a link reference definition, which then goes unused: `- [x] done`
with `[x]: /u` beneath it imports as `- [x] done`, following cmark-gfm rather than
GitHub's endpoint.

Djot migration is `djot_to_carve`, or `carve migrate --from djot input.dj`. It
rewrites the delimiters that differ between the two languages. Like Markdown,
it reports unverified fidelity and fails closed under `--check-loss`.

BBCode migration is `bbcode_to_carve`, or
`carve migrate --from bbcode input.bbcode`. It converts forum formatting,
links, images, quotes, lists, code, spoilers and tables while keeping ordinary
Carve-looking source text literal. Inputs above 256 KiB are rejected because
the compatibility rewrite pipeline makes several bounded passes over a post.
It also reports unverified fidelity and fails closed under `--check-loss`.

An ordered task item has no Carve spelling either, so `html_to_carve` keeps the
bracket pair as text and reports a `structure-unspellable` warning:
`<ol><li><input type="checkbox" checked> done` becomes `1. [x] done`.
`html_to_ast` keeps the box on the item and reports nothing, because only a
writer loses it.

For HTML, `--check-loss` exits 1 for `degraded` or `dropped` findings. Opaque
raw HTML is degraded even when its bytes survive because it is not modeled or
editable. An `attribute-preserved` row is not a failure by itself, but it only
appears alongside the failing `raw-preserved` row.

---

[Back to the README](../README.md)
